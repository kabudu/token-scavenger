use crate::app::state::AppState;
use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use tracing::warn;

/// Optional API key authentication middleware.
/// If `server.master_api_key` is set, all requests must include
/// `Authorization: Bearer <key>`.
/// Query-string API keys are accepted only when explicitly enabled.
pub async fn auth_middleware(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let config = state.config();
    let master_key = &config.server.master_api_key;

    if master_key.is_empty() {
        // Auth disabled
        return Ok(next.run(req).await);
    }

    // Check Authorization header
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if let Some(token) = auth_header.strip_prefix("Bearer ") {
        if token == master_key {
            return Ok(next.run(req).await);
        }
    }

    if config.server.allow_query_api_keys {
        if let Some(query_key) = req.uri().query().and_then(|q| {
            q.split('&').find_map(|pair| {
                let mut parts = pair.splitn(2, '=');
                if parts.next() == Some("api_key") {
                    parts.next()
                } else {
                    None
                }
            })
        }) {
            if query_key == master_key {
                return Ok(next.run(req).await);
            }
        }
    }

    warn!("Authentication failed: invalid or missing API key");
    Err(StatusCode::UNAUTHORIZED)
}
