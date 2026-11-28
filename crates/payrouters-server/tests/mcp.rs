//! OAuth and MCP end to end through the real router and a real Postgres. The browser step of the
//! flow (the signed-in dashboard approving on the consent page) is the admin API call
//! `POST /admin/oauth/authorize`, made here with the admin token exactly as the page makes it.

use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, HeaderName, Request, StatusCode};
use axum::routing::post;
use payrouters_server::services::accounts;
use payrouters_server::{AppState, Config, api};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use url::Url;

const ADMIN: &str = "adm-test-token-0123456789";
const REDIRECT: &str = "http://127.0.0.1:7777/callback";
// RFC 7636 appendix B
const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

struct App {
    router: Router,
    state: AppState,
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    json: Value,
    text: String,
}

impl App {
    async fn new(pool: PgPool) -> App {
        Self::with_config(pool, |_| {}).await
    }

    async fn with_config(pool: PgPool, edit: impl FnOnce(&mut Config)) -> App {
        let mut config = Config {
            database_url: String::new(),
            listen_addr: "127.0.0.1:0".parse().unwrap(),
            public_base_url: "http://pay.test".into(),
            admin_token: ADMIN.into(),
            master_key: [1u8; 32],
            web_dir: "/nonexistent".into(),
            timezone: "Asia/Jakarta".parse().unwrap(),
            trusted_proxy_header: None,
            rate: Default::default(),
        };
        edit(&mut config);
        let state = AppState::new(pool, config).unwrap();
        accounts::seed_mock(&state).await.unwrap();
        App {
            router: api::router(state.clone()),
            state,
        }
    }

    async fn raw(
        &self,
        method: &str,
        uri: &str,
        bearer: Option<&str>,
        extra: &[(&str, &str)],
        content_type: &str,
        body: Option<String>,
    ) -> Reply {
        let mut b = Request::builder().method(method).uri(uri);
        if let Some(t) = bearer {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        for (k, v) in extra {
            b = b.header(*k, *v);
        }
        let req = match body {
            Some(text) => b
                .header("content-type", content_type)
                .body(Body::from(text))
                .unwrap(),
            None => b.body(Body::empty()).unwrap(),
        };
        let resp = self.router.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = axum::body::to_bytes(resp.into_body(), 4 << 20)
            .await
            .unwrap();
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let json = serde_json::from_str(&text).unwrap_or(Value::Null);
        Reply {
            status,
            headers,
            json,
            text,
        }
    }

    async fn call(
        &self,
        method: &str,
        uri: &str,
        bearer: Option<&str>,
        body: Option<Value>,
    ) -> Reply {
        self.raw(
            method,
            uri,
            bearer,
            &[],
            "application/json",
            body.map(|b| b.to_string()),
        )
        .await
    }

    async fn admin(&self, method: &str, uri: &str, body: Option<Value>) -> Reply {
        self.call(method, uri, Some(ADMIN), body).await
    }

    async fn form(&self, fields: &[(&str, &str)], extra: &[(&str, &str)]) -> Reply {
        let mut ser = url::form_urlencoded::Serializer::new(String::new());
        for (k, v) in fields {
            ser.append_pair(k, v);
        }
        self.raw(
            "POST",
            "/oauth/token",
            None,
            extra,
            "application/x-www-form-urlencoded",
            Some(ser.finish()),
        )
        .await
    }

    async fn register(&self) -> String {
        let r = self
            .call(
                "POST",
                "/oauth/register",
                None,
                Some(json!({ "client_name": "Test Client", "redirect_uris": [REDIRECT] })),
            )
            .await;
        assert_eq!(r.status, StatusCode::CREATED, "{}", r.text);
        assert_eq!(r.json["token_endpoint_auth_method"], "none");
        r.json["client_id"].as_str().unwrap().to_string()
    }

    fn authorize_body(client_id: &str, scope: &str, decision: &str) -> Value {
        json!({
            "response_type": "code",
            "client_id": client_id,
            "redirect_uri": REDIRECT,
            "code_challenge": CHALLENGE,
            "code_challenge_method": "S256",
            "state": "st-1",
            "scope": scope,
            "resource": "http://pay.test/mcp",
            "decision": decision,
        })
    }

    /// The consent page's POST. Returns the address the browser is sent to.
    async fn approve(&self, client_id: &str, scope: &str) -> Url {
        let r = self
            .admin(
                "POST",
                "/admin/oauth/authorize",
                Some(Self::authorize_body(client_id, scope, "allow")),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.text);
        Url::parse(r.json["redirect_url"].as_str().unwrap()).unwrap()
    }

    async fn exchange(&self, client_id: &str, code: &str) -> Reply {
        self.form(
            &[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", REDIRECT),
                ("client_id", client_id),
                ("code_verifier", VERIFIER),
            ],
            &[],
        )
        .await
    }

    /// Register, approve with `scope` and exchange: access and refresh tokens plus the client id.
    async fn connect(&self, scope: &str) -> Connection {
        let client_id = self.register().await;
        let redirect = self.approve(&client_id, scope).await;
        let code = query_of(&redirect, "code");
        let t = self.exchange(&client_id, &code).await;
        assert_eq!(t.status, StatusCode::OK, "{}", t.text);
        Connection {
            client_id,
            access: t.json["access_token"].as_str().unwrap().to_string(),
            refresh: t.json["refresh_token"].as_str().unwrap().to_string(),
            scope: t.json["scope"].as_str().unwrap().to_string(),
        }
    }

    async fn rpc(&self, token: &str, method: &str, params: Value) -> Reply {
        self.call(
            "POST",
            "/mcp",
            Some(token),
            Some(json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params })),
        )
        .await
    }

    /// `tools/call` result object (`content`, `structuredContent`, `isError`).
    async fn tool(&self, token: &str, name: &str, args: Value) -> Value {
        let r = self
            .rpc(
                token,
                "tools/call",
                json!({ "name": name, "arguments": args }),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.text);
        assert!(r.json["error"].is_null(), "{}", r.text);
        r.json["result"].clone()
    }

    async fn product_with_keys(&self, name: &str, webhook_url: Option<&str>) -> (String, String) {
        let mut body = json!({ "name": name });
        if let Some(u) = webhook_url {
            body["webhook_url"] = json!(u);
        }
        let p = self.admin("POST", "/admin/products", Some(body)).await;
        assert_eq!(p.status, StatusCode::CREATED, "{}", p.text);
        let id = p.json["id"].as_str().unwrap().to_string();
        let mut sandbox = String::new();
        for mode in ["sandbox", "live"] {
            let k = self
                .admin(
                    "POST",
                    &format!("/admin/products/{id}/api-keys"),
                    Some(json!({ "mode": mode, "name": "t" })),
                )
                .await;
            assert_eq!(k.status, StatusCode::CREATED, "{}", k.text);
            if mode == "sandbox" {
                sandbox = k.json["key"].as_str().unwrap().to_string();
            }
        }
        (id, sandbox)
    }
}

struct Connection {
    client_id: String,
    access: String,
    refresh: String,
    scope: String,
}

fn query_of(url: &Url, key: &str) -> String {
    url.query_pairs()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
        .unwrap_or_else(|| panic!("no {key} in {url}"))
}

fn text_of(result: &Value) -> String {
    result["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

#[sqlx::test(migrations = "./migrations")]
async fn discovery_documents_and_cors(pool: PgPool) {
    let app = App::new(pool).await;

    let resource = app
        .raw(
            "GET",
            "/.well-known/oauth-protected-resource/mcp",
            None,
            &[("origin", "https://claude.ai")],
            "",
            None,
        )
        .await;
    assert_eq!(resource.status, StatusCode::OK);
    assert_eq!(resource.json["resource"], "http://pay.test/mcp");
    assert_eq!(resource.json["authorization_servers"][0], "http://pay.test");
    assert_eq!(resource.headers["access-control-allow-origin"], "*");

    let root = app
        .call("GET", "/.well-known/oauth-protected-resource", None, None)
        .await;
    assert_eq!(root.json["resource"], "http://pay.test/mcp");

    for path in [
        "/.well-known/oauth-authorization-server",
        "/.well-known/openid-configuration",
    ] {
        let meta = app.call("GET", path, None, None).await;
        assert_eq!(meta.status, StatusCode::OK, "{path}");
        assert_eq!(meta.json["issuer"], "http://pay.test");
        assert_eq!(
            meta.json["authorization_endpoint"],
            "http://pay.test/oauth/authorize"
        );
        assert_eq!(meta.json["token_endpoint"], "http://pay.test/oauth/token");
        assert_eq!(
            meta.json["registration_endpoint"],
            "http://pay.test/oauth/register"
        );
        assert_eq!(
            meta.json["code_challenge_methods_supported"],
            json!(["S256"])
        );
        assert_eq!(
            meta.json["token_endpoint_auth_methods_supported"],
            json!(["none"])
        );
    }

    // A browser-based client sends a preflight before registering or exchanging a code.
    let preflight = app
        .raw(
            "OPTIONS",
            "/oauth/token",
            None,
            &[
                ("origin", "https://app.example.com"),
                ("access-control-request-method", "POST"),
                ("access-control-request-headers", "content-type"),
            ],
            "",
            None,
        )
        .await;
    assert!(preflight.status.is_success(), "{}", preflight.status);
    assert_eq!(preflight.headers["access-control-allow-origin"], "*");
    let mcp_preflight = app
        .raw(
            "OPTIONS",
            "/mcp",
            None,
            &[
                ("origin", "https://app.example.com"),
                ("access-control-request-method", "POST"),
                (
                    "access-control-request-headers",
                    "authorization, mcp-protocol-version",
                ),
            ],
            "",
            None,
        )
        .await;
    assert!(
        mcp_preflight.status.is_success(),
        "{}",
        mcp_preflight.status
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn mcp_needs_a_token_and_points_at_the_metadata(pool: PgPool) {
    let app = App::new(pool).await;
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" });

    let none = app.call("POST", "/mcp", None, Some(body.clone())).await;
    assert_eq!(none.status, StatusCode::UNAUTHORIZED);
    let challenge = none.headers["www-authenticate"].to_str().unwrap();
    assert!(
        challenge.starts_with(
            "Bearer resource_metadata=\"http://pay.test/.well-known/oauth-protected-resource/mcp\""
        ),
        "{challenge}"
    );

    let bad = app
        .call(
            "POST",
            "/mcp",
            Some("prm_at_not-a-real-token"),
            Some(body.clone()),
        )
        .await;
    assert_eq!(bad.status, StatusCode::UNAUTHORIZED);
    assert!(
        bad.headers["www-authenticate"]
            .to_str()
            .unwrap()
            .contains("error=\"invalid_token\"")
    );

    // The admin token and product keys are not MCP credentials.
    let admin = app.call("POST", "/mcp", Some(ADMIN), Some(body)).await;
    assert_eq!(admin.status, StatusCode::UNAUTHORIZED);

    for method in ["GET", "DELETE"] {
        let r = app.call(method, "/mcp", None, None).await;
        assert_eq!(r.status, StatusCode::METHOD_NOT_ALLOWED, "{method}");
        assert_eq!(r.headers["allow"], "POST");
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn registration_validates_redirect_uris(pool: PgPool) {
    let app = App::new(pool).await;
    for bad in [
        "http://example.com/cb",
        "javascript:alert(1)",
        "https://example.com/cb#frag",
        "not a uri",
    ] {
        let r = app
            .call(
                "POST",
                "/oauth/register",
                None,
                Some(json!({ "client_name": "x", "redirect_uris": [bad] })),
            )
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{bad}");
        assert_eq!(r.json["error"], "invalid_redirect_uri", "{bad}");
        assert_eq!(r.headers["cache-control"], "no-store");
    }
    let none = app
        .call(
            "POST",
            "/oauth/register",
            None,
            Some(json!({ "client_name": "x" })),
        )
        .await;
    assert_eq!(none.json["error"], "invalid_redirect_uri");
    let secret_client = app
        .call(
            "POST",
            "/oauth/register",
            None,
            Some(json!({ "redirect_uris": [REDIRECT], "token_endpoint_auth_method": "client_secret_post" })),
        )
        .await;
    assert_eq!(secret_client.json["error"], "invalid_client_metadata");

    // custom schemes and https are fine
    let ok = app
        .call(
            "POST",
            "/oauth/register",
            None,
            Some(json!({ "client_name": "Cursor", "redirect_uris": ["cursor://anysphere.cursor-retrieval/oauth/callback", "https://claude.ai/api/mcp/auth_callback"] })),
        )
        .await;
    assert_eq!(ok.status, StatusCode::CREATED, "{}", ok.text);
    assert!(ok.json["client_id"].as_str().unwrap().starts_with("prc_"));
}

#[sqlx::test(migrations = "./migrations")]
async fn consent_preview_and_decisions_are_validated(pool: PgPool) {
    let app = App::new(pool).await;
    let client_id = app.register().await;
    let query = |extra: &str| {
        format!(
            "/admin/oauth/authorize?response_type=code&client_id={client_id}&redirect_uri={}&code_challenge={CHALLENGE}&code_challenge_method=S256&state=abc{extra}",
            url::form_urlencoded::byte_serialize(REDIRECT.as_bytes()).collect::<String>()
        )
    };

    // the consent page needs the signed-in session
    let anon = app.call("GET", &query(""), None, None).await;
    assert_eq!(anon.status, StatusCode::UNAUTHORIZED);

    let preview = app.admin("GET", &query(""), None).await;
    assert_eq!(preview.status, StatusCode::OK, "{}", preview.text);
    assert_eq!(preview.json["client"]["client_name"], "Test Client");
    assert_eq!(preview.json["redirect_host"], "127.0.0.1:7777");
    assert_eq!(preview.json["resource"], "http://pay.test/mcp");
    let scopes = preview.json["scopes"].as_array().unwrap();
    let by = |k: &str| scopes.iter().find(|s| s["key"] == k).unwrap();
    // no scope named: read and write pre-checked, live off, read locked
    assert_eq!(by("read")["locked"], true);
    assert_eq!(by("write")["granted"], true);
    assert_eq!(by("live")["granted"], false);

    let asked = app.admin("GET", &query("&scope=read%20live"), None).await;
    let scopes = asked.json["scopes"].as_array().unwrap();
    let live = scopes.iter().find(|s| s["key"] == "live").unwrap();
    let write = scopes.iter().find(|s| s["key"] == "write").unwrap();
    assert_eq!(live["granted"], true);
    assert_eq!(write["granted"], false);

    // problems the page must show instead of redirecting
    for (name, uri, needle) in [
        (
            "unknown client",
            query("").replace(&client_id, "prc_unknown"),
            "Unknown client",
        ),
        (
            "redirect not registered",
            query("").replace("7777%2Fcallback", "7777%2Fother"),
            "not registered",
        ),
        (
            "no PKCE",
            query("").replace(CHALLENGE, "short"),
            "PKCE is required",
        ),
        (
            "plain method",
            query("").replace("S256", "plain"),
            "PKCE is required",
        ),
        (
            "wrong resource",
            query("&resource=https%3A%2F%2Fevil.example%2Fmcp"),
            "resource must be",
        ),
        (
            "wrong response type",
            query("").replace("response_type=code", "response_type=token"),
            "response_type",
        ),
    ] {
        let r = app.admin("GET", &uri, None).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{name}: {}", r.text);
        assert!(
            r.json["error"]["message"]
                .as_str()
                .unwrap()
                .contains(needle),
            "{name}: {}",
            r.text
        );
    }

    // a loopback redirect may use another port than the registered one
    let other_port = REDIRECT.replace("7777", "40001");
    let mut body = App::authorize_body(&client_id, "read", "allow");
    body["redirect_uri"] = json!(other_port);
    let ok = app
        .admin("POST", "/admin/oauth/authorize", Some(body))
        .await;
    assert_eq!(ok.status, StatusCode::OK, "{}", ok.text);
    assert!(
        ok.json["redirect_url"]
            .as_str()
            .unwrap()
            .starts_with(&other_port)
    );

    // deny goes back to the client with access_denied and the state, never a code
    let denied = app
        .admin(
            "POST",
            "/admin/oauth/authorize",
            Some(App::authorize_body(&client_id, "read write", "deny")),
        )
        .await;
    let url = Url::parse(denied.json["redirect_url"].as_str().unwrap()).unwrap();
    assert_eq!(query_of(&url, "error"), "access_denied");
    assert_eq!(query_of(&url, "state"), "st-1");
    assert_eq!(query_of(&url, "iss"), "http://pay.test");
    assert!(url.query_pairs().all(|(k, _)| k != "code"));

    let weird = app
        .admin(
            "POST",
            "/admin/oauth/authorize",
            Some(App::authorize_body(&client_id, "read", "maybe")),
        )
        .await;
    assert_eq!(weird.status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn code_exchange_is_bound_to_pkce_client_and_redirect_and_is_single_use(pool: PgPool) {
    let app = App::new(pool).await;
    let client_id = app.register().await;

    let code = query_of(&app.approve(&client_id, "read write").await, "code");
    let ok = app.exchange(&client_id, &code).await;
    assert_eq!(ok.status, StatusCode::OK, "{}", ok.text);
    assert_eq!(ok.json["token_type"], "Bearer");
    assert_eq!(ok.json["expires_in"], 3600);
    assert_eq!(ok.json["scope"], "read write");
    assert_eq!(ok.headers["cache-control"], "no-store");
    assert!(
        ok.json["access_token"]
            .as_str()
            .unwrap()
            .starts_with("prm_at_")
    );
    assert!(
        ok.json["refresh_token"]
            .as_str()
            .unwrap()
            .starts_with("prm_rt_")
    );

    // replay of the same code fails
    let again = app.exchange(&client_id, &code).await;
    assert_eq!(again.status, StatusCode::BAD_REQUEST);
    assert_eq!(again.json["error"], "invalid_grant");

    // wrong verifier: refused, and the code is burnt even for the right verifier afterwards
    let code = query_of(&app.approve(&client_id, "read").await, "code");
    let wrong = app
        .form(
            &[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("redirect_uri", REDIRECT),
                ("client_id", &client_id),
                (
                    "code_verifier",
                    "a-verifier-that-is-long-enough-but-wrong-0123456789",
                ),
            ],
            &[],
        )
        .await;
    assert_eq!(wrong.json["error"], "invalid_grant");
    assert_eq!(
        app.exchange(&client_id, &code).await.json["error"],
        "invalid_grant"
    );

    // another client cannot redeem it
    let other = app.register().await;
    let code = query_of(&app.approve(&client_id, "read").await, "code");
    assert_eq!(
        app.exchange(&other, &code).await.json["error"],
        "invalid_grant"
    );

    // a different redirect_uri than the request used
    let code = query_of(&app.approve(&client_id, "read").await, "code");
    let r = app
        .form(
            &[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("redirect_uri", "http://127.0.0.1:7777/elsewhere"),
                ("client_id", &client_id),
                ("code_verifier", VERIFIER),
            ],
            &[],
        )
        .await;
    assert_eq!(r.json["error"], "invalid_grant");

    // an expired code
    let code = query_of(&app.approve(&client_id, "read").await, "code");
    sqlx::query("UPDATE oauth_codes SET expires_at = now() - interval '1 second'")
        .execute(&app.state.pool)
        .await
        .unwrap();
    assert_eq!(
        app.exchange(&client_id, &code).await.json["error"],
        "invalid_grant"
    );

    let bad_type = app.form(&[("grant_type", "password")], &[]).await;
    assert_eq!(bad_type.json["error"], "unsupported_grant_type");
    let missing = app.form(&[("grant_type", "authorization_code")], &[]).await;
    assert_eq!(missing.json["error"], "invalid_request");
}

#[sqlx::test(migrations = "./migrations")]
async fn refresh_rotates_and_a_replayed_token_revokes_the_connection(pool: PgPool) {
    let app = App::new(pool).await;
    let c = app.connect("read write").await;

    let first = app
        .form(
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", &c.refresh),
                ("client_id", &c.client_id),
            ],
            &[],
        )
        .await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.text);
    let new_refresh = first.json["refresh_token"].as_str().unwrap().to_string();
    let new_access = first.json["access_token"].as_str().unwrap().to_string();
    assert_ne!(new_refresh, c.refresh);
    assert_eq!(
        app.rpc(&new_access, "ping", json!({})).await.status,
        StatusCode::OK
    );

    // wider scope than granted
    let wide = app
        .form(
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", &new_refresh),
                ("client_id", &c.client_id),
                ("scope", "read write live"),
            ],
            &[],
        )
        .await;
    assert_eq!(wide.json["error"], "invalid_scope");

    // another client cannot use it
    let other = app.register().await;
    let stolen = app
        .form(
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", &new_refresh),
                ("client_id", &other),
            ],
            &[],
        )
        .await;
    assert_eq!(stolen.json["error"], "invalid_grant");

    // the already rotated token comes back: the whole connection is revoked
    let replay = app
        .form(
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", &c.refresh),
                ("client_id", &c.client_id),
            ],
            &[],
        )
        .await;
    assert_eq!(replay.json["error"], "invalid_grant");
    assert!(
        replay.json["error_description"]
            .as_str()
            .unwrap()
            .contains("reuse")
    );
    assert_eq!(
        app.rpc(&new_access, "ping", json!({})).await.status,
        StatusCode::UNAUTHORIZED
    );
    let after = app
        .form(
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", &new_refresh),
                ("client_id", &c.client_id),
            ],
            &[],
        )
        .await;
    assert_eq!(after.json["error"], "invalid_grant");
}

#[sqlx::test(migrations = "./migrations")]
async fn connections_are_listed_and_revoking_ends_them(pool: PgPool) {
    let app = App::new(pool).await;
    let c = app.connect("read").await;
    assert_eq!(c.scope, "read");

    let list = app.admin("GET", "/admin/oauth/grants", None).await;
    assert_eq!(list.status, StatusCode::OK);
    let grants = list.json["data"].as_array().unwrap();
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0]["client_name"], "Test Client");
    assert_eq!(grants[0]["scopes"], json!(["read"]));
    let id = grants[0]["id"].as_str().unwrap().to_string();

    assert_eq!(
        app.rpc(&c.access, "ping", json!({})).await.status,
        StatusCode::OK
    );
    let del = app
        .admin("DELETE", &format!("/admin/oauth/grants/{id}"), None)
        .await;
    assert_eq!(del.status, StatusCode::NO_CONTENT);
    assert_eq!(
        app.rpc(&c.access, "ping", json!({})).await.status,
        StatusCode::UNAUTHORIZED
    );
    let gone = app
        .form(
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", &c.refresh),
                ("client_id", &c.client_id),
            ],
            &[],
        )
        .await;
    assert_eq!(gone.json["error"], "invalid_grant");
    let empty = app.admin("GET", "/admin/oauth/grants", None).await;
    assert!(empty.json["data"].as_array().unwrap().is_empty());
    assert_eq!(
        app.admin("DELETE", &format!("/admin/oauth/grants/{id}"), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );

    // grants are an admin matter
    assert_eq!(
        app.call("GET", "/admin/oauth/grants", None, None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.call("GET", "/admin/oauth/grants", Some(&c.access), None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );

    // an expired access token stops working
    let c = app.connect("read").await;
    sqlx::query("UPDATE oauth_access_tokens SET expires_at = now() - interval '1 second'")
        .execute(&app.state.pool)
        .await
        .unwrap();
    assert_eq!(
        app.rpc(&c.access, "ping", json!({})).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn protocol_handshake_resources_and_prompts(pool: PgPool) {
    let app = App::new(pool).await;
    let c = app.connect("read write").await;

    let init = app
        .rpc(&c.access, "initialize", json!({ "protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": { "name": "t", "version": "1" } }))
        .await;
    assert_eq!(init.json["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(init.json["result"]["serverInfo"]["name"], "payrouters");
    assert!(init.json["result"]["capabilities"]["tools"].is_object());
    assert!(
        init.json["result"]["instructions"]
            .as_str()
            .unwrap()
            .contains("get_integration_status")
    );
    let unknown = app
        .rpc(
            &c.access,
            "initialize",
            json!({ "protocolVersion": "1999-01-01" }),
        )
        .await;
    assert_eq!(unknown.json["result"]["protocolVersion"], "2025-06-18");

    // notifications get 202 and no body
    let note = app
        .call(
            "POST",
            "/mcp",
            Some(&c.access),
            Some(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })),
        )
        .await;
    assert_eq!(note.status, StatusCode::ACCEPTED);
    assert!(note.text.is_empty());

    let nope = app.rpc(&c.access, "nope/missing", json!({})).await;
    assert_eq!(nope.json["error"]["code"], -32601);
    let garbage = app
        .raw(
            "POST",
            "/mcp",
            Some(&c.access),
            &[],
            "application/json",
            Some("{not json".into()),
        )
        .await;
    assert_eq!(garbage.status, StatusCode::BAD_REQUEST);
    assert_eq!(garbage.json["error"]["code"], -32700);

    // batches
    let batch = app
        .call(
            "POST",
            "/mcp",
            Some(&c.access),
            Some(json!([
                { "jsonrpc": "2.0", "id": 1, "method": "ping" },
                { "jsonrpc": "2.0", "method": "notifications/initialized" },
                { "jsonrpc": "2.0", "id": 2, "method": "ping" }
            ])),
        )
        .await;
    assert_eq!(batch.json.as_array().unwrap().len(), 2);

    let tools = app.rpc(&c.access, "tools/list", json!({})).await;
    let list = tools.json["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = list.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for expected in [
        "get_integration_status",
        "get_env_template",
        "list_products",
        "list_provider_accounts",
        "list_payment_methods",
        "list_payments",
        "get_payment",
        "list_webhook_deliveries",
        "list_inbound_webhook_events",
        "get_stats_summary",
        "get_docs",
        "generate_code",
        "create_product",
        "update_product",
        "get_product_payment_options",
        "set_product_payment_options",
        "rotate_webhook_secret",
        "create_api_key",
        "revoke_api_key",
        "create_test_payment",
        "simulate_payment",
        "sync_payment",
        "retry_webhook_delivery",
        "test_webhook_integration",
        "wait_for_payment",
        "run_integration_selftest",
    ] {
        assert!(names.contains(&expected), "missing tool {expected}");
    }
    for t in list {
        assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
        assert!(
            t["description"].as_str().unwrap().len() > 40,
            "{}",
            t["name"]
        );
        assert!(t["annotations"]["readOnlyHint"].is_boolean());
    }
    let unknown_tool = app
        .rpc(
            &c.access,
            "tools/call",
            json!({ "name": "drop_database", "arguments": {} }),
        )
        .await;
    assert_eq!(unknown_tool.json["error"]["code"], -32602);

    // resources are the docs with this deployment's URLs filled in
    let resources = app.rpc(&c.access, "resources/list", json!({})).await;
    assert_eq!(
        resources.json["result"]["resources"]
            .as_array()
            .unwrap()
            .len(),
        11
    );
    let read = app
        .rpc(
            &c.access,
            "resources/read",
            json!({ "uri": "payrouters://docs/overview" }),
        )
        .await;
    let text = read.json["result"]["contents"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("http://pay.test/v1") || text.contains("`http://pay.test`"),
        "{text}"
    );
    assert!(!text.contains("{{"), "unfilled placeholder in overview");
    let missing = app
        .rpc(
            &c.access,
            "resources/read",
            json!({ "uri": "payrouters://docs/nothing" }),
        )
        .await;
    assert_eq!(missing.json["error"]["code"], -32002);
    for topic in [
        "overview",
        "authentication",
        "create-payment",
        "checkout-and-methods",
        "payment-status",
        "webhooks",
        "errors-and-limits",
        "sandbox",
        "testing",
        "providers",
    ] {
        let doc = app
            .tool(&c.access, "get_docs", json!({ "topic": topic }))
            .await;
        assert_eq!(doc["isError"], false, "{topic}");
        assert!(
            !text_of(&doc).contains("{{"),
            "{topic} has an unfilled placeholder"
        );
    }
    assert_eq!(
        app.tool(&c.access, "get_docs", json!({ "topic": "nope" }))
            .await["isError"],
        true
    );

    // prompts
    let prompts = app.rpc(&c.access, "prompts/list", json!({})).await;
    assert_eq!(
        prompts.json["result"]["prompts"].as_array().unwrap().len(),
        4
    );
    let prompt = app
        .rpc(&c.access, "prompts/get", json!({ "name": "integrate_payrouters", "arguments": { "language": "PHP", "framework": "Laravel" } }))
        .await;
    let msg = prompt.json["result"]["messages"][0]["content"]["text"]
        .as_str()
        .unwrap();
    assert!(msg.contains("PHP, Laravel") && msg.contains("get_integration_status"));
}

#[sqlx::test(migrations = "./migrations")]
async fn a_read_only_connection_cannot_change_anything(pool: PgPool) {
    let app = App::new(pool).await;
    let c = app.connect("read").await;
    let (product_id, _) = app.product_with_keys("Shop", None).await;

    for (name, args) in [
        ("create_product", json!({ "name": "X" })),
        (
            "update_product",
            json!({ "product_id": product_id, "name": "Y" }),
        ),
        ("rotate_webhook_secret", json!({ "product_id": product_id })),
        (
            "create_api_key",
            json!({ "product_id": product_id, "name": "k" }),
        ),
        (
            "set_product_payment_options",
            json!({ "product_id": product_id, "options": [] }),
        ),
        (
            "create_test_payment",
            json!({ "product_id": product_id, "amount": 10000 }),
        ),
        (
            "test_webhook_integration",
            json!({ "product_id": product_id }),
        ),
    ] {
        let r = app.tool(&c.access, name, args).await;
        assert_eq!(r["isError"], true, "{name}");
        assert!(
            text_of(&r).contains("can only read"),
            "{name}: {}",
            text_of(&r)
        );
    }
    // and nothing was created
    let products = app.admin("GET", "/admin/products", None).await;
    assert_eq!(products.json["data"].as_array().unwrap().len(), 1);
    // reading still works
    assert_eq!(
        app.tool(&c.access, "list_products", json!({})).await["isError"],
        false
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn without_the_live_permission_everything_stays_in_sandbox(pool: PgPool) {
    let app = App::new(pool).await;
    let (product_id, _sandbox_key) = app
        .product_with_keys("Shop", Some("https://shop.test/hook"))
        .await;
    let c = app.connect("read write").await;

    // live keys are hidden
    let listed = app.tool(&c.access, "list_products", json!({})).await;
    let keys = listed["structuredContent"]["data"][0]["api_keys"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0]["mode"], "sandbox");

    // live is refused with a way forward
    for (name, args) in [
        (
            "create_api_key",
            json!({ "product_id": product_id, "name": "k", "mode": "live" }),
        ),
        ("list_payments", json!({ "mode": "live" })),
        ("get_stats_summary", json!({ "mode": "live" })),
        ("get_env_template", json!({ "mode": "live" })),
        ("list_payment_methods", json!({ "mode": "live" })),
        (
            "create_test_payment",
            json!({ "product_id": product_id, "amount": 10000, "mode": "live" }),
        ),
    ] {
        let r = app.tool(&c.access, name, args).await;
        assert_eq!(r["isError"], true, "{name}");
        assert!(
            text_of(&r).contains("live permission"),
            "{name}: {}",
            text_of(&r)
        );
    }

    // a product with live traffic is read-only for this connection
    for (name, args) in [
        (
            "update_product",
            json!({ "product_id": product_id, "webhook_url": "https://evil.test/hook" }),
        ),
        ("rotate_webhook_secret", json!({ "product_id": product_id })),
        (
            "set_product_payment_options",
            json!({ "product_id": product_id, "options": [] }),
        ),
    ] {
        let r = app.tool(&c.access, name, args).await;
        assert_eq!(r["isError"], true, "{name}");
        assert!(
            text_of(&r).contains("live traffic"),
            "{name}: {}",
            text_of(&r)
        );
    }
    let product = app.admin("GET", "/admin/products", None).await;
    assert_eq!(
        product.json["data"][0]["webhook_url"],
        "https://shop.test/hook"
    );

    // the status tool does not mention live accounts or keys
    let status = app
        .tool(&c.access, "get_integration_status", json!({}))
        .await;
    let data = &status["structuredContent"];
    assert_eq!(data["live_access"], false);
    assert_eq!(data["products"][0]["active_keys"], json!({ "sandbox": 1 }));
    assert!(
        data["provider_accounts"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["mode"] == "sandbox")
    );

    // with the live permission the same calls work
    let live = app.connect("read write live").await;
    assert_eq!(live.scope, "read write live");
    let key = app
        .tool(
            &live.access,
            "create_api_key",
            json!({ "product_id": product_id, "name": "prod", "mode": "live" }),
        )
        .await;
    assert_eq!(key["isError"], false, "{}", text_of(&key));
    assert!(
        key["structuredContent"]["key"]
            .as_str()
            .unwrap()
            .starts_with("prk_live_")
    );
    let rotate = app
        .tool(
            &live.access,
            "rotate_webhook_secret",
            json!({ "product_id": product_id }),
        )
        .await;
    assert_eq!(rotate["isError"], false);
    assert!(
        rotate["structuredContent"]["webhook_secret"]
            .as_str()
            .unwrap()
            .starts_with("whsec_")
    );
    let all_keys = app.tool(&live.access, "list_products", json!({})).await;
    assert_eq!(
        all_keys["structuredContent"]["data"][0]["api_keys"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn an_assistant_can_set_up_a_product_and_run_a_payment_without_guessing(pool: PgPool) {
    let app = App::new(pool).await;
    let c = app.connect("read write").await;
    let t = &c.access;

    // empty system: the status says what to do first
    let status = app.tool(t, "get_integration_status", json!({})).await;
    let steps = status["structuredContent"]["next_steps"]
        .as_array()
        .unwrap();
    assert!(
        steps[0].as_str().unwrap().contains("create_product"),
        "{steps:?}"
    );
    assert!(
        status["structuredContent"]["provider_accounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["provider"] == "mock")
    );

    // create the product and its key; secrets come back once, with the instruction to store them
    let created = app
        .tool(t, "create_product", json!({ "name": "Kelas Coding", "webhook_url": "https://kelas.example.id/hooks/payrouters" }))
        .await;
    assert_eq!(created["isError"], false, "{}", text_of(&created));
    let product = &created["structuredContent"];
    let product_id = product["id"].as_str().unwrap().to_string();
    assert!(
        product["webhook_secret"]
            .as_str()
            .unwrap()
            .starts_with("whsec_")
    );
    assert!(text_of(&created).contains("Shown once"));

    let key = app
        .tool(
            t,
            "create_api_key",
            json!({ "product_id": product_id, "name": "backend" }),
        )
        .await;
    let api_key = key["structuredContent"]["key"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(api_key.starts_with("prk_test_"));

    // that key really works against the product API
    let direct = app
        .call(
            "POST",
            "/v1/payments",
            Some(&api_key),
            Some(json!({ "order_id": "KEY-CHECK", "amount": 12000, "provider": "mock", "method": "va_bca" })),
        )
        .await;
    assert_eq!(direct.status, StatusCode::CREATED, "{}", direct.text);

    // env template carries the real base URL and the key prefix, never the key
    let env = app
        .tool(t, "get_env_template", json!({ "product_id": product_id }))
        .await;
    let env_text = text_of(&env);
    assert!(
        env_text.contains("PAYROUTERS_BASE_URL=http://pay.test"),
        "{env_text}"
    );
    assert!(env_text.contains(&api_key[..13]), "{env_text}");
    assert!(
        !env_text.contains(&api_key),
        "the full key must not be shown again"
    );
    assert!(env_text.contains("https://kelas.example.id/hooks/payrouters"));

    // valid values to pick from
    let methods = app.tool(t, "list_payment_methods", json!({})).await;
    let providers = methods["structuredContent"]["providers"]
        .as_array()
        .unwrap();
    assert!(
        providers
            .iter()
            .any(|p| p["provider"] == "mock" && p["methods"].as_array().unwrap().len() >= 5)
    );

    // code with the right base URL
    for task in ["create_payment", "verify_webhook", "get_payment"] {
        for language in ["curl", "typescript", "python", "php"] {
            let code = app
                .tool(
                    t,
                    "generate_code",
                    json!({ "task": task, "language": language }),
                )
                .await;
            assert_eq!(code["isError"], false, "{task}/{language}");
            let body = code["structuredContent"]["code"].as_str().unwrap();
            assert!(!body.contains("{{"), "{task}/{language}");
            if task != "verify_webhook" {
                assert!(body.contains("http://pay.test"), "{task}/{language}");
            } else {
                assert!(
                    body.contains("sha256") || body.contains("SHA256") || body.contains("sha256"),
                    "{task}/{language}"
                );
            }
        }
    }
    let bad_lang = app
        .tool(
            t,
            "generate_code",
            json!({ "task": "create_payment", "language": "cobol" }),
        )
        .await;
    assert_eq!(bad_lang["isError"], true);

    // a test payment, settled through the simulator, readable by order id
    let payment = app
        .tool(t, "create_test_payment", json!({ "product_id": product_id, "amount": 150000, "provider": "mock", "method": "va_bca", "order_id": "MCP-1" }))
        .await;
    assert_eq!(payment["isError"], false, "{}", text_of(&payment));
    let payment_id = payment["structuredContent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        payment["structuredContent"]["snap_url"]
            .as_str()
            .unwrap()
            .starts_with("http://pay.test/snap/")
    );
    let paid = app
        .tool(
            t,
            "simulate_payment",
            json!({ "payment_id": payment_id, "outcome": "paid" }),
        )
        .await;
    assert_eq!(paid["structuredContent"]["status"], "paid");

    let by_order = app
        .tool(t, "get_payment", json!({ "order_id": "MCP-1" }))
        .await;
    assert_eq!(by_order["structuredContent"]["payment"]["status"], "paid");
    assert!(
        by_order["structuredContent"]["cash_in"]["net_amount"]
            .as_i64()
            .unwrap()
            > 0
    );
    assert!(
        !by_order["structuredContent"]["deliveries"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let list = app
        .tool(t, "list_payments", json!({ "status": "paid" }))
        .await;
    let rows = list["structuredContent"]["data"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["order_id"], "MCP-1");
    assert_eq!(rows[0]["product"], "Kelas Coding");
    assert!(rows[0].get("instructions").is_none(), "rows are compact");

    let deliveries = app
        .tool(
            t,
            "list_webhook_deliveries",
            json!({ "payment_id": payment_id }),
        )
        .await;
    let d = &deliveries["structuredContent"]["data"][0];
    assert_eq!(d["event_type"], "payment.paid");
    assert!(d.get("payload").is_none());

    let stats = app.tool(t, "get_stats_summary", json!({})).await;
    assert_eq!(stats["structuredContent"]["range"]["mode"], "sandbox");
    assert_eq!(stats["structuredContent"]["cash_in"]["count"], 1);

    // errors explain themselves
    let dup = app
        .tool(t, "create_test_payment", json!({ "product_id": product_id, "amount": 99000, "provider": "mock", "method": "va_bca", "order_id": "MCP-1" }))
        .await;
    assert_eq!(dup["isError"], true);
    assert!(
        text_of(&dup).contains("order_id_conflict"),
        "{}",
        text_of(&dup)
    );
    let bad_args = app
        .tool(
            t,
            "create_test_payment",
            json!({ "product_id": product_id }),
        )
        .await;
    assert_eq!(bad_args["isError"], true);
    let bad_uuid = app
        .tool(t, "get_payment", json!({ "payment_id": "nope" }))
        .await;
    assert!(text_of(&bad_uuid).contains("UUID"));

    // revoke
    let key_id =
        app.admin("GET", "/admin/products", None).await.json["data"][0]["api_keys"][0]["id"]
            .as_str()
            .unwrap()
            .to_string();
    let revoked = app
        .tool(
            t,
            "revoke_api_key",
            json!({ "product_id": product_id, "key_id": key_id }),
        )
        .await;
    assert_eq!(revoked["isError"], false);
    let after = app.call("GET", "/v1/payments", Some(&api_key), None).await;
    assert_eq!(after.status, StatusCode::UNAUTHORIZED);
}

type Hits = Arc<Mutex<Vec<(HeaderMap, String)>>>;

async fn spawn_receiver(status: Arc<AtomicU16>) -> (String, Hits) {
    let hits: Hits = Default::default();
    let store = hits.clone();
    let app = Router::new().route(
        "/hook",
        post(move |headers: HeaderMap, body: String| {
            let store = store.clone();
            let status = status.clone();
            async move {
                store.lock().unwrap().push((headers, body));
                StatusCode::from_u16(status.load(Ordering::SeqCst)).unwrap()
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}/hook"), hits)
}

#[sqlx::test(migrations = "./migrations")]
async fn webhook_integration_test_reports_what_the_endpoint_answered(pool: PgPool) {
    let app = App::new(pool).await;
    let c = app.connect("read write").await;
    let status = Arc::new(AtomicU16::new(200));
    let (hook_url, hits) = spawn_receiver(status.clone()).await;
    let (product_id, _) = app.product_with_keys("Shop", Some(&hook_url)).await;

    let ok = app
        .tool(
            &c.access,
            "test_webhook_integration",
            json!({ "product_id": product_id }),
        )
        .await;
    assert_eq!(ok["isError"], false, "{}", text_of(&ok));
    let data = &ok["structuredContent"];
    assert_eq!(data["delivery"]["status"], "delivered");
    assert_eq!(data["delivery"]["last_status_code"], 200);
    assert!(data["verdict"].as_str().unwrap().starts_with("Delivered"));
    {
        let hits = hits.lock().unwrap();
        assert_eq!(hits.len(), 1);
        let signature = hits[0].0["payrouters-signature"].to_str().unwrap();
        assert!(
            signature.starts_with("t=") && signature.contains(",v1="),
            "{signature}"
        );
        assert!(hits[0].1.contains("payment.paid"));
    }

    // an endpoint that rejects the request is explained, and retry_webhook_delivery is offered
    status.store(401, Ordering::SeqCst);
    let rejected = app
        .tool(
            &c.access,
            "test_webhook_integration",
            json!({ "product_id": product_id }),
        )
        .await;
    let data = &rejected["structuredContent"];
    assert_eq!(data["delivery"]["status"], "pending");
    assert_eq!(data["delivery"]["last_status_code"], 401);
    let verdict = data["verdict"].as_str().unwrap();
    assert!(
        verdict.contains("HTTP 401") && verdict.contains("retry_webhook_delivery"),
        "{verdict}"
    );

    status.store(200, Ordering::SeqCst);
    let delivery_id = data["delivery"]["id"].as_str().unwrap().to_string();
    let retried = app
        .tool(
            &c.access,
            "retry_webhook_delivery",
            json!({ "delivery_id": delivery_id }),
        )
        .await;
    assert_eq!(retried["isError"], false, "{}", text_of(&retried));
    payrouters_server::services::outbound::run_once(&app.state)
        .await
        .unwrap();
    let after = app
        .tool(
            &c.access,
            "list_webhook_deliveries",
            json!({ "product_id": product_id, "status": "delivered" }),
        )
        .await;
    assert_eq!(
        after["structuredContent"]["data"].as_array().unwrap().len(),
        2
    );

    // an unreachable endpoint says so, and a product without a URL is told to set one
    let (dead, _) = app
        .product_with_keys("Dead", Some("http://127.0.0.1:1/hook"))
        .await;
    let unreachable = app
        .tool(
            &c.access,
            "test_webhook_integration",
            json!({ "product_id": dead }),
        )
        .await;
    let verdict = unreachable["structuredContent"]["verdict"]
        .as_str()
        .unwrap();
    assert!(
        verdict.contains("could not reach") && verdict.contains("localhost"),
        "{verdict}"
    );
    let (bare, _) = app.product_with_keys("Bare", None).await;
    let none = app
        .tool(
            &c.access,
            "test_webhook_integration",
            json!({ "product_id": bare }),
        )
        .await;
    assert_eq!(none["isError"], true);
    assert!(text_of(&none).contains("update_product"));
}

#[sqlx::test(migrations = "./migrations")]
async fn failed_oauth_attempts_lock_one_client_out(pool: PgPool) {
    let app = App::with_config(pool, |c| {
        c.trusted_proxy_header = Some(HeaderName::from_static("x-forwarded-for"));
        c.rate.api_key_fails = 3;
    })
    .await;
    let attacker = [("x-forwarded-for", "198.51.100.7")];
    let client_id = app.register().await;

    for _ in 0..3 {
        let r = app
            .form(
                &[
                    ("grant_type", "authorization_code"),
                    ("code", "guess"),
                    ("redirect_uri", REDIRECT),
                    ("client_id", &client_id),
                    ("code_verifier", VERIFIER),
                ],
                &attacker,
            )
            .await;
        assert_eq!(r.json["error"], "invalid_grant");
    }
    let locked = app
        .form(&[("grant_type", "authorization_code")], &attacker)
        .await;
    assert_eq!(locked.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(locked.json["error"], "slow_down");
    assert!(locked.headers.contains_key("retry-after"));

    // MCP tokens share the lockout, and another client is not affected
    let ping = json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" });
    let refused = app
        .raw(
            "POST",
            "/mcp",
            Some("prm_at_guess"),
            &attacker,
            "application/json",
            Some(ping.to_string()),
        )
        .await;
    assert_eq!(refused.status, StatusCode::TOO_MANY_REQUESTS);
    let other = [("x-forwarded-for", "198.51.100.8")];
    let fine = app
        .raw(
            "POST",
            "/mcp",
            Some("prm_at_guess"),
            &other,
            "application/json",
            Some(ping.to_string()),
        )
        .await;
    assert_eq!(fine.status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn oauth_cleanup_removes_expired_rows_and_idle_clients(pool: PgPool) {
    let app = App::new(pool).await;
    let c = app.connect("read").await;
    let idle = app.register().await;

    sqlx::query("UPDATE oauth_clients SET created_at = now() - interval '8 days'")
        .execute(&app.state.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE oauth_access_tokens SET expires_at = now() - interval '1 minute'")
        .execute(&app.state.pool)
        .await
        .unwrap();
    payrouters_server::services::oauth::purge(&app.state)
        .await
        .unwrap();

    let clients: Vec<(String,)> = sqlx::query_as("SELECT client_id FROM oauth_clients")
        .fetch_all(&app.state.pool)
        .await
        .unwrap();
    // the connected client stays, the one that never got a connection is pruned
    assert_eq!(clients, vec![(c.client_id.clone(),)]);
    assert!(clients.iter().all(|(id,)| *id != idle));
    let (tokens,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM oauth_access_tokens")
        .fetch_one(&app.state.pool)
        .await
        .unwrap();
    assert_eq!(tokens, 0);
}

/// A webhook endpoint written the way the docs ask: verifies the signature and its timestamp on the
/// raw body and refuses anything else. `secret` is filled in once the product exists.
async fn spawn_strict_receiver(secret: Arc<Mutex<String>>) -> String {
    let app = Router::new().route(
        "/hook",
        post(move |headers: HeaderMap, body: String| {
            let secret = secret.clone();
            async move {
                let Some(sig) = headers
                    .get("payrouters-signature")
                    .and_then(|v| v.to_str().ok())
                else {
                    return StatusCode::UNAUTHORIZED;
                };
                let ts: i64 = sig
                    .strip_prefix("t=")
                    .and_then(|r| r.split(',').next())
                    .and_then(|t| t.parse().ok())
                    .unwrap_or(0);
                let expected = payrouters_server::services::outbound::signature_header(
                    &secret.lock().unwrap(),
                    ts,
                    &body,
                );
                let fresh = (chrono::Utc::now().timestamp() - ts).abs() <= 300;
                if fresh && expected == sig {
                    StatusCode::OK
                } else {
                    StatusCode::UNAUTHORIZED
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}/hook")
}

async fn webhook_secret(app: &App, product_id: &str) -> String {
    let (enc,): (String,) = sqlx::query_as("SELECT webhook_secret_enc FROM products WHERE id = $1")
        .bind(uuid::Uuid::parse_str(product_id).unwrap())
        .fetch_one(&app.state.pool)
        .await
        .unwrap();
    app.state.cipher.decrypt_string(&enc).unwrap()
}

fn statuses(result: &Value) -> Vec<(String, String)> {
    result["structuredContent"]["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["check"].as_str().unwrap().to_string(),
                c["status"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[sqlx::test(migrations = "./migrations")]
async fn selftest_passes_an_endpoint_that_follows_the_docs(pool: PgPool) {
    let app = App::new(pool).await;
    let c = app.connect("read write").await;
    let secret = Arc::new(Mutex::new(String::new()));
    let url = spawn_strict_receiver(secret.clone()).await;
    let (product_id, _) = app.product_with_keys("Shop", Some(&url)).await;
    *secret.lock().unwrap() = webhook_secret(&app, &product_id).await;

    let r = app
        .tool(
            &c.access,
            "run_integration_selftest",
            json!({ "product_id": product_id }),
        )
        .await;
    assert_eq!(r["isError"], false, "{}", text_of(&r));
    let checks = statuses(&r);
    assert_eq!(checks.len(), 7, "{checks:?}");
    assert!(
        checks.iter().all(|(_, s)| s == "pass"),
        "{checks:?}\n{}",
        text_of(&r)
    );
    assert_eq!(r["structuredContent"]["ok"], true);
    assert_eq!(r["structuredContent"]["failed"], 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn selftest_flags_an_endpoint_that_trusts_everything(pool: PgPool) {
    let app = App::new(pool).await;
    let c = app.connect("read write").await;
    let (url, hits) = spawn_receiver(Arc::new(AtomicU16::new(200))).await;
    let (product_id, _) = app.product_with_keys("Lax", Some(&url)).await;

    let r = app
        .tool(
            &c.access,
            "run_integration_selftest",
            json!({ "product_id": product_id }),
        )
        .await;
    let checks = statuses(&r);
    let status_of = |name: &str| {
        checks
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, s)| s.as_str())
            .unwrap_or("missing")
    };
    assert_eq!(status_of("payment.paid delivered"), "pass");
    assert_eq!(status_of("refuses a wrong signature"), "fail");
    assert_eq!(status_of("refuses a missing signature"), "fail");
    assert_eq!(status_of("refuses a stale timestamp"), "warn");
    assert_eq!(status_of("acknowledges a repeated event"), "pass");
    assert_eq!(r["structuredContent"]["ok"], false);
    assert!(
        text_of(&r).contains("Anyone who knows the URL can fake a paid event"),
        "{}",
        text_of(&r)
    );
    // 3 real deliveries + 4 probes
    assert_eq!(hits.lock().unwrap().len(), 7);
}

#[sqlx::test(migrations = "./migrations")]
async fn selftest_stops_when_nothing_answers_and_needs_a_url(pool: PgPool) {
    let app = App::new(pool).await;
    let c = app.connect("read write").await;
    let (dead, _) = app
        .product_with_keys("Dead", Some("http://127.0.0.1:1/hook"))
        .await;
    let r = app
        .tool(
            &c.access,
            "run_integration_selftest",
            json!({ "product_id": dead }),
        )
        .await;
    assert_eq!(r["structuredContent"]["stopped_early"], true);
    assert_eq!(r["structuredContent"]["ok"], false);
    assert_eq!(statuses(&r).len(), 1, "probes must not repeat the error");

    let (bare, _) = app.product_with_keys("Bare", None).await;
    let r = app
        .tool(
            &c.access,
            "run_integration_selftest",
            json!({ "product_id": bare }),
        )
        .await;
    assert_eq!(r["isError"], true);
    assert!(text_of(&r).contains("no webhook_url"));

    // write permission is required
    let ro = app.connect("read").await;
    let denied = app
        .tool(
            &ro.access,
            "run_integration_selftest",
            json!({ "product_id": dead }),
        )
        .await;
    assert_eq!(denied["isError"], true);
}

#[sqlx::test(migrations = "./migrations")]
async fn the_application_checkout_can_be_waited_for_simulated_and_confirmed(pool: PgPool) {
    let app = App::new(pool).await;
    let c = app.connect("read write").await;
    let secret = Arc::new(Mutex::new(String::new()));
    let url = spawn_strict_receiver(secret.clone()).await;
    let (product_id, key) = app.product_with_keys("Shop", Some(&url)).await;
    *secret.lock().unwrap() = webhook_secret(&app, &product_id).await;

    // nothing yet: not an error, with advice
    let none = app
        .tool(
            &c.access,
            "wait_for_payment",
            json!({ "product_id": product_id, "timeout_seconds": 0 }),
        )
        .await;
    assert_eq!(none["isError"], false);
    assert_eq!(none["structuredContent"]["found"], false);
    assert!(text_of(&none).contains("Trigger the checkout"));

    // MCP's own test payments are not mistaken for the application's
    app.tool(
        &c.access,
        "test_webhook_integration",
        json!({ "product_id": product_id }),
    )
    .await;
    let still_none = app
        .tool(
            &c.access,
            "wait_for_payment",
            json!({ "product_id": product_id, "timeout_seconds": 0 }),
        )
        .await;
    assert_eq!(still_none["structuredContent"]["found"], false);

    // the application creates an order through the API with its sandbox key
    let created = app
        .call(
            "POST",
            "/v1/payments",
            Some(&key),
            Some(json!({ "order_id": "APP-1001", "amount": 75000, "provider": "mock", "method": "va_bca" })),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text);
    let found = app
        .tool(
            &c.access,
            "wait_for_payment",
            json!({ "product_id": product_id, "timeout_seconds": 5 }),
        )
        .await;
    assert_eq!(
        found["structuredContent"]["found"],
        true,
        "{}",
        text_of(&found)
    );
    assert_eq!(
        found["structuredContent"]["payment"]["order_id"],
        "APP-1001"
    );
    let payment_id = found["structuredContent"]["payment"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    // the same order can be waited for by id
    let by_order = app
        .tool(
            &c.access,
            "wait_for_payment",
            json!({ "product_id": product_id, "order_id": "APP-1001", "timeout_seconds": 0 }),
        )
        .await;
    assert_eq!(by_order["structuredContent"]["found"], true);

    // paying it delivers the webhook and reports the application's answer
    let paid = app
        .tool(
            &c.access,
            "simulate_payment",
            json!({ "payment_id": payment_id, "outcome": "paid" }),
        )
        .await;
    assert_eq!(paid["isError"], false, "{}", text_of(&paid));
    assert_eq!(paid["structuredContent"]["status"], "paid");
    assert_eq!(
        paid["structuredContent"]["webhook"]["delivery"]["status"],
        "delivered"
    );
    assert!(text_of(&paid).contains("Delivered: your endpoint answered 200"));

    // a live payment of the same product stays hidden without the live permission
    let live = app
        .tool(
            &c.access,
            "wait_for_payment",
            json!({ "product_id": product_id, "mode": "live", "timeout_seconds": 0 }),
        )
        .await;
    assert_eq!(live["isError"], true);
}

#[sqlx::test(migrations = "./migrations")]
async fn an_assistant_can_choose_a_products_payment_options_and_payments_follow_them(pool: PgPool) {
    let app = App::new(pool).await;
    let (product_id, sandbox_key) = app.product_with_keys("Shop", None).await;
    // the helper product has a live key, so changing it needs the live permission
    let c = app.connect("read write live").await;

    // nothing set: one provider, and the tool says what could be chosen
    let r = app
        .tool(
            &c.access,
            "get_product_payment_options",
            json!({ "product_id": product_id }),
        )
        .await;
    assert_eq!(r["isError"], false, "{}", text_of(&r));
    assert_eq!(r["structuredContent"]["data"], json!([]));
    let mock = r["structuredContent"]["available"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["provider"] == "mock")
        .expect("the simulator is always available in sandbox");
    assert!(mock["methods"].as_array().unwrap().len() >= 5);
    assert_eq!(mock["checkout"], false);
    assert!(text_of(&r).contains("one provider"), "{}", text_of(&r));

    // bad lists are refused with the reason
    for (options, needle) in [
        (
            json!([{ "option": "qris", "provider": "paypal" }]),
            "does not support",
        ),
        (
            json!([{ "option": "qris", "provider": "mock" }, { "option": "qris", "provider": "midtrans" }]),
            "listed twice",
        ),
        (
            json!([{ "option": "checkout:mock", "provider": "mock" }]),
            "no provider-hosted checkout",
        ),
        (
            json!([{ "option": "bitcoin", "provider": "mock" }]),
            "unknown payment option",
        ),
    ] {
        let r = app
            .tool(
                &c.access,
                "set_product_payment_options",
                json!({ "product_id": product_id, "options": options }),
            )
            .await;
        assert_eq!(r["isError"], true, "{needle}");
        assert!(text_of(&r).contains(needle), "{needle}: {}", text_of(&r));
    }

    // a valid list is saved and shows up on the product
    let r = app
        .tool(
            &c.access,
            "set_product_payment_options",
            json!({
                "product_id": product_id,
                "options": [
                    { "option": "qris", "provider": "mock" },
                    { "option": "va_bca", "provider": "mock" }
                ]
            }),
        )
        .await;
    assert_eq!(r["isError"], false, "{}", text_of(&r));
    assert_eq!(
        r["structuredContent"]["data"],
        json!([
            { "option": "qris", "provider": "mock" },
            { "option": "va_bca", "provider": "mock" }
        ])
    );
    let listed = app.tool(&c.access, "list_products", json!({})).await;
    assert_eq!(
        listed["structuredContent"]["data"][0]["payment_options"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    // payments follow the options when the request names no provider
    let ok = app
        .tool(
            &c.access,
            "create_test_payment",
            json!({ "product_id": product_id, "amount": 50000, "method": "qris" }),
        )
        .await;
    assert_eq!(ok["isError"], false, "{}", text_of(&ok));
    assert_eq!(ok["structuredContent"]["provider"], "mock");
    assert_eq!(ok["structuredContent"]["instructions"]["type"], "qris");
    let refused = app
        .tool(
            &c.access,
            "create_test_payment",
            json!({ "product_id": product_id, "amount": 50000, "method": "va_bni" }),
        )
        .await;
    assert_eq!(refused["isError"], true);
    assert!(
        text_of(&refused).contains("method_not_enabled"),
        "{}",
        text_of(&refused)
    );

    // the Snap page offers exactly the product's methods
    let pending = app
        .tool(
            &c.access,
            "create_test_payment",
            json!({ "product_id": product_id, "amount": 50000 }),
        )
        .await;
    let snap_url = pending["structuredContent"]["snap_url"].as_str().unwrap();
    let token = snap_url.rsplit('/').next().unwrap();
    let view = app
        .call("GET", &format!("/snap/api/{token}"), None, None)
        .await;
    let offered: Vec<&str> = view.json["methods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["code"].as_str().unwrap())
        .collect();
    assert_eq!(offered, ["qris", "va_bca"]);
    assert_eq!(view.json["checkouts"], json!([]));
    // and a method outside them is refused on the page too
    let pay = app
        .call(
            "POST",
            &format!("/snap/api/{token}/pay"),
            None,
            Some(json!({ "method": "va_bni" })),
        )
        .await;
    assert_eq!(
        pay.json["error"]["code"], "method_unsupported",
        "{}",
        pay.text
    );
    let both = app
        .call(
            "POST",
            &format!("/snap/api/{token}/pay"),
            None,
            Some(json!({ "method": "qris", "provider": "mock" })),
        )
        .await;
    assert_eq!(both.status, StatusCode::BAD_REQUEST, "{}", both.text);
    let chosen = app
        .call(
            "POST",
            &format!("/snap/api/{token}/pay"),
            None,
            Some(json!({ "method": "va_bca" })),
        )
        .await;
    assert_eq!(
        chosen.json["payment"]["provider"], "mock",
        "{}",
        chosen.text
    );
    assert_eq!(
        chosen.json["payment"]["instructions"]["type"],
        "virtual_account"
    );

    // an explicit provider bypasses the options, and `GET /v1/methods` shows the product's own list
    let methods = app
        .call("GET", "/v1/methods", Some(&sandbox_key), None)
        .await;
    let providers = methods.json["data"].as_array().unwrap();
    assert_eq!(providers.len(), 1, "{}", methods.text);
    let codes: Vec<&str> = providers[0]["methods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["code"].as_str().unwrap())
        .collect();
    assert_eq!(codes, ["qris", "va_bca"]);
    let bypass = app
        .call(
            "POST",
            "/v1/payments",
            Some(&sandbox_key),
            Some(json!({ "order_id": "B-1", "amount": 10000, "provider": "mock", "method": "va_bni" })),
        )
        .await;
    assert_eq!(bypass.status, StatusCode::CREATED, "{}", bypass.text);

    // clearing returns to one provider
    let cleared = app
        .tool(
            &c.access,
            "set_product_payment_options",
            json!({ "product_id": product_id, "options": [] }),
        )
        .await;
    assert_eq!(cleared["isError"], false);
    assert_eq!(cleared["structuredContent"]["data"], json!([]));
}
