//! OAuth 2.1 authorization server for the MCP endpoint.
//!
//! Public clients register themselves (RFC 7591), the signed-in dashboard session approves a
//! request on the consent page (the page talks to `/admin/oauth/authorize`), and the client
//! trades the single-use code plus its PKCE verifier for a short-lived access token and a
//! rotating refresh token. Codes and tokens are stored as SHA-256 hashes only.

use std::time::Duration;

use axum::Json;
use axum::http::StatusCode;
use axum::http::header::{CACHE_CONTROL, HeaderValue, PRAGMA};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use url::Url;
use uuid::Uuid;

use crate::error::AppError;
use crate::state::AppState;
use crate::util::{new_id, random_token, secure_eq, sha256_hex};

pub const ACCESS_TOKEN_PREFIX: &str = "prm_at_";
const REFRESH_TOKEN_PREFIX: &str = "prm_rt_";
pub const ACCESS_TTL_SECS: i64 = 3600;
const REFRESH_TTL_DAYS: i64 = 30;
const CODE_TTL_MINUTES: i64 = 10;
const MAX_CLIENTS: i64 = 1000;
const MAX_REDIRECT_URIS: usize = 5;
const MAX_NAME_CHARS: usize = 80;

// --- Errors in the OAuth wire format (RFC 6749 section 5.2) -------------------------------------

#[derive(Debug)]
pub struct OAuthError {
    pub status: StatusCode,
    pub error: &'static str,
    pub description: String,
}

pub type OResult<T> = Result<T, OAuthError>;

impl OAuthError {
    pub fn new(error: &'static str, description: impl Into<String>) -> Self {
        OAuthError {
            status: StatusCode::BAD_REQUEST,
            error,
            description: description.into(),
        }
    }

    pub fn invalid_request(description: impl Into<String>) -> Self {
        Self::new("invalid_request", description)
    }

    pub fn invalid_grant(description: impl Into<String>) -> Self {
        Self::new("invalid_grant", description)
    }

    pub fn invalid_client(description: impl Into<String>) -> Self {
        OAuthError {
            status: StatusCode::UNAUTHORIZED,
            ..Self::new("invalid_client", description)
        }
    }

    pub fn unavailable(description: impl Into<String>) -> Self {
        OAuthError {
            status: StatusCode::SERVICE_UNAVAILABLE,
            ..Self::new("temporarily_unavailable", description)
        }
    }
}

impl From<sqlx::Error> for OAuthError {
    fn from(e: sqlx::Error) -> Self {
        tracing::error!(error = ?e, "oauth database error");
        OAuthError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            ..Self::new("server_error", "internal server error")
        }
    }
}

impl IntoResponse for OAuthError {
    fn into_response(self) -> Response {
        let mut response = (
            self.status,
            Json(json!({ "error": self.error, "error_description": self.description })),
        )
            .into_response();
        no_store(&mut response);
        response
    }
}

pub fn no_store(response: &mut Response) {
    let headers = response.headers_mut();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(PRAGMA, HeaderValue::from_static("no-cache"));
}

// --- Scopes -------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Scopes {
    pub read: bool,
    pub write: bool,
    pub live: bool,
}

pub const SCOPES_SUPPORTED: [&str; 3] = ["read", "write", "live"];

impl Scopes {
    /// Known scopes only, `read` always included. Unknown names are ignored, as RFC 6749 allows.
    pub fn parse(raw: &str) -> Scopes {
        let mut s = Scopes {
            read: true,
            ..Scopes::default()
        };
        for name in raw.split_whitespace() {
            match name {
                "write" => s.write = true,
                "live" => s.live = true,
                _ => {}
            }
        }
        s
    }

    /// What a client gets when it names no scope: read and write, sandbox only.
    pub fn default_request() -> Scopes {
        Scopes {
            read: true,
            write: true,
            live: false,
        }
    }

    pub fn from_request(raw: Option<&str>) -> Scopes {
        match raw.map(str::trim).filter(|s| !s.is_empty()) {
            Some(raw) => Scopes::parse(raw),
            None => Scopes::default_request(),
        }
    }

    pub fn list(&self) -> Vec<&'static str> {
        let mut out = vec!["read"];
        if self.write {
            out.push("write");
        }
        if self.live {
            out.push("live");
        }
        out
    }

    pub fn to_scope_string(&self) -> String {
        self.list().join(" ")
    }

    pub fn is_subset_of(&self, other: &Scopes) -> bool {
        (!self.write || other.write) && (!self.live || other.live)
    }
}

// --- Redirect URIs, PKCE ------------------------------------------------------------------------

const BLOCKED_SCHEMES: [&str; 8] = [
    "javascript",
    "data",
    "file",
    "vbscript",
    "about",
    "blob",
    "ftp",
    "ws",
];

fn is_loopback(url: &Url) -> bool {
    matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
}

/// A redirect URI a client may register: `https`, `http` on a loopback host, or a custom scheme
/// such as `cursor://`. No fragment, no credentials.
pub fn validate_redirect_uri(raw: &str) -> Result<(), String> {
    if raw.is_empty() || raw.len() > 2048 {
        return Err("redirect_uris must be 1-2048 characters".into());
    }
    let url = Url::parse(raw).map_err(|_| format!("{raw} is not an absolute URI"))?;
    if url.fragment().is_some() {
        return Err("redirect_uris must not contain a fragment".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("redirect_uris must not contain credentials".into());
    }
    match url.scheme() {
        "https" => {
            if url.host_str().is_none() {
                return Err("an https redirect_uri needs a host".into());
            }
            Ok(())
        }
        "http" => {
            if is_loopback(&url) {
                Ok(())
            } else {
                Err("http redirect_uris are only allowed on localhost, 127.0.0.1 and [::1]".into())
            }
        }
        scheme if BLOCKED_SCHEMES.contains(&scheme) => Err(format!(
            "the {scheme}: scheme is not allowed in redirect_uris"
        )),
        _ => Ok(()),
    }
}

/// Exact match, except that a registered loopback `http` URI matches any port (RFC 8252 7.3).
pub fn redirect_matches(registered: &str, requested: &str) -> bool {
    if registered == requested {
        return true;
    }
    let (Ok(a), Ok(b)) = (Url::parse(registered), Url::parse(requested)) else {
        return false;
    };
    a.scheme() == "http"
        && b.scheme() == "http"
        && is_loopback(&a)
        && is_loopback(&b)
        && a.host_str() == b.host_str()
        && a.path() == b.path()
        && a.query() == b.query()
}

/// An S256 challenge is the unpadded base64url of a SHA-256 digest: exactly 43 characters.
pub fn valid_challenge(challenge: &str) -> bool {
    challenge.len() == 43
        && challenge
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub fn pkce_matches(verifier: &str, challenge: &str) -> bool {
    let valid = (43..=128).contains(&verifier.len())
        && verifier
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~'));
    valid
        && secure_eq(
            &URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
            challenge,
        )
}

fn same_resource(a: &str, b: &str) -> bool {
    a.trim_end_matches('/') == b.trim_end_matches('/')
}

fn clean_name(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(MAX_NAME_CHARS)
        .collect()
}

// --- Client registration ------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub client_name: Option<String>,
    pub redirect_uris: Option<Vec<String>>,
    pub token_endpoint_auth_method: Option<String>,
    pub grant_types: Option<Vec<String>>,
    pub response_types: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct RegisteredClient {
    pub client_id: String,
    pub client_id_issued_at: i64,
    pub client_name: String,
    pub redirect_uris: Vec<String>,
    pub grant_types: Vec<&'static str>,
    pub response_types: Vec<&'static str>,
    pub token_endpoint_auth_method: &'static str,
}

fn metadata_error(description: impl Into<String>) -> OAuthError {
    OAuthError::new("invalid_client_metadata", description)
}

pub async fn register_client(state: &AppState, req: RegisterRequest) -> OResult<RegisteredClient> {
    if let Some(method) = req.token_endpoint_auth_method.as_deref()
        && method != "none"
    {
        return Err(metadata_error(
            "only public clients are supported: token_endpoint_auth_method must be \"none\"",
        ));
    }
    if let Some(grants) = &req.grant_types
        && grants
            .iter()
            .any(|g| g != "authorization_code" && g != "refresh_token")
    {
        return Err(metadata_error(
            "grant_types may only contain authorization_code and refresh_token",
        ));
    }
    if let Some(types) = &req.response_types
        && types.iter().any(|t| t != "code")
    {
        return Err(metadata_error("response_types may only contain code"));
    }
    let uris = req.redirect_uris.unwrap_or_default();
    if uris.is_empty() || uris.len() > MAX_REDIRECT_URIS {
        return Err(OAuthError::new(
            "invalid_redirect_uri",
            format!("send 1-{MAX_REDIRECT_URIS} redirect_uris"),
        ));
    }
    let mut redirect_uris: Vec<String> = Vec::new();
    for uri in uris {
        validate_redirect_uri(&uri).map_err(|m| OAuthError::new("invalid_redirect_uri", m))?;
        if !redirect_uris.contains(&uri) {
            redirect_uris.push(uri);
        }
    }
    let client_name = {
        let name = clean_name(req.client_name.as_deref().unwrap_or_default());
        if name.is_empty() {
            "Unnamed client".to_string()
        } else {
            name
        }
    };

    prune_clients(state).await?;
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM oauth_clients")
        .fetch_one(&state.pool)
        .await?;
    if count >= MAX_CLIENTS {
        return Err(OAuthError::unavailable(
            "too many registered clients; try again later",
        ));
    }

    let client_id = format!("prc_{}", random_token::<16>());
    let issued: (DateTime<Utc>,) = sqlx::query_as(
        "INSERT INTO oauth_clients (client_id, client_name, redirect_uris)
         VALUES ($1, $2, $3) RETURNING created_at",
    )
    .bind(&client_id)
    .bind(&client_name)
    .bind(json!(redirect_uris))
    .fetch_one(&state.pool)
    .await?;
    Ok(RegisteredClient {
        client_id,
        client_id_issued_at: issued.0.timestamp(),
        client_name,
        redirect_uris,
        grant_types: vec!["authorization_code", "refresh_token"],
        response_types: vec!["code"],
        token_endpoint_auth_method: "none",
    })
}

/// Registered clients that never got a connection are dropped after a week.
async fn prune_clients(state: &AppState) -> Result<(), sqlx::Error> {
    sqlx::query(
        "DELETE FROM oauth_clients c
         WHERE c.created_at < now() - interval '7 days'
           AND NOT EXISTS (SELECT 1 FROM oauth_grants g WHERE g.client_id = c.client_id)",
    )
    .execute(&state.pool)
    .await?;
    Ok(())
}

// --- Authorization request (validated for the consent page) ------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct AuthorizeParams {
    pub response_type: Option<String>,
    pub client_id: String,
    pub redirect_uri: String,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
    pub state: Option<String>,
    pub scope: Option<String>,
    pub resource: Option<String>,
}

#[derive(Debug)]
pub struct ValidAuthorize {
    pub client_id: String,
    pub client_name: String,
    pub redirect: Url,
    pub requested: Scopes,
    pub challenge: String,
    pub resource: Option<String>,
    pub state: Option<String>,
}

/// Checks everything that must hold before the user sees a consent screen. An error here is
/// shown on the dashboard; the browser is never sent to an address that was not registered.
pub async fn validate_authorize(
    state: &AppState,
    p: &AuthorizeParams,
) -> Result<ValidAuthorize, AppError> {
    if p.response_type.as_deref() != Some("code") {
        return Err(AppError::bad_request("response_type must be \"code\""));
    }
    let row: Option<(String, serde_json::Value)> =
        sqlx::query_as("SELECT client_name, redirect_uris FROM oauth_clients WHERE client_id = $1")
            .bind(&p.client_id)
            .fetch_optional(&state.pool)
            .await?;
    let (client_name, uris) =
        row.ok_or_else(|| AppError::bad_request("Unknown client. Register it again."))?;
    let registered: Vec<String> = serde_json::from_value(uris).unwrap_or_default();
    if !registered
        .iter()
        .any(|r| redirect_matches(r, &p.redirect_uri))
    {
        return Err(AppError::bad_request(
            "redirect_uri is not registered for this client",
        ));
    }
    let redirect = Url::parse(&p.redirect_uri)
        .map_err(|_| AppError::bad_request("redirect_uri is not a valid URI"))?;
    let challenge = p.code_challenge.clone().unwrap_or_default();
    if !valid_challenge(&challenge) || p.code_challenge_method.as_deref() != Some("S256") {
        return Err(AppError::bad_request(
            "PKCE is required: send a code_challenge and code_challenge_method=S256",
        ));
    }
    if let Some(resource) = p.resource.as_deref()
        && !same_resource(resource, &state.mcp_url())
    {
        return Err(AppError::bad_request(format!(
            "resource must be {}",
            state.mcp_url()
        )));
    }
    if p.state.as_deref().is_some_and(|s| s.len() > 1024) {
        return Err(AppError::bad_request("state is too long"));
    }
    Ok(ValidAuthorize {
        client_id: p.client_id.clone(),
        client_name,
        redirect,
        requested: Scopes::from_request(p.scope.as_deref()),
        challenge,
        resource: p.resource.clone(),
        state: p.state.clone(),
    })
}

/// The user's decision as the address to send the browser to.
pub async fn decide(
    state: &AppState,
    v: &ValidAuthorize,
    approved: Option<Scopes>,
) -> Result<String, AppError> {
    let mut url = v.redirect.clone();
    match approved {
        Some(scopes) => {
            let code = random_token::<32>();
            sqlx::query(
                "INSERT INTO oauth_codes
                   (code_hash, client_id, redirect_uri, code_challenge, scopes, resource, expires_at)
                 VALUES ($1, $2, $3, $4, $5, $6, now() + make_interval(mins => $7))",
            )
            .bind(sha256_hex(&code))
            .bind(&v.client_id)
            .bind(v.redirect.as_str())
            .bind(&v.challenge)
            .bind(scopes.to_scope_string())
            .bind(&v.resource)
            .bind(CODE_TTL_MINUTES as i32)
            .execute(&state.pool)
            .await?;
            url.query_pairs_mut().append_pair("code", &code);
        }
        None => {
            url.query_pairs_mut().append_pair("error", "access_denied");
        }
    }
    {
        let mut q = url.query_pairs_mut();
        if let Some(s) = &v.state {
            q.append_pair("state", s);
        }
        q.append_pair("iss", &state.config.public_base_url);
    }
    Ok(url.to_string())
}

// --- Token endpoint -----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct TokenForm {
    pub grant_type: Option<String>,
    pub code: Option<String>,
    pub redirect_uri: Option<String>,
    pub client_id: Option<String>,
    pub code_verifier: Option<String>,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
    pub resource: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: &'static str,
    pub expires_in: i64,
    pub refresh_token: String,
    pub scope: String,
}

fn required<'a>(value: &'a Option<String>, name: &str) -> OResult<&'a str> {
    value
        .as_deref()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| OAuthError::invalid_request(format!("{name} is required")))
}

pub async fn token(state: &AppState, form: TokenForm) -> OResult<TokenResponse> {
    match form.grant_type.as_deref() {
        Some("authorization_code") => exchange_code(state, &form).await,
        Some("refresh_token") => refresh(state, &form).await,
        Some(other) => Err(OAuthError::new(
            "unsupported_grant_type",
            format!("grant_type {other} is not supported"),
        )),
        None => Err(OAuthError::invalid_request("grant_type is required")),
    }
}

#[derive(sqlx::FromRow)]
struct CodeRow {
    client_id: String,
    redirect_uri: String,
    code_challenge: String,
    scopes: String,
    resource: Option<String>,
    expires_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct RefreshRow {
    id: Uuid,
    client_id: String,
    scopes: String,
    refresh_hash: String,
    prev_refresh_hash: Option<String>,
    refresh_expires_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
}

async fn exchange_code(state: &AppState, form: &TokenForm) -> OResult<TokenResponse> {
    let code = required(&form.code, "code")?;
    let redirect_uri = required(&form.redirect_uri, "redirect_uri")?;
    let client_id = required(&form.client_id, "client_id")?;
    let verifier = required(&form.code_verifier, "code_verifier")?;

    let mut tx = state.pool.begin().await?;
    // Single use: the row is gone whatever happens next.
    let row: Option<CodeRow> = sqlx::query_as(
        "DELETE FROM oauth_codes WHERE code_hash = $1
         RETURNING client_id, redirect_uri, code_challenge, scopes, resource, expires_at",
    )
    .bind(sha256_hex(code))
    .fetch_optional(&mut *tx)
    .await?;
    let Some(CodeRow {
        client_id: code_client,
        redirect_uri: code_redirect,
        code_challenge: challenge,
        scopes,
        resource,
        expires_at,
    }) = row
    else {
        tx.commit().await?;
        return Err(OAuthError::invalid_grant("unknown or already used code"));
    };
    if expires_at < Utc::now() {
        tx.commit().await?;
        return Err(OAuthError::invalid_grant("the code has expired"));
    }
    if code_client != client_id {
        tx.commit().await?;
        return Err(OAuthError::invalid_grant(
            "the code was issued to another client",
        ));
    }
    if code_redirect != redirect_uri {
        tx.commit().await?;
        return Err(OAuthError::invalid_grant(
            "redirect_uri does not match the request",
        ));
    }
    if !pkce_matches(verifier, &challenge) {
        tx.commit().await?;
        return Err(OAuthError::invalid_grant(
            "code_verifier does not match the code_challenge",
        ));
    }
    if let (Some(asked), Some(bound)) = (form.resource.as_deref(), resource.as_deref())
        && !same_resource(asked, bound)
    {
        tx.commit().await?;
        return Err(OAuthError::new(
            "invalid_target",
            "resource does not match the request",
        ));
    }

    let scopes = Scopes::parse(&scopes);
    let grant_id = new_id();
    let refresh_token = format!("{REFRESH_TOKEN_PREFIX}{}", random_token::<32>());
    sqlx::query(
        "INSERT INTO oauth_grants (id, client_id, scopes, refresh_hash, refresh_expires_at)
         VALUES ($1, $2, $3, $4, now() + make_interval(days => $5))",
    )
    .bind(grant_id)
    .bind(client_id)
    .bind(scopes.to_scope_string())
    .bind(sha256_hex(&refresh_token))
    .bind(REFRESH_TTL_DAYS as i32)
    .execute(&mut *tx)
    .await?;
    let access_token = insert_access_token(&mut tx, grant_id).await?;
    tx.commit().await?;
    Ok(TokenResponse {
        access_token,
        token_type: "Bearer",
        expires_in: ACCESS_TTL_SECS,
        refresh_token,
        scope: scopes.to_scope_string(),
    })
}

async fn insert_access_token(
    tx: &mut sqlx::PgConnection,
    grant_id: Uuid,
) -> Result<String, sqlx::Error> {
    let token = format!("{ACCESS_TOKEN_PREFIX}{}", random_token::<32>());
    sqlx::query(
        "INSERT INTO oauth_access_tokens (token_hash, grant_id, expires_at)
         VALUES ($1, $2, now() + make_interval(secs => $3))",
    )
    .bind(sha256_hex(&token))
    .bind(grant_id)
    .bind(ACCESS_TTL_SECS as f64)
    .execute(&mut *tx)
    .await?;
    Ok(token)
}

async fn refresh(state: &AppState, form: &TokenForm) -> OResult<TokenResponse> {
    let presented = required(&form.refresh_token, "refresh_token")?;
    let client_id = required(&form.client_id, "client_id")?;
    let hash = sha256_hex(presented);

    let mut tx = state.pool.begin().await?;
    let row: Option<RefreshRow> = sqlx::query_as(
        "SELECT id, client_id, scopes, refresh_hash, prev_refresh_hash, refresh_expires_at, revoked_at
         FROM oauth_grants WHERE refresh_hash = $1 OR prev_refresh_hash = $1
         FOR UPDATE",
    )
    .bind(&hash)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(RefreshRow {
        id: grant_id,
        client_id: grant_client,
        scopes,
        refresh_hash: current,
        prev_refresh_hash: previous,
        refresh_expires_at: expires_at,
        revoked_at,
    }) = row
    else {
        tx.commit().await?;
        return Err(OAuthError::invalid_grant("unknown refresh token"));
    };
    if grant_client != client_id {
        tx.commit().await?;
        return Err(OAuthError::invalid_grant(
            "the refresh token belongs to another client",
        ));
    }
    if revoked_at.is_some() {
        tx.commit().await?;
        return Err(OAuthError::invalid_grant("this connection was revoked"));
    }
    if current != hash && previous.as_deref() == Some(hash.as_str()) {
        // An already rotated token came back: assume it leaked and end the connection.
        sqlx::query("UPDATE oauth_grants SET revoked_at = now() WHERE id = $1")
            .bind(grant_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Err(OAuthError::invalid_grant(
            "refresh token reuse detected; the connection was revoked",
        ));
    }
    if expires_at < Utc::now() {
        tx.commit().await?;
        return Err(OAuthError::invalid_grant("the refresh token has expired"));
    }

    let mut granted = Scopes::parse(&scopes);
    if let Some(raw) = form.scope.as_deref().filter(|s| !s.trim().is_empty()) {
        let asked = Scopes::parse(raw);
        if !asked.is_subset_of(&granted) {
            tx.commit().await?;
            return Err(OAuthError::new(
                "invalid_scope",
                "scope may not be wider than the original grant",
            ));
        }
        granted = asked;
    }
    let refresh_token = format!("{REFRESH_TOKEN_PREFIX}{}", random_token::<32>());
    sqlx::query(
        "UPDATE oauth_grants
         SET scopes = $2, prev_refresh_hash = refresh_hash, refresh_hash = $3,
             refresh_expires_at = now() + make_interval(days => $4), last_used_at = now()
         WHERE id = $1",
    )
    .bind(grant_id)
    .bind(granted.to_scope_string())
    .bind(sha256_hex(&refresh_token))
    .bind(REFRESH_TTL_DAYS as i32)
    .execute(&mut *tx)
    .await?;
    let access_token = insert_access_token(&mut tx, grant_id).await?;
    tx.commit().await?;
    Ok(TokenResponse {
        access_token,
        token_type: "Bearer",
        expires_in: ACCESS_TTL_SECS,
        refresh_token,
        scope: granted.to_scope_string(),
    })
}

// --- Bearer authentication for /mcp -------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct McpGrant {
    pub grant_id: Uuid,
    pub client_name: String,
    pub scopes: Scopes,
}

pub async fn authenticate(state: &AppState, token: &str) -> Result<Option<McpGrant>, AppError> {
    if !token.starts_with(ACCESS_TOKEN_PREFIX) {
        return Ok(None);
    }
    let row: Option<(Uuid, String, String)> = sqlx::query_as(
        "SELECT g.id, c.client_name, g.scopes
         FROM oauth_access_tokens t
         JOIN oauth_grants g ON g.id = t.grant_id
         JOIN oauth_clients c ON c.client_id = g.client_id
         WHERE t.token_hash = $1 AND t.expires_at > now() AND g.revoked_at IS NULL",
    )
    .bind(sha256_hex(token))
    .fetch_optional(&state.pool)
    .await?;
    let Some((grant_id, client_name, scopes)) = row else {
        return Ok(None);
    };
    let pool = state.pool.clone();
    tokio::spawn(async move {
        let _ = sqlx::query(
            "UPDATE oauth_grants SET last_used_at = now()
             WHERE id = $1 AND (last_used_at IS NULL OR last_used_at < now() - interval '1 minute')",
        )
        .bind(grant_id)
        .execute(&pool)
        .await;
    });
    Ok(Some(McpGrant {
        grant_id,
        client_name,
        scopes: Scopes::parse(&scopes),
    }))
}

// --- Connections (dashboard) --------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct GrantView {
    pub id: Uuid,
    pub client_id: String,
    pub client_name: String,
    pub scopes: Vec<&'static str>,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct GrantRow {
    id: Uuid,
    client_id: String,
    client_name: String,
    scopes: String,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
    expires_at: DateTime<Utc>,
}

pub async fn list_grants(state: &AppState) -> Result<Vec<GrantView>, AppError> {
    let rows: Vec<GrantRow> = sqlx::query_as(
        "SELECT g.id, g.client_id, c.client_name, g.scopes, g.created_at, g.last_used_at,
                g.refresh_expires_at AS expires_at
         FROM oauth_grants g JOIN oauth_clients c ON c.client_id = g.client_id
         WHERE g.revoked_at IS NULL AND g.refresh_expires_at > now()
         ORDER BY g.created_at DESC",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| GrantView {
            id: r.id,
            client_id: r.client_id,
            client_name: r.client_name,
            scopes: Scopes::parse(&r.scopes).list(),
            created_at: r.created_at,
            last_used_at: r.last_used_at,
            expires_at: r.expires_at,
        })
        .collect())
}

pub async fn revoke_grant(state: &AppState, id: Uuid) -> Result<(), AppError> {
    let res = sqlx::query(
        "UPDATE oauth_grants SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL",
    )
    .bind(id)
    .execute(&state.pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("connection"));
    }
    sqlx::query("DELETE FROM oauth_access_tokens WHERE grant_id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

// --- Housekeeping -------------------------------------------------------------------------------

pub async fn purge(state: &AppState) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM oauth_codes WHERE expires_at < now()")
        .execute(&state.pool)
        .await?;
    sqlx::query("DELETE FROM oauth_access_tokens WHERE expires_at < now()")
        .execute(&state.pool)
        .await?;
    sqlx::query(
        "DELETE FROM oauth_grants
         WHERE refresh_expires_at < now()
            OR (revoked_at IS NOT NULL AND revoked_at < now() - interval '7 days')",
    )
    .execute(&state.pool)
    .await?;
    prune_clients(state).await
}

pub async fn run_cleanup(state: AppState) {
    tokio::time::sleep(Duration::from_secs(60)).await;
    loop {
        if let Err(e) = purge(&state).await {
            tracing::error!(error = ?e, "oauth cleanup failed");
        }
        tokio::time::sleep(Duration::from_secs(3600)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_default_and_normalise() {
        let d = Scopes::from_request(None);
        assert_eq!(d.list(), vec!["read", "write"]);
        let all = Scopes::from_request(Some("live write read bogus"));
        assert_eq!(all.to_scope_string(), "read write live");
        // read is always there, even when only another scope is named
        assert_eq!(Scopes::parse("live").to_scope_string(), "read live");
        assert!(Scopes::parse("read").is_subset_of(&Scopes::parse("read write")));
        assert!(!Scopes::parse("live").is_subset_of(&Scopes::parse("read write")));
    }

    #[test]
    fn redirect_uri_rules() {
        for ok in [
            "https://claude.ai/api/mcp/auth_callback",
            "http://127.0.0.1:33418/callback",
            "http://localhost/cb",
            "http://[::1]:9/cb",
            "cursor://anysphere.cursor-retrieval/oauth/callback",
            "vscode://ms-vscode.mcp/callback",
        ] {
            assert_eq!(validate_redirect_uri(ok), Ok(()), "{ok}");
        }
        for bad in [
            "",
            "not a uri",
            "http://example.com/cb",
            "https://example.com/cb#frag",
            "https://user:pw@example.com/cb",
            "javascript:alert(1)",
            "data:text/html,hi",
            "file:///etc/passwd",
        ] {
            assert!(validate_redirect_uri(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn loopback_port_may_differ_everything_else_must_match() {
        assert!(redirect_matches(
            "http://127.0.0.1:1111/cb",
            "http://127.0.0.1:2222/cb"
        ));
        assert!(redirect_matches("https://a.test/cb", "https://a.test/cb"));
        assert!(!redirect_matches(
            "http://127.0.0.1:1111/cb",
            "http://127.0.0.1:2222/other"
        ));
        assert!(!redirect_matches(
            "http://127.0.0.1/cb",
            "http://localhost/cb"
        ));
        assert!(!redirect_matches(
            "https://a.test/cb",
            "https://a.test:444/cb"
        ));
        assert!(!redirect_matches("https://a.test/cb", "https://a.test/cb/"));
    }

    #[test]
    fn pkce_s256() {
        // RFC 7636 appendix B
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        assert!(valid_challenge(challenge));
        assert!(pkce_matches(verifier, challenge));
        assert!(!pkce_matches("short", challenge));
        assert!(!pkce_matches(&verifier.replace('d', "e"), challenge));
        assert!(!valid_challenge("plain-text-challenge"));
    }

    #[test]
    fn client_names_are_cleaned() {
        assert_eq!(clean_name("  Claude\u{0007} Code  "), "Claude Code");
        assert_eq!(clean_name(&"x".repeat(200)).chars().count(), MAX_NAME_CHARS);
    }
}
