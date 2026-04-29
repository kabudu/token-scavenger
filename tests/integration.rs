//! Integration tests for TokenScavenger API endpoints.
//!
//! Uses the Axum test harness with an ephemeral SQLite database.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use sqlx::SqlitePool;
use tokenscavenger::api::routes;
use tokenscavenger::app::state::AppState;
use tokenscavenger::config::schema::Config;
use tokenscavenger::config::schema::ServerConfig;
use tokenscavenger::resilience::breaker::CircuitBreaker;
use tokenscavenger::resilience::retry::backoff_duration;
use tokenscavenger::usage::pricing::estimate_cost;
use tokenscavenger::util::redact;
use tower::ServiceExt;

/// Build a test app with an in-memory SQLite database.
async fn build_test_app() -> (axum::Router, AppState) {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("Failed to create in-memory SQLite");

    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    let config = Config {
        server: ServerConfig {
            bind: "0.0.0.0:0".into(),
            master_api_key: "test-key".into(),
            ..Default::default()
        },
        ..Default::default()
    };

    let state = AppState::new(config, pool);

    let router = axum::Router::new()
        .route("/healthz", axum::routing::get(routes::healthz))
        .route("/readyz", axum::routing::get(routes::readyz))
        .route("/v1/models", axum::routing::get(routes::models))
        .route(
            "/v1/chat/completions",
            axum::routing::post(routes::chat_completions),
        )
        .route("/v1/embeddings", axum::routing::post(routes::embeddings))
        .route("/admin/config", axum::routing::get(routes::admin_config))
        .with_state(state.clone());

    (router, state)
}

#[tokio::test]
async fn test_healthz_returns_ok() {
    let (app, _state) = build_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    assert_eq!(&body[..], b"ok");
}

#[tokio::test]
async fn test_readyz_returns_json() {
    let (app, _state) = build_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.get("status").is_some());
    assert!(json.get("providers_configured").is_some());
    assert!(json.get("uptime_secs").is_some());
}

#[tokio::test]
async fn test_v1_models_returns_list() {
    let (app, _state) = build_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 65536)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["object"], "list");
    assert!(json["data"].is_array());
    assert!(!json["data"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_admin_config_returns_json() {
    let (app, _state) = build_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 65536)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.get("server").is_some());
    assert!(json.get("database").is_some());
}

#[tokio::test]
async fn test_admin_config_redacts_provider_secrets() {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("Failed to create in-memory SQLite");
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();

    let config = Config {
        providers: vec![tokenscavenger::config::schema::ProviderConfig {
            id: "groq".into(),
            enabled: true,
            base_url: None,
            api_key: Some("gsk_super_secret_key".into()),
            free_only: true,
            discover_models: true,
        }],
        ..Default::default()
    };
    let state = AppState::new(config, pool);
    let app = axum::Router::new()
        .route("/admin/config", axum::routing::get(routes::admin_config))
        .with_state(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 65536)
        .await
        .unwrap();
    let body_text = String::from_utf8(body.to_vec()).unwrap();
    assert!(!body_text.contains("gsk_super_secret_key"));
    assert!(body_text.contains("****"));
}

#[tokio::test]
async fn test_startup_router_enforces_auth_on_protected_routes() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();
    let config = Config {
        server: ServerConfig {
            master_api_key: "secret".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    let state = AppState::new(config, pool);
    let router = tokenscavenger::app::startup::build_router(state);

    let health = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);

    let unauthorized = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let authorized = router
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .header("Authorization", "Bearer secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(authorized.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_chat_completions_no_config_returns_error() {
    let (app, _state) = build_test_app().await;

    let req = json!({
        "model": "llama3-70b-8192",
        "messages": [{"role": "user", "content": "Hello"}]
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/chat/completions")
                .method("POST")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&req).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn test_db_init_and_query() {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("Failed to create in-memory SQLite");

    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    sqlx::query("INSERT INTO providers (provider_id, display_name) VALUES (?, ?)")
        .bind("test-provider")
        .bind("Test Provider")
        .execute(&pool)
        .await
        .expect("Failed to insert provider");

    let row: (String,) = sqlx::query_as("SELECT provider_id FROM providers WHERE provider_id = ?")
        .bind("test-provider")
        .fetch_one(&pool)
        .await
        .expect("Failed to query provider");

    assert_eq!(row.0, "test-provider");
}

#[tokio::test]
async fn test_usage_events_insert_and_query() {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("Failed to create in-memory SQLite");

    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    // First insert a request_log entry (FK requirement for usage_events)
    sqlx::query("INSERT INTO request_log (request_id, endpoint_kind, status) VALUES (?1, ?2, ?3)")
        .bind("test-req-1")
        .bind("chat")
        .bind("success")
        .execute(&pool)
        .await
        .expect("Failed to insert request_log");

    sqlx::query(
        "INSERT INTO usage_events (request_id, provider_id, model_id, input_tokens, output_tokens, estimated_cost_usd, free_tier)
         VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind("test-req-1")
    .bind("groq")
    .bind("llama3-70b-8192")
    .bind(100i64)
    .bind(50i64)
    .bind(0.0f64)
    .bind(true)
    .execute(&pool)
    .await
    .expect("Failed to insert usage event");

    let row: (String, i64, i64) = sqlx::query_as(
        "SELECT provider_id, input_tokens, output_tokens FROM usage_events WHERE request_id = ?",
    )
    .bind("test-req-1")
    .fetch_one(&pool)
    .await
    .expect("Failed to query usage");

    assert_eq!(row.0, "groq");
    assert_eq!(row.1, 100);
    assert_eq!(row.2, 50);
}

#[test]
fn test_secret_redaction() {
    let cases = vec![
        ("sk-abc123def456", "****f456"),
        ("short", "********"),
        ("", "********"),
    ];

    for (input, expected) in cases {
        assert_eq!(redact::redact_secret(input), expected);
    }
}

#[test]
fn test_pricing_free_tier() {
    assert_eq!(estimate_cost(100, 50, "groq"), 0.0);
    assert_eq!(estimate_cost(100, 50, "google"), 0.0);
    assert_eq!(estimate_cost(100, 50, "unknown"), 0.0);
}

#[tokio::test]
async fn test_breaker_state_machine() {
    let cb = CircuitBreaker::new(3, 60);
    assert!(cb.allow_request().await);

    cb.record_failure().await;
    cb.record_failure().await;
    assert!(cb.allow_request().await);

    cb.record_failure().await;
    assert!(!cb.allow_request().await);
}

#[test]
fn test_backoff_calculation() {
    assert_eq!(backoff_duration(1, 100, 5000, false), 200);
    assert_eq!(backoff_duration(2, 100, 5000, false), 400);
    assert_eq!(backoff_duration(10, 100, 5000, false), 5000);

    let with_jitter = backoff_duration(1, 100, 5000, true);
    assert!(with_jitter > 0);
}

#[tokio::test]
async fn test_disabled_provider_is_not_registered() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();
    let config = Config {
        providers: vec![tokenscavenger::config::schema::ProviderConfig {
            id: "groq".into(),
            enabled: false,
            base_url: None,
            api_key: None,
            free_only: true,
            discover_models: true,
        }],
        ..Default::default()
    };
    let state = AppState::new(config, pool);
    state.provider_registry.init_from_config(&state).await;
    assert!(state.provider_registry.list_ids().await.is_empty());
}

#[test]
fn test_stream_payload_is_not_preframed() {
    let event = tokenscavenger::api::openai::stream::StreamEvent::Chunk {
        id: "chatcmpl-test".into(),
        created: 1,
        model: "test-model".into(),
        delta: tokenscavenger::api::openai::chat::StreamDelta {
            role: Some("assistant".into()),
            content: Some("hello".into()),
        },
        finish_reason: None,
    };
    let payload = tokenscavenger::api::openai::stream::format_sse_payload(&event);
    assert!(!payload.starts_with("data:"));
    assert_eq!(
        tokenscavenger::api::openai::stream::format_sse_payload(
            &tokenscavenger::api::openai::stream::StreamEvent::Done
        ),
        "[DONE]"
    );
}

#[test]
fn test_metrics_render_recorded_values() {
    tokenscavenger::metrics::prometheus::record_request("groq", "llama", "chat", "success");
    let rendered = tokenscavenger::metrics::prometheus::render_metrics();
    assert!(rendered.contains(
        r#"tokenscavenger_requests_total{provider="groq",model="llama",endpoint="chat",status="success"}"#
    ));
}

#[tokio::test]
async fn test_not_found_returns_404() {
    let (app, _state) = build_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
