use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::StatusCode;
use axum::http::header::{HeaderValue, RETRY_AFTER};
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use payrouters_types::{ApiErrorBody, ApiErrorDetail};
use serde::de::DeserializeOwned;

pub type Result<T, E = AppError> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{message}")]
    Conflict { code: &'static str, message: String },
    #[error("{message}")]
    Unprocessable { code: &'static str, message: String },
    #[error("{0}")]
    Provider(String),
    /// Too many requests; the number is the seconds until the client may retry.
    #[error("Too many requests. Try again in {0} seconds.")]
    RateLimited(u64),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        AppError::BadRequest(msg.into())
    }

    pub fn not_found(what: &str) -> Self {
        AppError::NotFound(format!("{what} not found"))
    }

    pub fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        AppError::Conflict {
            code,
            message: message.into(),
        }
    }

    pub fn unprocessable(code: &'static str, message: impl Into<String>) -> Self {
        AppError::Unprocessable {
            code,
            message: message.into(),
        }
    }

    fn parts(&self) -> (StatusCode, &str) {
        match self {
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, "invalid_request"),
            AppError::Unauthorized(_) => (StatusCode::UNAUTHORIZED, "unauthorized"),
            AppError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            AppError::Conflict { code, .. } => (StatusCode::CONFLICT, code),
            AppError::Unprocessable { code, .. } => (StatusCode::UNPROCESSABLE_ENTITY, code),
            AppError::Provider(_) => (StatusCode::BAD_GATEWAY, "provider_error"),
            AppError::RateLimited(_) => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
            AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::RowNotFound => AppError::NotFound("resource not found".into()),
            other => AppError::Internal(anyhow::Error::new(other)),
        }
    }
}

pub fn is_unique_violation(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.code())
        .is_some_and(|c| c == "23505")
}

pub fn is_fk_violation(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.code())
        .is_some_and(|c| c == "23503")
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = self.parts();
        let retry_after = match &self {
            AppError::RateLimited(secs) => Some(*secs),
            _ => None,
        };
        let message = match &self {
            AppError::Internal(err) => {
                tracing::error!(error = ?err, "internal error");
                "internal server error".to_string()
            }
            other => other.to_string(),
        };
        let body = ApiErrorBody {
            error: ApiErrorDetail {
                code: code.to_string(),
                message,
                details: None,
            },
        };
        let mut response = (status, Json(body)).into_response();
        if let Some(secs) = retry_after {
            response
                .headers_mut()
                .insert(RETRY_AFTER, HeaderValue::from(secs));
        }
        response
    }
}

/// `Json<T>` whose rejection is the standard error envelope.
pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(v)) => Ok(ApiJson(v)),
            Err(rej) => Err(match rej {
                JsonRejection::MissingJsonContentType(_) => {
                    AppError::bad_request("expected Content-Type: application/json")
                }
                other => AppError::bad_request(other.body_text()),
            }),
        }
    }
}

/// `Query<T>` whose rejection is the standard error envelope.
pub struct ApiQuery<T>(pub T);

impl<S, T> FromRequestParts<S> for ApiQuery<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self> {
        let q = parts.uri.query().unwrap_or("");
        serde_urlencoded_from_str::<T>(q)
            .map(ApiQuery)
            .map_err(|e| AppError::bad_request(format!("invalid query string: {e}")))
    }
}

fn serde_urlencoded_from_str<T: DeserializeOwned>(q: &str) -> std::result::Result<T, String> {
    axum::extract::Query::<T>::try_from_uri(
        &format!("/?{q}")
            .parse()
            .map_err(|_| "bad uri".to_string())?,
    )
    .map(|axum::extract::Query(v)| v)
    .map_err(|e| e.body_text())
}

pub async fn json_not_found() -> AppError {
    AppError::NotFound("route not found".into())
}
