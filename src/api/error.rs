use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

/// OpenAI-compatible error response body.
#[derive(Debug, Serialize)]
pub struct ApiErrorBody {
    pub error: ApiErrorDetail,
}

#[derive(Debug, Serialize)]
pub struct ApiErrorDetail {
    pub message: String,
    #[serde(rename = "type")]
    pub error_type: String,
    pub param: Option<String>,
    pub code: String,
}

/// Internal error taxonomy for the proxy.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    #[error("Authentication failed")]
    AuthError,
    #[error("Provider unavailable: {0}")]
    ProviderUnavailable(String),
    #[error("All routes exhausted: {0}")]
    RouteExhausted(String),
    #[error("Rate limited")]
    RateLimited,
    #[error("Quota exhausted")]
    QuotaExhausted,
    #[error("Unsupported feature: {0}")]
    UnsupportedFeature(String),
    #[error("Internal error: {0}")]
    InternalError(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, error_type, message) = match &self {
            ApiError::InvalidRequest(msg) => {
                (StatusCode::BAD_REQUEST, "invalid_request".into(), "invalid_request_error".into(), msg.clone())
            }
            ApiError::AuthError => {
                (StatusCode::UNAUTHORIZED, "auth_error".into(), "authentication_error".into(), "Authentication failed".into())
            }
            ApiError::ProviderUnavailable(msg) => {
                (StatusCode::SERVICE_UNAVAILABLE, "provider_unavailable".into(), "provider_error".into(), msg.clone())
            }
            ApiError::RouteExhausted(msg) => {
                (StatusCode::SERVICE_UNAVAILABLE, "route_exhausted".into(), "provider_unavailable".into(), msg.clone())
            }
            ApiError::RateLimited => {
                (StatusCode::TOO_MANY_REQUESTS, "rate_limited".into(), "rate_limit_error".into(), "Rate limited".into())
            }
            ApiError::QuotaExhausted => {
                (StatusCode::TOO_MANY_REQUESTS, "quota_exhausted".into(), "quota_error".into(), "Quota exhausted".into())
            }
            ApiError::UnsupportedFeature(msg) => {
                (StatusCode::BAD_REQUEST, "unsupported_feature".into(), "invalid_request_error".into(), msg.clone())
            }
            ApiError::InternalError(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error".into(), "internal_server_error".into(), msg.clone())
            }
        };

        let body = ApiErrorBody {
            error: ApiErrorDetail {
                message,
                error_type,
                param: None,
                code,
            },
        };

        (status, Json(body)).into_response()
    }
}
