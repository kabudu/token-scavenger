use axum::{
    body::Body,
    http::{Request, header::HeaderValue},
    middleware::Next,
    response::Response,
};
use uuid::Uuid;

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
