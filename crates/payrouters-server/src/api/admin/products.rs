use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use chrono::{DateTime, Utc};
use payrouters_types::{Mode, Provider};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use super::double_option;
use crate::error::{ApiJson, AppError, Result, is_unique_violation};
use crate::models::{ApiKeyRow, ApiKeyView, ProductRow};
use crate::services::options::{self, OptionInput, OptionsView};
use crate::state::AppState;
use crate::util::{new_id, random_hex, sha256_hex, slugify};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/products", get(list).post(create))
        .route(
            "/products/{id}",
            axum::routing::patch(update).delete(archive),
        )
        .route("/products/{id}/webhook-secret/rotate", post(rotate_secret))
        .route("/products/{id}/api-keys", post(create_key))
        .route("/products/{id}/api-keys/{key_id}", delete(revoke_key))
        .route(
            "/products/{id}/payment-options",
            get(get_options).put(put_options),
        )
}

#[derive(Serialize)]
pub(crate) struct ProductView {
    id: Uuid,
    name: String,
    slug: String,
    webhook_url: Option<String>,
    default_provider: Option<String>,
    /// The payment options the product offers (empty: one provider, see `default_provider`).
    payment_options: Vec<OptionInput>,
    api_keys: Vec<ApiKeyView>,
    created_at: DateTime<Utc>,
}

fn view(p: ProductRow, keys: Vec<ApiKeyRow>, options: Vec<OptionInput>) -> ProductView {
    ProductView {
        id: p.id,
        name: p.name,
        slug: p.slug,
        webhook_url: p.webhook_url,
        default_provider: p.default_provider,
        payment_options: options,
        api_keys: keys.into_iter().map(Into::into).collect(),
        created_at: p.created_at,
    }
}

async fn keys_for(state: &AppState, product_ids: &[Uuid]) -> Result<Vec<ApiKeyRow>> {
    Ok(sqlx::query_as::<_, ApiKeyRow>(
        "SELECT id, product_id, mode, name, prefix, created_at, last_used_at, revoked_at
         FROM api_keys WHERE product_id = ANY($1) ORDER BY created_at",
    )
    .bind(product_ids)
    .fetch_all(&state.pool)
    .await?)
}

async fn one(state: &AppState, id: Uuid) -> Result<ProductView> {
    let p: ProductRow =
        sqlx::query_as("SELECT * FROM products WHERE id = $1 AND archived_at IS NULL")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::not_found("product"))?;
    let keys = keys_for(state, &[id]).await?;
    let opts = options::list(state, id)
        .await?
        .iter()
        .map(OptionInput::from)
        .collect();
    Ok(view(p, keys, opts))
}

pub(crate) async fn list(State(state): State<AppState>) -> Result<Json<Value>> {
    let products: Vec<ProductRow> =
        sqlx::query_as("SELECT * FROM products WHERE archived_at IS NULL ORDER BY created_at DESC")
            .fetch_all(&state.pool)
            .await?;
    let ids: Vec<Uuid> = products.iter().map(|p| p.id).collect();
    let keys = keys_for(&state, &ids).await?;
    let opts = options::list_for(&state, &ids).await?;
    let data: Vec<ProductView> = products
        .into_iter()
        .map(|p| {
            let k = keys
                .iter()
                .filter(|k| k.product_id == p.id)
                .cloned()
                .collect();
            let o = opts
                .iter()
                .filter(|(id, _)| *id == p.id)
                .map(|(_, o)| OptionInput::from(o))
                .collect();
            view(p, k, o)
        })
        .collect();
    Ok(Json(json!({ "data": data })))
}

fn validate_webhook_url(url: &str) -> Result<()> {
    match url::Url::parse(url) {
        Ok(u) if matches!(u.scheme(), "http" | "https") && u.host().is_some() => Ok(()),
        _ => Err(AppError::bad_request("webhook_url must be an http(s) URL")),
    }
}

fn validate_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(AppError::bad_request("name must be 1-80 characters"));
    }
    Ok(name.to_string())
}

fn new_webhook_secret() -> String {
    format!("whsec_{}", random_hex::<32>())
}

#[derive(Deserialize)]
pub(crate) struct CreateProduct {
    name: String,
    webhook_url: Option<String>,
    default_provider: Option<Provider>,
}

pub(crate) async fn create(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<CreateProduct>,
) -> Result<(StatusCode, Json<Value>)> {
    let name = validate_name(&body.name)?;
    if let Some(u) = &body.webhook_url {
        validate_webhook_url(u)?;
    }
    let secret = new_webhook_secret();
    let secret_enc = state.cipher.encrypt_str(&secret)?;
    let base = slugify(&name);
    let id = new_id();
    let mut slug = base.clone();
    for attempt in 0..5 {
        let res = sqlx::query(
            "INSERT INTO products (id, name, slug, webhook_url, webhook_secret_enc, default_provider)
             VALUES ($1,$2,$3,$4,$5,$6)",
        )
        .bind(id)
        .bind(&name)
        .bind(&slug)
        .bind(&body.webhook_url)
        .bind(&secret_enc)
        .bind(body.default_provider.map(|p| p.as_str()))
        .execute(&state.pool)
        .await;
        match res {
            Ok(_) => break,
            Err(e) if is_unique_violation(&e) && attempt < 4 => {
                slug = format!("{base}-{}", random_hex::<2>())
            }
            Err(e) => return Err(e.into()),
        }
    }
    let mut v = serde_json::to_value(one(&state, id).await?).map_err(anyhow::Error::from)?;
    v["webhook_secret"] = json!(secret);
    Ok((StatusCode::CREATED, Json(v)))
}

#[derive(Deserialize)]
pub(crate) struct UpdateProduct {
    name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    webhook_url: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    default_provider: Option<Option<Provider>>,
}

pub(crate) async fn update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    ApiJson(body): ApiJson<UpdateProduct>,
) -> Result<Json<ProductView>> {
    let current: ProductRow =
        sqlx::query_as("SELECT * FROM products WHERE id = $1 AND archived_at IS NULL")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::not_found("product"))?;
    let name = match body.name {
        Some(n) => validate_name(&n)?,
        None => current.name,
    };
    let webhook_url = match body.webhook_url {
        Some(Some(u)) => {
            validate_webhook_url(&u)?;
            Some(u)
        }
        Some(None) => None,
        None => current.webhook_url,
    };
    let default_provider = match body.default_provider {
        Some(p) => p.map(|p| p.as_str().to_string()),
        None => current.default_provider,
    };
    sqlx::query(
        "UPDATE products SET name = $2, webhook_url = $3, default_provider = $4 WHERE id = $1",
    )
    .bind(id)
    .bind(name)
    .bind(webhook_url)
    .bind(default_provider)
    .execute(&state.pool)
    .await?;
    Ok(Json(one(&state, id).await?))
}

async fn archive(State(state): State<AppState>, Path(id): Path<Uuid>) -> Result<StatusCode> {
    let mut tx = state.pool.begin().await?;
    let res = sqlx::query(
        "UPDATE products SET archived_at = now() WHERE id = $1 AND archived_at IS NULL",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("product"));
    }
    sqlx::query(
        "UPDATE api_keys SET revoked_at = now() WHERE product_id = $1 AND revoked_at IS NULL",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn rotate_secret(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>> {
    let secret = new_webhook_secret();
    let res = sqlx::query(
        "UPDATE products SET webhook_secret_enc = $2 WHERE id = $1 AND archived_at IS NULL",
    )
    .bind(id)
    .bind(state.cipher.encrypt_str(&secret)?)
    .execute(&state.pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("product"));
    }
    Ok(Json(json!({ "webhook_secret": secret })))
}

#[derive(Deserialize)]
pub(crate) struct CreateKey {
    mode: Mode,
    name: String,
}

pub(crate) async fn create_key(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    ApiJson(body): ApiJson<CreateKey>,
) -> Result<(StatusCode, Json<Value>)> {
    let name = validate_name(&body.name)?;
    let exists: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM products WHERE id = $1 AND archived_at IS NULL")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;
    if exists.is_none() {
        return Err(AppError::not_found("product"));
    }
    let tag = match body.mode {
        Mode::Sandbox => "test",
        Mode::Live => "live",
    };
    let key = format!("prk_{tag}_{}", random_hex::<24>());
    let prefix: String = key.chars().take(13).collect();
    let row: ApiKeyRow = sqlx::query_as(
        "INSERT INTO api_keys (id, product_id, mode, name, prefix, key_hash)
         VALUES ($1,$2,$3,$4,$5,$6)
         RETURNING id, product_id, mode, name, prefix, created_at, last_used_at, revoked_at",
    )
    .bind(new_id())
    .bind(id)
    .bind(body.mode.as_str())
    .bind(name)
    .bind(prefix)
    .bind(sha256_hex(&key))
    .fetch_one(&state.pool)
    .await?;
    let mut v = serde_json::to_value(ApiKeyView::from(row)).map_err(anyhow::Error::from)?;
    v["key"] = json!(key);
    Ok((StatusCode::CREATED, Json(v)))
}

pub(crate) async fn revoke_key(
    State(state): State<AppState>,
    Path((id, key_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode> {
    let res = sqlx::query(
        "UPDATE api_keys SET revoked_at = now() WHERE id = $1 AND product_id = $2 AND revoked_at IS NULL",
    )
    .bind(key_id)
    .bind(id)
    .execute(&state.pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("API key"));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// The product's payment options and what could be added (providers with an enabled account).
pub(crate) async fn get_options(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<OptionsView>> {
    Ok(Json(options::view(&state, id).await?))
}

#[derive(Deserialize)]
pub(crate) struct PutOptions {
    options: Vec<OptionInput>,
}

/// Replace the product's payment options. An empty list clears them: the product goes back to one
/// provider (the `provider` of the request, its default, then the first enabled one).
pub(crate) async fn put_options(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    ApiJson(body): ApiJson<PutOptions>,
) -> Result<Json<OptionsView>> {
    Ok(Json(options::set(&state, id, &body.options).await?))
}
