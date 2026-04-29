//! Shared test utilities — mock provider HTTP server.
//!
//! Provides a configurable mock server that simulates provider API responses
//! for integration and E2E testing.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Sse},
    routing::{get, post},
    Json, Router,
};
use std::sync::{Arc, Mutex};
use tokio_stream::StreamExt;

/// Configurable mock provider state.
#[derive(Clone, Default)]
pub struct MockProviderState {
    pub delay_ms: u64,
    pub status_code: u16,
    pub response_body: String,
    pub fail_count: Arc<Mutex<u32>>,
    pub succeed_after: u32,
    pub usage_tokens: (u32, u32),
}

/// Start a mock provider server on a random port. Returns (base_url, join_handle).
pub async fn start_mock_server(state: MockProviderState) -> (String, tokio::task::JoinHandle<()>) {
    let state = Arc::new(state);

    let app = Router::new()
        .route("/v1/chat/completions", post(chat_handler))
        .route("/v1/models", get(models_handler))
        .route("/healthz", get(|| async { "ok" }))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let addr_str = format!("http://{}", addr);

    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    (addr_str, handle)
}

async fn chat_handler(
    State(state): State<Arc<MockProviderState>>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let stream = body.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);

    tokio::time::sleep(std::time::Duration::from_millis(state.delay_ms)).await;

    // Check fail-after count
    let mut fails = state.fail_count.lock().unwrap();
    if *fails < state.succeed_after {
        *fails += 1;
        let status = StatusCode::from_u16(state.status_code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        return (status, Json(serde_json::json!({"error": {"message": "mock failure"}}))).into_response();
    }

    if stream {
        // Return SSE stream
        let stream = async_stream::stream! {
            yield Ok::<_, std::convert::Infallible>(
                axum::response::sse::Event::default().data("data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"test\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Mock\"},\"finish_reason\":null}]}\n\n")
            );
            yield Ok(
                axum::response::sse::Event::default().data("data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"test\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" stream\"},\"finish_reason\":null}]}\n\n")
            );
            yield Ok(
                axum::response::sse::Event::default().data("data: [DONE]\n\n")
            );
        };
        Sse::new(stream).into_response()
    } else {
        (StatusCode::OK, Json(serde_json::json!({
            "id": "chatcmpl-1",
            "object": "chat.completion",
            "created": 1,
            "model": "test-model",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Mock response"},
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": state.usage_tokens.0,
                "completion_tokens": state.usage_tokens.1,
                "total_tokens": state.usage_tokens.0 + state.usage_tokens.1
            }
        }))).into_response()
    }
}

async fn models_handler(
    State(_state): State<Arc<MockProviderState>>,
) -> impl IntoResponse {
    Json(serde_json::json!({
        "object": "list",
        "data": [
            {"id": "test-model", "object": "model", "created": 0, "owned_by": "test-org"}
        ]
    }))
}
