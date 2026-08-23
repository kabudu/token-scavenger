//! End-to-end tests using an in-process mock provider adapter.

use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::{get, post},
};
use sqlx::SqlitePool;
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};
use tower::ServiceExt;

/// Mini mock provider state.
struct MiniMock {
    failures: AtomicU32,
    succeed_after: u32,
    rate_limited: bool,
    delay_ms: u64,
    empty_stream: bool,
}

/// Build a TokenScavenger test app with one mock provider.
async fn build_e2e_app(succeed_after: u32) -> (axum::Router, tokenscavenger::app::state::AppState) {
    build_e2e_app_with_failure(succeed_after, false, 0).await
}

async fn build_e2e_app_with_failure(
    succeed_after: u32,
    rate_limited: bool,
    delay_ms: u64,
) -> (axum::Router, tokenscavenger::app::state::AppState) {
    build_e2e_app_with_options(succeed_after, rate_limited, delay_ms, false, false).await
}

async fn build_e2e_app_with_options(
    succeed_after: u32,
    rate_limited: bool,
    delay_ms: u64,
    empty_stream: bool,
    recover_empty_stream: bool,
) -> (axum::Router, tokenscavenger::app::state::AppState) {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();

    let mut config = tokenscavenger::config::schema::Config::default();
    config.server.master_api_key = String::new();
    config.routing.provider_order = vec!["mock".into()];
    config.routing.recover_empty_stream_with_non_streaming = recover_empty_stream;
    if delay_ms > 0 {
        config
            .routing
            .stream_first_content_timeout_ms
            .insert("test-model".into(), 1_000);
    }
    config.providers = vec![tokenscavenger::config::schema::ProviderConfig {
        id: "mock".into(),
        enabled: true,
        base_url: Some("http://mock.local".into()),
        api_key: None,
        free_only: true,
        discover_models: true,
        embedding_support: Default::default(),
    }];

    let state = tokenscavenger::app::state::AppState::new(
        config,
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );
    state.provider_registry.init_from_config(&state).await;
    sqlx::query(
        "INSERT OR REPLACE INTO providers (provider_id, display_name, enabled, base_url, free_only)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("mock")
    .bind("Mock")
    .bind(true)
    .bind("http://mock.local")
    .bind(true)
    .execute(&state.db)
    .await
    .unwrap();

    // Seed a model row so filter_by_model_enabled finds it.
    // Missing rows represent models not present in a provider catalog.
    sqlx::query(
        "INSERT OR IGNORE INTO models (provider_id, upstream_model_id, public_model_id, enabled, free_tier, supports_chat, discovered_at, updated_at)
         VALUES (?, ?, ?, 1, 1, 1, datetime('now'), datetime('now'))",
    )
    .bind("mock")
    .bind("test-model")
    .bind("test-model")
    .execute(&state.db)
    .await
    .unwrap();

    // Register a simple mock adapter
    use async_trait::async_trait;
    use reqwest::header::HeaderMap;
    use tokenscavenger::api::openai::chat::{NormalizedChatRequest, ProviderChatResponse};
    use tokenscavenger::api::openai::embeddings::{
        NormalizedEmbeddingsRequest, ProviderEmbeddingsResponse,
    };
    use tokenscavenger::api::openai::stream::StreamEvent;
    use tokenscavenger::config::schema::ProviderConfig;
    use tokenscavenger::discovery::curated::DiscoveredModel;
    use tokenscavenger::providers::normalization::ProviderCapabilities;
    use tokenscavenger::providers::traits::{
        AuthKind, EndpointKind, ProviderAdapter, ProviderContext, ProviderError,
    };
    use url::Url;

    struct MockAdapter {
        state: Arc<MiniMock>,
    }

    #[async_trait]
    impl ProviderAdapter for MockAdapter {
        fn provider_id(&self) -> &'static str {
            "mock"
        }
        fn display_name(&self) -> &'static str {
            "Mock"
        }
        fn supports_endpoint(&self, _: &EndpointKind) -> bool {
            true
        }
        fn auth_kind(&self) -> AuthKind {
            AuthKind::None
        }
        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities::default()
        }
        fn base_url(&self, cfg: &ProviderConfig) -> Url {
            cfg.base_url.as_ref().unwrap().parse().unwrap()
        }
        fn default_headers(&self, _: &ProviderConfig) -> HeaderMap {
            HeaderMap::new()
        }
        async fn discover_models(
            &self,
            _: &ProviderContext,
        ) -> Result<Vec<DiscoveredModel>, ProviderError> {
            Ok(vec![DiscoveredModel {
                provider_id: "mock".into(),
                upstream_model_id: "test-model".into(),
                display_name: Some("Test Model".into()),
                endpoint_compatibility: vec!["chat".into()],
                context_window: Some(8192),
                free_tier: true,
            }])
        }
        async fn chat_completions(
            &self,
            _: &ProviderContext,
            req: NormalizedChatRequest,
        ) -> Result<ProviderChatResponse, ProviderError> {
            if self.state.delay_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(self.state.delay_ms)).await;
            }
            let count = self.state.failures.fetch_add(1, Ordering::SeqCst);
            if count < self.state.succeed_after {
                if self.state.rate_limited {
                    return Err(ProviderError::RateLimited {
                        retry_after: Some(7),
                        details: "mock rate limited".into(),
                    });
                }
                return Err(ProviderError::Other("mock unavailable".into()));
            }

            Ok(ProviderChatResponse {
                provider_id: "mock".into(),
                model_id: req.model,
                content: Some("OK".into()),
                tool_calls: None,
                finish_reason: Some("stop".into()),
                usage: Some(tokenscavenger::api::openai::chat::ProviderUsage {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    total_tokens: 15,
                    prompt_cache_hit_tokens: None,
                    prompt_cache_miss_tokens: None,
                    reasoning_tokens: None,
                }),
                latency_ms: 1,
            })
        }
        async fn embeddings(
            &self,
            _: &ProviderContext,
            _: NormalizedEmbeddingsRequest,
        ) -> Result<ProviderEmbeddingsResponse, ProviderError> {
            Err(ProviderError::UnsupportedFeature("no embeddings".into()))
        }
        async fn stream_chat_completions(
            &self,
            ctx: &ProviderContext,
            req: NormalizedChatRequest,
            tx: tokio::sync::mpsc::Sender<StreamEvent>,
        ) -> Result<(), ProviderError> {
            if self.state.empty_stream {
                let _ = tx.send(StreamEvent::Done).await;
                return Ok(());
            }
            let response = self.chat_completions(ctx, req).await?;
            let id = uuid::Uuid::new_v4().to_string();
            let created = chrono::Utc::now().timestamp();
            let _ = tx
                .send(StreamEvent::Chunk {
                    id: id.clone(),
                    created,
                    model: response.model_id.clone(),
                    delta: tokenscavenger::api::openai::chat::StreamDelta {
                        role: Some("assistant".into()),
                        content: response.content,
                    },
                    finish_reason: response.finish_reason,
                })
                .await;
            if let Some(usage) = response.usage {
                let _ = tx
                    .send(StreamEvent::Usage {
                        id,
                        created,
                        model: response.model_id,
                        prompt_tokens: usage.prompt_tokens,
                        completion_tokens: usage.completion_tokens,
                        total_tokens: usage.total_tokens,
                    })
                    .await;
            }
            let _ = tx.send(StreamEvent::Done).await;
            Ok(())
        }
    }
    state
        .provider_registry
        .register(Arc::new(MockAdapter {
            state: Arc::new(MiniMock {
                failures: AtomicU32::new(0),
                succeed_after,
                rate_limited,
                delay_ms,
                empty_stream,
            }),
        }))
        .await;

    let router = axum::Router::new()
        .route(
            "/v1/chat/completions",
            post(tokenscavenger::api::routes::chat_completions),
        )
        .route(
            "/v1/embeddings",
            post(tokenscavenger::api::routes::embeddings),
        )
        .route("/v1/models", get(tokenscavenger::api::routes::models))
        .route("/healthz", get(tokenscavenger::api::routes::healthz))
        .with_state(state.clone());

    (router, state)
}

#[tokio::test]
async fn e2e_chat_success() {
    let (app, state) = build_e2e_app(0).await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/chat/completions")
                .method("POST")
                .header("Content-Type", "application/json")
                .header("X-Request-Id", "req-e2e-chat-success")
                .body(Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "model": "test-model", "messages": [{"role":"user","content":"Hi"}]
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["choices"][0]["message"]["content"], "OK");
    assert!(json["usage"]["prompt_tokens"].as_u64().unwrap() > 0);

    let row: (String, String) =
        sqlx::query_as("SELECT request_id, endpoint_kind FROM request_log WHERE request_id = ?")
            .bind("req-e2e-chat-success")
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert_eq!(row.0, "req-e2e-chat-success");
    assert_eq!(row.1, "chat");
}

#[tokio::test]
async fn e2e_retry_then_success() {
    let (app, _state) = build_e2e_app(1).await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/chat/completions")
                .method("POST")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "model": "test-model", "messages": [{"role":"user","content":"Hi"}]
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["choices"][0]["message"]["content"], "OK");
}

#[tokio::test]
async fn e2e_upstream_rate_limit_exhaustion_returns_429() {
    let (app, state) = build_e2e_app_with_failure(u32::MAX, true, 0).await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/chat/completions")
                .method("POST")
                .header("Content-Type", "application/json")
                .header("X-Request-Id", "req-e2e-rate-limited")
                .body(Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "model": "test-model", "messages": [{"role":"user","content":"Hi"}]
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(resp.headers().get("retry-after").unwrap(), "7");
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "rate_limit_exceeded");

    let row: (String, i64) =
        sqlx::query_as("SELECT status, http_status FROM request_log WHERE request_id = ?")
            .bind("req-e2e-rate-limited")
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert_eq!(row.0, "rate_limited");
    assert_eq!(row.1, 429);
}

#[tokio::test]
async fn e2e_streaming_rate_limit_emits_error_event_and_records_429() {
    let (app, state) = build_e2e_app_with_failure(u32::MAX, true, 0).await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/chat/completions")
                .method("POST")
                .header("Content-Type", "application/json")
                .header("X-Request-Id", "req-e2e-stream-rate-limited")
                .body(Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "model": "test-model",
                        "messages": [{"role":"user","content":"Hi"}],
                        "stream": true
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 16 * 1024)
        .await
        .unwrap();
    let stream = String::from_utf8(body.to_vec()).unwrap();
    assert!(stream.contains("rate_limit_exceeded"));
    assert!(stream.contains("mock rate limited"));
    assert!(stream.contains("data: [DONE]"));

    let row: (String, i64) =
        sqlx::query_as("SELECT status, http_status FROM request_log WHERE request_id = ?")
            .bind("req-e2e-stream-rate-limited")
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert_eq!(row.0, "rate_limited");
    assert_eq!(row.1, 429);
}

#[tokio::test]
async fn e2e_streaming_timeout_emits_classified_error_and_persists_details() {
    let (app, state) = build_e2e_app_with_failure(0, false, 1_200).await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/chat/completions")
                .method("POST")
                .header("Content-Type", "application/json")
                .header("X-Request-Id", "req-e2e-stream-timeout")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "model": "test-model",
                        "messages": [{"role":"user","content":"Hi"}],
                        "stream": true
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 16 * 1024)
        .await
        .unwrap();
    let stream = String::from_utf8(body.to_vec()).unwrap();
    assert!(stream.contains("stream_timeout_pre_content"));
    assert!(stream.contains("timed out before content after 1000 ms"));
    assert!(stream.contains("data: [DONE]"));

    let row: (String, i64, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT status, http_status, error_code, error_summary FROM request_log WHERE request_id = ?",
    )
    .bind("req-e2e-stream-timeout")
    .fetch_one(&state.db)
    .await
    .unwrap();
    assert_eq!(row.0, "route_exhausted");
    assert_eq!(row.1, 503);
    assert_eq!(row.2.as_deref(), Some("stream_timeout_pre_content"));
    assert_eq!(
        row.3.as_deref(),
        Some("timed out before content after 1000 ms")
    );
}

#[tokio::test]
async fn e2e_stream_silence_hint_does_not_suppress_only_route() {
    let (app, state) = build_e2e_app(0).await;
    tokenscavenger::router::selection::record_stream_silence_hint(&state, "mock", "test-model", 0);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/chat/completions")
                .method("POST")
                .header("Content-Type", "application/json")
                .header("X-Request-Id", "req-e2e-single-silenced-route")
                .body(Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "model": "test-model",
                        "messages": [{"role":"user","content":"Hi"}],
                        "stream": true
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 16 * 1024)
        .await
        .unwrap();
    let stream = String::from_utf8(body.to_vec()).unwrap();
    assert!(stream.contains("OK"));

    let skipped: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM request_trace_events WHERE request_id = ? AND event_type = 'attempt_skipped'",
    )
    .bind("req-e2e-single-silenced-route")
    .fetch_one(&state.db)
    .await
    .unwrap();
    assert_eq!(skipped, 0);
}

#[tokio::test]
async fn e2e_route_exhausted_no_providers() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();
    let config = tokenscavenger::config::schema::Config::default();
    let state = tokenscavenger::app::state::AppState::new(
        config,
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );
    let db = state.db.clone();

    let app = axum::Router::new()
        .route(
            "/v1/chat/completions",
            post(tokenscavenger::api::routes::chat_completions),
        )
        .with_state(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/chat/completions")
                .method("POST")
                .header("Content-Type", "application/json")
                .header("X-Request-Id", "req-route-exhausted")
                .body(Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "model": "nonexistent", "messages": [{"role":"user","content":"Hi"}]
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

    let row: (String, String, i64) = sqlx::query_as(
        "SELECT request_id, status, http_status FROM request_log WHERE request_id = ?",
    )
    .bind("req-route-exhausted")
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(row.0, "req-route-exhausted");
    assert_eq!(row.1, "route_exhausted");
    assert_eq!(row.2, 503);
}

#[tokio::test]
async fn e2e_health_check() {
    let (app, _state) = build_e2e_app(0).await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn e2e_models_endpoint() {
    let (app, _state) = build_e2e_app(0).await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 65536).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["object"], "list");
    assert!(!json["data"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn e2e_empty_stream_recovers_once_via_same_provider_non_streaming() {
    let (app, state) = build_e2e_app_with_options(0, false, 0, true, true).await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/chat/completions")
                .method("POST")
                .header("Content-Type", "application/json")
                .header("X-Request-Id", "req-e2e-empty-stream-recovery")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "model": "test-model",
                        "messages": [{"role":"user","content":"Hi"}],
                        "stream": true
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 16 * 1024)
        .await
        .unwrap();
    let stream = String::from_utf8(body.to_vec()).unwrap();
    assert!(stream.contains("OK"));
    assert!(stream.contains("data: [DONE]"));
    assert!(!stream.contains("empty_stream"));

    let recovery_events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM request_trace_events
         WHERE request_id = ? AND outcome = 'stream_recovered_non_streaming'",
    )
    .bind("req-e2e-empty-stream-recovery")
    .fetch_one(&state.db)
    .await
    .unwrap();
    assert_eq!(recovery_events, 1);

    let usage_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM usage_events WHERE request_id = ?")
            .bind("req-e2e-empty-stream-recovery")
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert_eq!(usage_rows, 1);
}

#[tokio::test]
async fn e2e_empty_stream_recovery_remains_opt_in() {
    let (app, state) = build_e2e_app_with_options(0, false, 0, true, false).await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/chat/completions")
                .method("POST")
                .header("Content-Type", "application/json")
                .header("X-Request-Id", "req-e2e-empty-stream-no-recovery")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "model": "test-model",
                        "messages": [{"role":"user","content":"Hi"}],
                        "stream": true
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), 16 * 1024)
        .await
        .unwrap();
    let stream = String::from_utf8(body.to_vec()).unwrap();
    assert!(stream.contains("empty_stream"));
    assert!(!stream.contains("\"content\":\"OK\""));

    let recovery_events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM request_trace_events
         WHERE request_id = ? AND outcome = 'stream_recovered_non_streaming'",
    )
    .bind("req-e2e-empty-stream-no-recovery")
    .fetch_one(&state.db)
    .await
    .unwrap();
    assert_eq!(recovery_events, 0);
}

#[tokio::test]
async fn e2e_streaming_response_is_wire_level_sse() {
    let (app, state) = build_e2e_app(0).await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/chat/completions")
                .method("POST")
                .header("Content-Type", "application/json")
                .header("X-Request-Id", "req-e2e-streaming-usage")
                .body(Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "model": "test-model",
                        "stream": true,
                        "messages": [{"role":"user","content":"Hi"}]
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 16384).await.unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("data: {"));
    assert!(text.contains("data: [DONE]"));
    assert!(!text.contains("data: data:"));

    let row: (i64, i64, bool) = sqlx::query_as(
        "SELECT input_tokens, output_tokens, streaming FROM usage_events
         JOIN request_log USING (request_id)
         WHERE usage_events.request_id = ?",
    )
    .bind("req-e2e-streaming-usage")
    .fetch_one(&state.db)
    .await
    .unwrap();
    assert_eq!(row.0, 10);
    assert_eq!(row.1, 5);
    assert!(row.2);

    let mut timing_events = 0;
    for _ in 0..20 {
        timing_events = sqlx::query_scalar(
            "SELECT COUNT(*) FROM request_trace_events
             WHERE request_id = ? AND event_type IN ('stream_first_event', 'stream_first_content')
             AND latency_ms IS NOT NULL",
        )
        .bind("req-e2e-streaming-usage")
        .fetch_one(&state.db)
        .await
        .unwrap();
        if timing_events == 2 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(timing_events, 2);
}

#[tokio::test]
async fn e2e_disabled_model_is_not_routed() {
    let (app, state) = build_e2e_app(0).await;
    sqlx::query(
        "INSERT OR REPLACE INTO models (provider_id, upstream_model_id, public_model_id, enabled)
         VALUES (?, ?, ?, 0)",
    )
    .bind("mock")
    .bind("test-model")
    .bind("test-model")
    .execute(&state.db)
    .await
    .unwrap();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/chat/completions")
                .method("POST")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "model": "test-model", "messages": [{"role":"user","content":"Hi"}]
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn e2e_disabled_model_is_not_routed_for_streaming() {
    let (app, state) = build_e2e_app(0).await;
    sqlx::query(
        "INSERT OR REPLACE INTO models (provider_id, upstream_model_id, public_model_id, enabled)
         VALUES (?, ?, ?, 0)",
    )
    .bind("mock")
    .bind("test-model")
    .bind("test-model")
    .execute(&state.db)
    .await
    .unwrap();

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/chat/completions")
                .method("POST")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "model": "test-model",
                        "stream": true,
                        "messages": [{"role":"user","content":"Hi"}]
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn e2e_model_without_embeddings_support_is_not_routed_for_embeddings() {
    let (app, _state) = build_e2e_app(0).await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/embeddings")
                .method("POST")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "model": "test-model",
                        "input": "hello"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}
