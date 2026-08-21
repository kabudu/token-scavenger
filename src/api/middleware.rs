use axum::{
    body::Body,
    extract::State,
    http::{Request, header::HeaderValue},
    middleware::Next,
    response::Response,
};
use std::time::Instant;
use uuid::Uuid;

use crate::app::state::AppState;

/// Publish a redacted HTTP lifecycle line to the operational System Stream.
pub async fn system_stream_middleware(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let request_id = req
        .headers()
        .get("X-Request-Id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("-")
        .to_string();
    let started = Instant::now();
    let response = next.run(req).await;
    let line = format!(
        "[HTTP] {} {} status={} latency_ms={} request_id={}",
        method,
        path,
        response.status().as_u16(),
        started.elapsed().as_millis(),
        request_id
    );
    if let Some(sender) = state.log_tx.lock().unwrap().as_ref() {
        let _ = sender.send(line);
    }
    response
}

/// Middleware that adds an `X-Request-Id` header to every response.
/// If the request already has one, it is reused; otherwise a new UUID v4 is generated.
pub async fn request_id_middleware(mut req: Request<Body>, next: Next) -> Response {
    let request_id = req
        .headers()
        .get("X-Request-Id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let header_value = HeaderValue::from_str(&request_id).unwrap_or_else(|_| {
        HeaderValue::from_str(&Uuid::new_v4().to_string()).expect("uuid is a valid header")
    });

    req.headers_mut()
        .insert("X-Request-Id", header_value.clone());

    let mut response = next.run(req).await;
    response.headers_mut().insert("X-Request-Id", header_value);
    response
}
