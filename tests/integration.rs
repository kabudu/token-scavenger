//! Integration tests for TokenScavenger API endpoints.
//!
//! Uses the Axum test harness with an ephemeral SQLite database.

mod common;

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
use tokenscavenger::resilience::health::{HealthState, ProviderHealthState};
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

    let state = AppState::new(
        config,
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );

    let router = axum::Router::new()
        .route("/healthz", axum::routing::get(routes::healthz))
        .route("/readyz", axum::routing::get(routes::readyz))
        .route("/v1/models", axum::routing::get(routes::models))
        .route(
            "/v1/chat/completions",
            axum::routing::post(routes::chat_completions),
        )
        .route("/v1/embeddings", axum::routing::post(routes::embeddings))
        .route(
            "/admin/config",
            axum::routing::get(routes::admin_config).put(routes::admin_config_save),
        )
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
    assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
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
            embedding_support: Default::default(),
        }],
        ..Default::default()
    };
    let state = AppState::new(
        config,
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );
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
async fn test_admin_config_save_persists_model_priority() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();
    let config = Config::default();
    let state = AppState::new(
        config,
        pool.clone(),
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );

    // 1. Manually insert a model
    sqlx::query("INSERT INTO providers (provider_id, display_name) VALUES ('groq', 'Groq')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO models (provider_id, upstream_model_id, public_model_id, priority) VALUES ('groq', 'm1', 'm1', 100)").execute(&pool).await.unwrap();

    let app = axum::Router::new()
        .route(
            "/admin/config",
            axum::routing::put(routes::admin_config_save),
        )
        .with_state(state);

    // 2. Update priority via API
    let update_body = serde_json::json!({
        "models": [{
            "provider_id": "groq",
            "model_id": "m1",
            "priority": 500
        }]
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/admin/config")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&update_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // 3. Verify in DB
    let priority: i64 = sqlx::query_scalar(
        "SELECT priority FROM models WHERE provider_id = 'groq' AND upstream_model_id = 'm1'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(priority, 500);
}

#[tokio::test]
async fn test_admin_config_save_hot_reloads_server_auth_fields() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();
    let state = AppState::new(
        Config::default(),
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );
    let app = axum::Router::new()
        .route(
            "/admin/config",
            axum::routing::put(routes::admin_config_save),
        )
        .with_state(state.clone());

    let update_body = serde_json::json!({
        "server": {
            "allow_query_api_keys": true,
            "ui_session_auth": true,
            "ui_path": "/ui",
            "request_timeout_ms": 42_000
        }
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/admin/config")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&update_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let config = state.config();
    assert!(config.server.allow_query_api_keys);
    assert!(config.server.ui_session_auth);
    assert_eq!(config.server.ui_path, "/ui");
    assert_eq!(config.server.request_timeout_ms, 42_000);
}

#[tokio::test]
async fn test_admin_config_save_persists_provider_runtime_fields() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();
    let state = AppState::new(
        Config::default(),
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );
    let app = axum::Router::new()
        .route(
            "/admin/config",
            axum::routing::put(routes::admin_config_save),
        )
        .with_state(state.clone());

    let update_body = serde_json::json!({
        "providers": [{
            "id": "custom-runtime",
            "enabled": true,
            "base_url": "https://example.invalid/v1",
            "free_only": false
        }]
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/admin/config")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&update_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let row = sqlx::query_as::<_, (String, bool, Option<String>, bool)>(
        "SELECT provider_id, enabled, base_url, free_only FROM providers WHERE provider_id = 'custom-runtime'",
    )
    .fetch_one(&state.db)
    .await
    .unwrap();

    assert_eq!(row.0, "custom-runtime");
    assert!(row.1);
    assert_eq!(row.2.as_deref(), Some("https://example.invalid/v1"));
    assert!(!row.3);
}

#[tokio::test]
async fn test_admin_config_save_preserves_redacted_provider_api_key() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
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
            embedding_support: Default::default(),
        }],
        ..Default::default()
    };
    let state = AppState::new(
        config,
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );
    let app = axum::Router::new()
        .route(
            "/admin/config",
            axum::routing::get(routes::admin_config).put(routes::admin_config_save),
        )
        .with_state(state.clone());

    let response = app
        .clone()
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
    let redacted_config: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(redacted_config["providers"][0]["api_key"], "****_key");

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/admin/config")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&redacted_config).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let config = state.config();
    assert_eq!(
        config.providers[0].api_key.as_deref(),
        Some("gsk_super_secret_key")
    );
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
    let state = AppState::new(
        config,
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );
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
async fn test_query_string_api_key_requires_explicit_opt_in() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();

    let config = Config {
        server: ServerConfig {
            master_api_key: "secret".into(),
            allow_query_api_keys: false,
            ..Default::default()
        },
        ..Default::default()
    };
    let router = tokenscavenger::app::startup::build_router(AppState::new(
        config,
        pool.clone(),
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    ));

    let rejected = router
        .oneshot(
            Request::builder()
                .uri("/v1/models?api_key=secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::UNAUTHORIZED);

    let config = Config {
        server: ServerConfig {
            master_api_key: "secret".into(),
            allow_query_api_keys: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let router = tokenscavenger::app::startup::build_router(AppState::new(
        config,
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    ));
    let accepted = router
        .oneshot(
            Request::builder()
                .uri("/v1/models?api_key=secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(accepted.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_optional_ui_session_cookie_authenticates_browser_requests() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();
    let config = Config {
        server: ServerConfig {
            master_api_key: "secret".into(),
            ui_session_auth: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let router = tokenscavenger::app::startup::build_router(AppState::new(
        config,
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    ));

    let login = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/session")
                .method("POST")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::json!({"api_key": "secret"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = login
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(cookie.contains("HttpOnly"));

    let response = router
        .oneshot(
            Request::builder()
                .uri("/ui")
                .header("Cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_ui_redirects_to_login_when_session_auth_required() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();
    let config = Config {
        server: ServerConfig {
            master_api_key: "secret".into(),
            ui_session_auth: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let router = tokenscavenger::app::startup::build_router(AppState::new(
        config,
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    ));

    let response = router
        .clone()
        .oneshot(Request::builder().uri("/ui").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/ui/login");

    let login = router
        .oneshot(
            Request::builder()
                .uri("/ui/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_configured_cors_origin_is_allowed() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();
    let config = Config {
        server: ServerConfig {
            allowed_cors_origins: vec!["https://ops.example".into()],
            ..Default::default()
        },
        ..Default::default()
    };
    let router = tokenscavenger::app::startup::build_router(AppState::new(
        config,
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    ));

    let response = router
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .method("OPTIONS")
                .header("Origin", "https://ops.example")
                .header("Access-Control-Request-Method", "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("access-control-allow-origin")
            .unwrap(),
        "https://ops.example"
    );
}

#[tokio::test]
async fn test_admin_models_falls_back_to_curated_catalog() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();
    let state = AppState::new(
        Config::default(),
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );

    let models = tokenscavenger::discovery::merge::get_all_models(&state).await;
    let arr = models["models"].as_array().unwrap();
    assert!(!arr.is_empty());
    assert!(arr.iter().any(|model| model["source"] == "curated"));
}

#[tokio::test]
async fn test_startup_seeds_configured_providers_for_discovery_persistence() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("tokenscavenger-startup-{unique}"));
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("tokenscavenger.db");
    let config_path = dir.join("tokenscavenger.toml");
    std::fs::write(
        &config_path,
        format!(
            r#"
[server]
bind = "127.0.0.1:0"

[database]
path = "{}"

[[providers]]
id = "groq"
enabled = false
api_key = "test-key"
free_only = true
discover_models = false
"#,
            db_path.display()
        ),
    )
    .unwrap();

    let startup = tokenscavenger::app::startup::startup(&config_path)
        .await
        .unwrap();

    let row = sqlx::query_as::<_, (String, bool, bool)>(
        "SELECT provider_id, enabled, free_only FROM providers WHERE provider_id = 'groq'",
    )
    .fetch_one(&startup.state.db)
    .await
    .unwrap();

    assert_eq!(row.0, "groq");
    assert!(!row.1);
    assert!(row.2);

    let _ = startup.state.shutdown_tx.send(true);
    let handles = {
        let mut guard = startup.state.background_handles.lock().unwrap();
        guard.drain(..).collect::<Vec<_>>()
    };
    for handle in handles {
        handle.abort();
    }
    startup.state.db.close().await;
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
async fn test_route_plan_endpoint_explains_attempts() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();
    let mut config = Config::default();
    config.routing.provider_order = vec!["local".into()];
    config.providers = vec![tokenscavenger::config::schema::ProviderConfig {
        id: "local".into(),
        enabled: true,
        base_url: None,
        api_key: None,
        free_only: true,
        discover_models: false,
        embedding_support: Default::default(),
    }];
    let state = AppState::new(
        config,
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );
    state.provider_registry.init_from_config(&state).await;
    sqlx::query(
        "INSERT INTO providers (provider_id, display_name, enabled, base_url, free_only)
         VALUES (?, ?, 1, ?, 1)",
    )
    .bind("local")
    .bind("Local")
    .bind("http://127.0.0.1:1234/v1")
    .execute(&state.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO models (provider_id, upstream_model_id, public_model_id, enabled, free_tier, supports_chat, supports_embeddings, discovered_at, updated_at)
         VALUES (?, ?, ?, 1, 1, 1, 0, datetime('now'), datetime('now'))",
    )
    .bind("local")
    .bind("test-local")
    .bind("test-local")
    .execute(&state.db)
    .await
    .unwrap();
    let router = tokenscavenger::app::startup::build_router(state);

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/route-plan?model=test-local&endpoint=chat")
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
    assert_eq!(json["requested_model"], "test-local");
    assert!(
        json["attempts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|attempt| { attempt["provider_id"] == "local" && attempt["included"] == true })
    );

    let response = router
        .oneshot(
            Request::builder()
                .uri("/admin/route-plan?model=test-local&endpoint=embeddings")
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
    assert!(json["attempts"].as_array().unwrap().iter().any(|attempt| {
        attempt["provider_id"] == "local"
            && attempt["included"] == false
            && attempt["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("model endpoint capability"))
    }));
}

#[tokio::test]
async fn test_observability_endpoints_return_traces_incidents_and_redacted_bundle() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();
    let mut config = Config::default();
    config.providers = vec![tokenscavenger::config::schema::ProviderConfig {
        id: "local".into(),
        enabled: true,
        base_url: Some("http://127.0.0.1:11434/v1".into()),
        api_key: Some("sk-secret-observability-key".into()),
        free_only: true,
        discover_models: false,
        embedding_support: Default::default(),
    }];
    let state = AppState::new(
        config,
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );

    sqlx::query(
        "INSERT INTO request_log
         (request_id, endpoint_kind, requested_model, selected_provider_id, selected_model_id, status, http_status, latency_ms, fallback_count)
         VALUES ('req-ok', 'chat', 'fast:chat', 'local', 'llama3.2', 'success', 200, 42, 1)",
    )
    .execute(&state.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO usage_events
         (request_id, provider_id, model_id, input_tokens, output_tokens, estimated_cost_usd, cost_confidence, free_tier)
         VALUES ('req-ok', 'local', 'llama3.2', 12, 8, 0.0, 'free_tier', 1)",
    )
    .execute(&state.db)
    .await
    .unwrap();
    tokenscavenger::observability::record_event(
        &state,
        tokenscavenger::observability::TraceEventRecord {
            request_id: "req-ok",
            event_type: "route_plan",
            provider_id: None,
            model_id: None,
            outcome: Some("planned"),
            latency_ms: None,
            details: json!({"api_key": "sk-never-return-this", "candidate_count": 1}),
        },
    )
    .await;
    sqlx::query(
        "INSERT INTO request_log
         (request_id, endpoint_kind, requested_model, selected_provider_id, selected_model_id, status, http_status, latency_ms)
         VALUES ('req-429', 'chat', 'fast:chat', 'local', 'llama3.2', 'rate_limited', 429, 13)",
    )
    .execute(&state.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO provider_health_events (provider_id, health_state, breaker_state, event_type, details_json)
         VALUES ('local', 'unhealthy', 'open', 'passive_failure', '{\"reason\":\"boom\"}')",
    )
    .execute(&state.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO config_audit_log (actor, action, target_type) VALUES ('operator', 'config_update', 'config')",
    )
    .execute(&state.db)
    .await
    .unwrap();

    let router = tokenscavenger::app::startup::build_router(state);

    let summary_response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/observability/summary?period=24h")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(summary_response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(summary_response.into_body(), 65536)
        .await
        .unwrap();
    let summary: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(summary["request_count"], 2);
    assert_eq!(summary["rate_limit_count"], 1);
    assert_eq!(summary["fallback_count"], 1);
    assert_eq!(summary["total_tokens"], 20);

    let traces_response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/request-traces?limit=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(traces_response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(traces_response.into_body(), 65536)
        .await
        .unwrap();
    let traces: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(traces["traces"].as_array().unwrap().len() >= 2);

    let trace_response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/request-traces/req-ok")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(trace_response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(trace_response.into_body(), 65536)
        .await
        .unwrap();
    let trace: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(trace["request"]["request_id"], "req-ok");
    assert_eq!(trace["usage"][0]["input_tokens"], 12);
    assert_eq!(trace["events"][0]["details"]["api_key"], "****this");

    let incidents_response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/incidents?limit=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(incidents_response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(incidents_response.into_body(), 65536)
        .await
        .unwrap();
    let incidents: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        incidents["incidents"]
            .as_array()
            .unwrap()
            .iter()
            .any(|incident| {
                incident["kind"] == "provider_health" || incident["kind"] == "request_failure"
            })
    );

    let bundle_response = router
        .oneshot(
            Request::builder()
                .uri("/admin/diagnostics/bundle")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bundle_response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(bundle_response.into_body(), 262144)
        .await
        .unwrap();
    let bundle_text = String::from_utf8(body.to_vec()).unwrap();
    assert!(!bundle_text.contains("sk-secret-observability-key"));
    assert!(bundle_text.contains("****-key"));
}

#[tokio::test]
async fn test_config_save_creates_snapshot_and_rollback_restores_it() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();
    let mut config = Config::default();
    config.routing.free_first = true;
    let path =
        std::env::temp_dir().join(format!("tokenscavenger-test-{}.toml", uuid::Uuid::new_v4()));
    let state = AppState::new(
        config,
        pool,
        path.clone(),
        tokio::sync::broadcast::channel(1).0,
    );
    let router = tokenscavenger::app::startup::build_router(state);

    let save = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/config")
                .method("PUT")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::json!({"routing": {"free_first": false}}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(save.status(), StatusCode::OK);

    let rollback = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/admin/config/rollback")
                .method("POST")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::json!({"snapshot_id": 1}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rollback.status(), StatusCode::OK);

    let current = router
        .oneshot(
            Request::builder()
                .uri("/admin/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(current.into_body(), 65536)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["routing"]["free_first"], true);
    let _ = std::fs::remove_file(path.with_extension("overrides.toml"));
}

#[test]
fn test_runtime_overrides_restore_all_hot_reload_sections() {
    let path = std::env::temp_dir().join(format!(
        "tokenscavenger-override-{}.toml",
        uuid::Uuid::new_v4()
    ));
    let mut config = Config::default();
    config.server.master_api_key = "new-secret".into();
    config.server.allowed_cors_origins = vec!["https://ops.example".into()];
    config.server.ui_enabled = false;
    config.server.ui_session_auth = true;
    config.database.max_connections = 3;
    config.metrics.enabled = false;

    tokenscavenger::config::overrides::save_runtime_overrides(&path, &config).unwrap();
    let loaded = tokenscavenger::config::overrides::load_runtime_overrides(&path).unwrap();

    assert_eq!(loaded.server.master_api_key, "new-secret");
    assert_eq!(
        loaded.server.allowed_cors_origins,
        vec!["https://ops.example"]
    );
    assert!(!loaded.server.ui_enabled);
    assert!(loaded.server.ui_session_auth);
    assert_eq!(loaded.database.max_connections, 3);
    assert!(!loaded.metrics.enabled);

    let _ = std::fs::remove_file(path.with_extension("overrides.toml"));
}

#[tokio::test]
async fn test_rate_limited_health_state_does_not_globally_block_route_filter() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let mut config = Config::default();
    config.routing.provider_order = vec!["groq".into()];
    let state = AppState::new(
        config,
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );
    state.health_states.insert(
        "groq".into(),
        ProviderHealthState {
            state: HealthState::RateLimited,
            ..ProviderHealthState::new()
        },
    );

    let plan = vec![tokenscavenger::router::selection::RouteAttempt {
        provider_id: "groq".into(),
        model_id: "llama".into(),
        priority: 0,
    }];
    assert_eq!(
        tokenscavenger::router::selection::filter_by_health(plan, &state).len(),
        1
    );
}

#[tokio::test]
async fn test_ui_smoke_pages_include_accessibility_and_analytics_surfaces() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();
    let app = tokenscavenger::app::startup::build_router(AppState::new(
        Config::default(),
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    ));
    for path in ["/ui", "/ui/routing", "/ui/models", "/ui/config", "/ui/logs"] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "path {path}");
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let html = String::from_utf8(body.to_vec()).unwrap();
        assert!(html.contains("<aside"));
        if path == "/ui/routing" {
            assert!(html.contains("Route Plan"));
        }
        if path == "/ui/config" {
            assert!(html.contains("Rollback"));
        }
        if path == "/ui/models" {
            assert!(html.contains("Model Catalog"));
            assert!(html.contains("fetch('/admin/models'"));
        }
        if path == "/ui/logs" {
            assert!(html.contains("aria-live"));
        }
        if path == "/ui" {
            assert!(html.contains("Requests"));
            assert!(html.contains("Avg Latency"));
        }
    }
}

#[tokio::test]
async fn test_provider_contract_matrix_and_failure_classification() {
    let provider_ids = [
        "groq",
        "google",
        "openrouter",
        "cloudflare",
        "cerebras",
        "nvidia",
        "cohere",
        "mistral",
        "github-models",
        "huggingface",
        "zai",
        "siliconflow",
        "deepseek",
        "xai",
        "local",
        "ollama",
        "llama-cpp",
        "lmstudio",
    ];
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let mut config = Config::default();
    config.providers = provider_ids
        .iter()
        .map(|id| tokenscavenger::config::schema::ProviderConfig {
            id: (*id).into(),
            enabled: true,
            base_url: None,
            api_key: None,
            free_only: true,
            discover_models: false,
            embedding_support: Default::default(),
        })
        .collect();
    let state = AppState::new(
        config,
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );
    state.provider_registry.init_from_config(&state).await;
    let adapters = state.provider_registry.list_all().await;
    assert_eq!(adapters.len(), provider_ids.len());
    for adapter in adapters {
        assert!(!adapter.provider_id().is_empty());
        assert!(!adapter.display_name().is_empty());
        assert!(
            adapter.supports_endpoint(
                &tokenscavenger::providers::traits::EndpointKind::ChatCompletions
            )
        );
        assert!(adapter.capabilities().docs_url.is_some() || adapter.capabilities().has_quirks);
    }

    use tokenscavenger::providers::traits::ProviderError;
    assert!(matches!(
        tokenscavenger::providers::shared::classify_error(429, "slow down"),
        ProviderError::RateLimited { .. }
    ));
    assert!(matches!(
        tokenscavenger::providers::shared::classify_error(
            413,
            r#"{"error":{"message":"Request too large on tokens per minute (TPM): Limit 12000, Requested 20092","type":"tokens","code":"rate_limit_exceeded"}}"#
        ),
        ProviderError::RateLimited { .. }
    ));
    assert!(matches!(
        tokenscavenger::providers::shared::classify_error(401, "bad key"),
        ProviderError::Auth(_)
    ));
    assert!(matches!(
        tokenscavenger::providers::shared::classify_error(500, "boom"),
        ProviderError::Other(_)
    ));
}

#[tokio::test]
async fn test_local_openai_adapter_auto_probes_embeddings() {
    let (base_url, handle) = common::start_mock_server(common::MockProviderState {
        usage_tokens: (3, 0),
        ..Default::default()
    })
    .await;

    let adapter = tokenscavenger::providers::local::LocalOpenAiAdapter;
    let config = tokenscavenger::config::schema::ProviderConfig {
        id: "local".into(),
        enabled: true,
        base_url: Some(format!("{base_url}/v1")),
        api_key: None,
        free_only: true,
        discover_models: true,
        embedding_support: Default::default(),
    };
    let ctx = tokenscavenger::providers::traits::ProviderContext {
        base_url: tokenscavenger::providers::traits::ProviderAdapter::base_url(&adapter, &config),
        api_key: None,
        config: std::sync::Arc::new(config),
        client: reqwest::Client::new(),
    };

    assert!(
        tokenscavenger::providers::traits::ProviderAdapter::supports_endpoint(
            &adapter,
            &tokenscavenger::providers::traits::EndpointKind::Embeddings,
        )
    );
    let models =
        tokenscavenger::providers::traits::ProviderAdapter::discover_models(&adapter, &ctx)
            .await
            .unwrap();
    assert!(
        models[0]
            .endpoint_compatibility
            .contains(&"embeddings".to_string())
    );

    let response = tokenscavenger::providers::traits::ProviderAdapter::embeddings(
        &adapter,
        &ctx,
        tokenscavenger::api::openai::embeddings::NormalizedEmbeddingsRequest {
            model: "local-embed".into(),
            input: vec!["hello".into()],
            encoding_format: None,
            user: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(response.provider_id, "local");
    assert_eq!(response.model_id, "local-embed");
    assert_eq!(response.data[0].embedding, vec![0.125, -0.25, 0.5]);
    assert_eq!(response.usage.prompt_tokens, 3);

    handle.abort();
}

#[tokio::test]
async fn test_local_openai_adapter_does_not_advertise_embeddings_when_probe_fails() {
    let (base_url, handle) = common::start_mock_server(common::MockProviderState {
        embeddings_status_code: 501,
        embeddings_error_body: Some("embeddings disabled".into()),
        ..Default::default()
    })
    .await;

    let adapter = tokenscavenger::providers::local::LocalOpenAiAdapter;
    let config = tokenscavenger::config::schema::ProviderConfig {
        id: "local".into(),
        enabled: true,
        base_url: Some(format!("{base_url}/v1")),
        api_key: None,
        free_only: true,
        discover_models: true,
        embedding_support: tokenscavenger::config::schema::ProviderEmbeddingSupport::Auto,
    };
    let ctx = tokenscavenger::providers::traits::ProviderContext {
        base_url: tokenscavenger::providers::traits::ProviderAdapter::base_url(&adapter, &config),
        api_key: None,
        config: std::sync::Arc::new(config),
        client: reqwest::Client::new(),
    };

    let models =
        tokenscavenger::providers::traits::ProviderAdapter::discover_models(&adapter, &ctx)
            .await
            .unwrap();
    assert!(
        !models[0]
            .endpoint_compatibility
            .contains(&"embeddings".to_string())
    );

    handle.abort();
}

#[tokio::test]
async fn test_local_openai_adapter_embedding_support_can_be_configured() {
    let (base_url, handle) = common::start_mock_server(common::MockProviderState {
        embeddings_status_code: 501,
        embeddings_error_body: Some("embeddings disabled".into()),
        ..Default::default()
    })
    .await;

    let adapter = tokenscavenger::providers::local::LocalOpenAiAdapter;
    let mut config = tokenscavenger::config::schema::ProviderConfig {
        id: "local".into(),
        enabled: true,
        base_url: Some(format!("{base_url}/v1")),
        api_key: None,
        free_only: true,
        discover_models: true,
        embedding_support: tokenscavenger::config::schema::ProviderEmbeddingSupport::Enabled,
    };
    let mut ctx = tokenscavenger::providers::traits::ProviderContext {
        base_url: tokenscavenger::providers::traits::ProviderAdapter::base_url(&adapter, &config),
        api_key: None,
        config: std::sync::Arc::new(config.clone()),
        client: reqwest::Client::new(),
    };

    let forced_models =
        tokenscavenger::providers::traits::ProviderAdapter::discover_models(&adapter, &ctx)
            .await
            .unwrap();
    assert!(
        forced_models[0]
            .endpoint_compatibility
            .contains(&"embeddings".to_string())
    );

    config.embedding_support = tokenscavenger::config::schema::ProviderEmbeddingSupport::Disabled;
    ctx.config = std::sync::Arc::new(config);
    let disabled_models =
        tokenscavenger::providers::traits::ProviderAdapter::discover_models(&adapter, &ctx)
            .await
            .unwrap();
    assert!(
        !disabled_models[0]
            .endpoint_compatibility
            .contains(&"embeddings".to_string())
    );

    handle.abort();
}

#[tokio::test]
async fn test_local_openai_adapter_auto_probes_embeddings_with_bounded_concurrency() {
    let (base_url, handle) = common::start_mock_server(common::MockProviderState {
        delay_ms: 250,
        model_ids: vec![
            "local-a".into(),
            "local-b".into(),
            "local-c".into(),
            "local-d".into(),
        ],
        ..Default::default()
    })
    .await;

    let adapter = tokenscavenger::providers::local::LocalOpenAiAdapter;
    let config = tokenscavenger::config::schema::ProviderConfig {
        id: "local".into(),
        enabled: true,
        base_url: Some(format!("{base_url}/v1")),
        api_key: None,
        free_only: true,
        discover_models: true,
        embedding_support: tokenscavenger::config::schema::ProviderEmbeddingSupport::Auto,
    };
    let ctx = tokenscavenger::providers::traits::ProviderContext {
        base_url: tokenscavenger::providers::traits::ProviderAdapter::base_url(&adapter, &config),
        api_key: None,
        config: std::sync::Arc::new(config),
        client: reqwest::Client::new(),
    };

    let started_at = std::time::Instant::now();
    let models =
        tokenscavenger::providers::traits::ProviderAdapter::discover_models(&adapter, &ctx)
            .await
            .unwrap();
    let elapsed = started_at.elapsed();

    assert_eq!(models.len(), 4);
    assert!(models.iter().all(|model| {
        model
            .endpoint_compatibility
            .contains(&"embeddings".to_string())
    }));
    assert!(
        elapsed < std::time::Duration::from_millis(800),
        "local embeddings probes should be bounded-concurrent, elapsed={elapsed:?}"
    );

    handle.abort();
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

#[tokio::test]
async fn test_paid_deepseek_usage_records_nonzero_cost() {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("Failed to create in-memory SQLite");

    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");
    tokenscavenger::usage::pricing_catalog::seed_builtin_pricing(&pool)
        .await
        .expect("Failed to seed pricing");

    let state = AppState::new(
        Config::default(),
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );

    tokenscavenger::usage::accounting::record_usage(
        &state,
        tokenscavenger::usage::accounting::UsageRecord {
            provider_id: "deepseek",
            model_id: "deepseek-chat",
            requested_model: "deepseek-chat",
            usage: Some(&tokenscavenger::api::openai::chat::UsageResponse {
                prompt_tokens: 1_000,
                completion_tokens: 500,
                total_tokens: 1_500,
                prompt_cache_hit_tokens: Some(400),
                prompt_cache_miss_tokens: Some(600),
                reasoning_tokens: None,
            }),
            latency_ms: 42,
            free_tier: false,
            request_id: "paid-req-1",
            endpoint_kind: "chat",
            streaming: false,
        },
    )
    .await
    .expect("Failed to record usage");

    let row: (f64, String, Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT estimated_cost_usd, cost_confidence, cached_input_tokens, cache_miss_input_tokens
         FROM usage_events WHERE request_id = ?",
    )
    .bind("paid-req-1")
    .fetch_one(&state.db)
    .await
    .expect("Failed to query paid usage");

    assert!(row.0 > 0.0);
    assert_eq!(row.1, "provider_published");
    assert_eq!(row.2, Some(400));
    assert_eq!(row.3, Some(600));
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
            embedding_support: Default::default(),
        }],
        ..Default::default()
    };
    let state = AppState::new(
        config,
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );
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
#[tokio::test]
async fn test_multi_model_group_resolution() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();

    // 1. Insert a model group with multiple targets
    sqlx::query("INSERT INTO model_groups (name, target_json, enabled) VALUES ('my-group', '[\"m1\", \"m2\"]', 1)")
        .execute(&pool).await.unwrap();

    let config = Config::default();
    let state = AppState::new(
        config,
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );

    // 2. Resolve model group
    let resolved = tokenscavenger::router::model_groups::resolve_model_group(&state, "my-group")
        .await
        .unwrap();
    assert_eq!(resolved, vec!["m1".to_string(), "m2".to_string()]);
}

#[tokio::test]
async fn test_provider_qualified_model_group_resolution() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO model_groups (name, target_json, enabled) VALUES ('agentic', '[{\"provider\":\"nvidia\",\"model\":\"google/gemma-4-31b-it\"}, \"gemini-2.5-flash\"]', 1)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let mut config = Config::default();
    config.routing.provider_order = vec!["google".into(), "nvidia".into()];
    config.providers = vec![
        tokenscavenger::config::schema::ProviderConfig {
            id: "google".into(),
            enabled: true,
            ..Default::default()
        },
        tokenscavenger::config::schema::ProviderConfig {
            id: "nvidia".into(),
            enabled: true,
            ..Default::default()
        },
    ];

    let state = AppState::new(
        config,
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );
    state.provider_registry.init_from_config(&state).await;

    let targets =
        tokenscavenger::router::model_groups::resolve_model_group_targets(&state, "agentic")
            .await
            .unwrap();
    assert_eq!(targets[0].provider_id.as_deref(), Some("nvidia"));
    assert_eq!(targets[0].model_id, "google/gemma-4-31b-it");
    assert_eq!(targets[1].provider_id, None);
    assert_eq!(targets[1].model_id, "gemini-2.5-flash");

    let registry = &state.provider_registry;
    let policy = tokenscavenger::router::policy::RoutePolicy::from_config(&state.config());
    let mut plan = Vec::new();
    for target in &targets {
        let model_plan = tokenscavenger::router::selection::build_attempt_plan_for_target(
            &policy,
            registry,
            target,
            tokenscavenger::providers::traits::EndpointKind::ChatCompletions,
        )
        .await;
        plan.extend(model_plan);
    }
    tokenscavenger::router::selection::assign_attempt_priorities(&mut plan);

    assert_eq!(plan[0].provider_id, "nvidia");
    assert_eq!(plan[0].model_id, "google/gemma-4-31b-it");
    assert!(
        plan.iter().any(
            |attempt| attempt.provider_id == "google" && attempt.model_id == "gemini-2.5-flash"
        )
    );
}

#[tokio::test]
async fn test_model_group_fallback_logic() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();

    // Model group points to two models. We want to verify the engine builds a combined plan.
    sqlx::query(
        "INSERT INTO model_groups (name, target_json, enabled) VALUES ('multi', '[\"m1\", \"m2\"]', 1)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let mut config = Config::default();
    config.routing.provider_order = vec!["groq".into()];
    config.providers = vec![tokenscavenger::config::schema::ProviderConfig {
        id: "groq".into(),
        enabled: true,
        ..Default::default()
    }];

    let state = AppState::new(
        config,
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );
    state.provider_registry.init_from_config(&state).await;

    // Build plan for the model group
    let resolved = tokenscavenger::router::model_groups::resolve_model_group(&state, "multi")
        .await
        .unwrap();
    let registry = &state.provider_registry;
    let policy = tokenscavenger::router::policy::RoutePolicy::from_config(&state.config());

    let mut plan = Vec::new();
    for model in resolved {
        let model_plan = tokenscavenger::router::selection::build_attempt_plan(
            &policy,
            registry,
            &model,
            tokenscavenger::providers::traits::EndpointKind::ChatCompletions,
        )
        .await;
        plan.extend(model_plan);
    }

    // Should have 2 attempts: groq/m1 and groq/m2
    assert_eq!(plan.len(), 2);
    assert_eq!(plan[0].provider_id, "groq");
    assert_eq!(plan[0].model_id, "m1");
    assert_eq!(plan[1].provider_id, "groq");
    assert_eq!(plan[1].model_id, "m2");
}

#[tokio::test]
async fn test_tool_requests_keep_operator_order_for_tool_capable_attempts() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();
    let state = AppState::new(
        Config::default(),
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );

    for provider in ["mistral", "groq"] {
        sqlx::query(
            "INSERT INTO providers (provider_id, display_name, enabled, free_only)
             VALUES (?, ?, 1, 1)",
        )
        .bind(provider)
        .bind(provider)
        .execute(&state.db)
        .await
        .unwrap();
    }

    for (provider, model) in [
        ("mistral", "mistral-medium-3.5"),
        ("mistral", "devstral-latest"),
        ("groq", "llama-3.3-70b-versatile"),
    ] {
        sqlx::query(
            "INSERT INTO models (provider_id, upstream_model_id, public_model_id, enabled, supports_tools)
             VALUES (?, ?, ?, 1, 1)",
        )
        .bind(provider)
        .bind(model)
        .bind(model)
        .execute(&state.db)
        .await
        .unwrap();
    }

    let plan = vec![
        tokenscavenger::router::selection::RouteAttempt {
            provider_id: "mistral".into(),
            model_id: "mistral-medium-3.5".into(),
            priority: 0,
        },
        tokenscavenger::router::selection::RouteAttempt {
            provider_id: "mistral".into(),
            model_id: "devstral-latest".into(),
            priority: 1,
        },
        tokenscavenger::router::selection::RouteAttempt {
            provider_id: "groq".into(),
            model_id: "llama-3.3-70b-versatile".into(),
            priority: 2,
        },
    ];

    let prioritized =
        tokenscavenger::router::selection::prioritize_for_tool_use(plan, &state).await;

    assert_eq!(prioritized[0].provider_id, "mistral");
    assert_eq!(prioritized[0].model_id, "mistral-medium-3.5");
    assert_eq!(prioritized[1].model_id, "devstral-latest");
    assert_eq!(prioritized[2].provider_id, "groq");
}

#[tokio::test]
async fn test_policy_min_cost_prefers_free_route_over_paid_latency() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();

    let mut config = Config::default();
    config.routing.objective = tokenscavenger::config::schema::RoutingObjective::MinCost;
    config.routing.allow_paid_fallback = true;
    config.providers = vec![
        tokenscavenger::config::schema::ProviderConfig {
            id: "deepseek".into(),
            free_only: false,
            ..Default::default()
        },
        tokenscavenger::config::schema::ProviderConfig {
            id: "groq".into(),
            free_only: true,
            ..Default::default()
        },
    ];
    let state = AppState::new(
        config,
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );
    seed_policy_provider_model(&state, "deepseek", "deepseek-chat", false, 1, true, true).await;
    seed_policy_provider_model(
        &state,
        "groq",
        "llama-3.3-70b-versatile",
        true,
        100,
        true,
        true,
    )
    .await;
    seed_policy_rate(&state, "deepseek", "deepseek-chat", 1.0, 3.0).await;
    seed_request_log(
        &state,
        "fast-paid",
        "deepseek",
        "deepseek-chat",
        "success",
        25,
    )
    .await;
    seed_request_log(
        &state,
        "slow-free",
        "groq",
        "llama-3.3-70b-versatile",
        "success",
        900,
    )
    .await;

    let policy = tokenscavenger::router::policy::RoutePolicy::from_config(&state.config());
    let plan = vec![
        tokenscavenger::router::selection::RouteAttempt {
            provider_id: "deepseek".into(),
            model_id: "deepseek-chat".into(),
            priority: 0,
        },
        tokenscavenger::router::selection::RouteAttempt {
            provider_id: "groq".into(),
            model_id: "llama-3.3-70b-versatile".into(),
            priority: 1,
        },
    ];

    let planned = tokenscavenger::router::selection::apply_policy_engine(
        plan,
        &state,
        &policy,
        "agentic",
        tokenscavenger::providers::traits::EndpointKind::ChatCompletions,
        tokenscavenger::router::selection::TokenEstimate {
            input_tokens: 1_000,
            output_tokens: 1_000,
        },
    )
    .await;

    assert_eq!(planned[0].provider_id, "groq");
    assert_eq!(planned[1].provider_id, "deepseek");
}

#[tokio::test]
async fn test_policy_hard_budget_filters_over_cap_and_unknown_paid_price() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();

    let mut config = Config::default();
    config.routing.allow_paid_fallback = true;
    config.routing.budgets.max_cost_per_request_usd = Some(0.0001);
    config.providers = vec![
        tokenscavenger::config::schema::ProviderConfig {
            id: "deepseek".into(),
            free_only: false,
            ..Default::default()
        },
        tokenscavenger::config::schema::ProviderConfig {
            id: "xai".into(),
            free_only: false,
            ..Default::default()
        },
        tokenscavenger::config::schema::ProviderConfig {
            id: "groq".into(),
            free_only: true,
            ..Default::default()
        },
    ];
    let state = AppState::new(
        config,
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );
    seed_policy_provider_model(&state, "deepseek", "deepseek-chat", false, 10, true, true).await;
    seed_policy_provider_model(&state, "xai", "unknown-paid", false, 20, true, true).await;
    seed_policy_provider_model(&state, "groq", "free-model", true, 30, true, true).await;
    seed_policy_rate(&state, "deepseek", "deepseek-chat", 20.0, 20.0).await;

    let policy = tokenscavenger::router::policy::RoutePolicy::from_config(&state.config());
    let plan = vec![
        tokenscavenger::router::selection::RouteAttempt {
            provider_id: "deepseek".into(),
            model_id: "deepseek-chat".into(),
            priority: 0,
        },
        tokenscavenger::router::selection::RouteAttempt {
            provider_id: "xai".into(),
            model_id: "unknown-paid".into(),
            priority: 1,
        },
        tokenscavenger::router::selection::RouteAttempt {
            provider_id: "groq".into(),
            model_id: "free-model".into(),
            priority: 2,
        },
    ];

    let explanations = tokenscavenger::router::selection::explain_policy_plan(
        plan,
        &state,
        &policy,
        "agentic",
        tokenscavenger::providers::traits::EndpointKind::ChatCompletions,
        tokenscavenger::router::selection::TokenEstimate {
            input_tokens: 10_000,
            output_tokens: 10_000,
        },
    )
    .await;

    let included = explanations
        .iter()
        .filter(|entry| entry.included)
        .map(|entry| entry.attempt.provider_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(included, vec!["groq"]);
    assert!(
        explanations
            .iter()
            .find(|entry| entry.attempt.provider_id == "deepseek")
            .unwrap()
            .reasons
            .iter()
            .any(|reason| reason.contains("per-request budget"))
    );
    assert!(
        explanations
            .iter()
            .find(|entry| entry.attempt.provider_id == "xai")
            .unwrap()
            .reasons
            .iter()
            .any(|reason| reason.contains("paid price is unknown"))
    );
}

#[tokio::test]
async fn test_policy_daily_provider_and_model_group_budgets_filter_projected_spend() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();

    let mut config = Config::default();
    config.routing.allow_paid_fallback = true;
    config.routing.budgets.max_cost_per_day_usd = Some(0.03);
    config
        .routing
        .budgets
        .max_cost_per_provider_per_day_usd
        .insert("deepseek".into(), 0.01);
    config
        .routing
        .budgets
        .max_cost_per_model_group_per_day_usd
        .insert("agentic".into(), 0.02);
    config.providers = vec![
        tokenscavenger::config::schema::ProviderConfig {
            id: "deepseek".into(),
            free_only: false,
            ..Default::default()
        },
        tokenscavenger::config::schema::ProviderConfig {
            id: "xai".into(),
            free_only: false,
            ..Default::default()
        },
    ];
    let state = AppState::new(
        config,
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );
    seed_policy_provider_model(&state, "deepseek", "deepseek-chat", false, 10, true, true).await;
    seed_policy_provider_model(&state, "xai", "grok-3-mini", false, 20, true, true).await;
    seed_policy_rate(&state, "deepseek", "deepseek-chat", 1.0, 1.0).await;
    seed_policy_rate(&state, "xai", "grok-3-mini", 1.0, 1.0).await;
    seed_success_usage(
        &state,
        "spent-deepseek",
        "agentic",
        "deepseek",
        "deepseek-chat",
        0.0095,
    )
    .await;
    seed_success_usage(
        &state,
        "spent-agentic",
        "agentic",
        "xai",
        "grok-3-mini",
        0.0195,
    )
    .await;

    let policy = tokenscavenger::router::policy::RoutePolicy::from_config(&state.config());
    let token_estimate = tokenscavenger::router::selection::TokenEstimate {
        input_tokens: 1_000,
        output_tokens: 1_000,
    };
    let plan = vec![
        tokenscavenger::router::selection::RouteAttempt {
            provider_id: "deepseek".into(),
            model_id: "deepseek-chat".into(),
            priority: 0,
        },
        tokenscavenger::router::selection::RouteAttempt {
            provider_id: "xai".into(),
            model_id: "grok-3-mini".into(),
            priority: 1,
        },
    ];

    let explanations = tokenscavenger::router::selection::explain_policy_plan(
        plan,
        &state,
        &policy,
        "agentic",
        tokenscavenger::providers::traits::EndpointKind::ChatCompletions,
        token_estimate,
    )
    .await;

    assert!(explanations.iter().all(|entry| !entry.included));
    assert!(explanations.iter().any(|entry| {
        entry
            .reasons
            .iter()
            .any(|reason| reason.starts_with("filtered by daily budget:"))
    }));
    assert!(
        explanations
            .iter()
            .find(|entry| entry.attempt.provider_id == "deepseek")
            .unwrap()
            .reasons
            .iter()
            .any(|reason| reason.contains("provider daily budget"))
    );
    assert!(
        explanations
            .iter()
            .find(|entry| entry.attempt.provider_id == "xai")
            .unwrap()
            .reasons
            .iter()
            .any(|reason| reason.contains("model-group daily budget"))
    );
}

#[tokio::test]
async fn test_policy_tie_breaking_preserves_operator_priority() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();

    let mut config = Config::default();
    config.routing.objective = tokenscavenger::config::schema::RoutingObjective::Balanced;
    config.providers = vec![
        tokenscavenger::config::schema::ProviderConfig {
            id: "groq".into(),
            free_only: true,
            ..Default::default()
        },
        tokenscavenger::config::schema::ProviderConfig {
            id: "google".into(),
            free_only: true,
            ..Default::default()
        },
    ];
    let state = AppState::new(
        config,
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );
    seed_policy_provider_model(&state, "groq", "same", true, 100, true, true).await;
    seed_policy_provider_model(&state, "google", "same", true, 100, true, true).await;

    let policy = tokenscavenger::router::policy::RoutePolicy::from_config(&state.config());
    let plan = vec![
        tokenscavenger::router::selection::RouteAttempt {
            provider_id: "google".into(),
            model_id: "same".into(),
            priority: 0,
        },
        tokenscavenger::router::selection::RouteAttempt {
            provider_id: "groq".into(),
            model_id: "same".into(),
            priority: 1,
        },
    ];

    let planned = tokenscavenger::router::selection::apply_policy_engine(
        plan,
        &state,
        &policy,
        "same",
        tokenscavenger::providers::traits::EndpointKind::ChatCompletions,
        tokenscavenger::router::selection::TokenEstimate {
            input_tokens: 100,
            output_tokens: 100,
        },
    )
    .await;

    assert_eq!(planned[0].provider_id, "google");
    assert_eq!(planned[1].provider_id, "groq");
}

#[tokio::test]
async fn test_policy_local_only_filters_remote_providers() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();

    let mut config = Config::default();
    config.routing.objective = tokenscavenger::config::schema::RoutingObjective::LocalOnly;
    config.providers = vec![
        tokenscavenger::config::schema::ProviderConfig {
            id: "remote".into(),
            base_url: Some("https://api.example.test".into()),
            free_only: true,
            ..Default::default()
        },
        tokenscavenger::config::schema::ProviderConfig {
            id: "local".into(),
            base_url: Some("http://127.0.0.1:11434/v1".into()),
            free_only: true,
            ..Default::default()
        },
    ];
    let state = AppState::new(
        config,
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );
    seed_policy_provider_model(&state, "remote", "same", true, 1, true, true).await;
    seed_policy_provider_model(&state, "local", "same", true, 100, true, true).await;

    let policy = tokenscavenger::router::policy::RoutePolicy::from_config(&state.config());
    let plan = vec![
        tokenscavenger::router::selection::RouteAttempt {
            provider_id: "remote".into(),
            model_id: "same".into(),
            priority: 0,
        },
        tokenscavenger::router::selection::RouteAttempt {
            provider_id: "local".into(),
            model_id: "same".into(),
            priority: 1,
        },
    ];

    let planned = tokenscavenger::router::selection::apply_policy_engine(
        plan,
        &state,
        &policy,
        "same",
        tokenscavenger::providers::traits::EndpointKind::ChatCompletions,
        tokenscavenger::router::selection::TokenEstimate {
            input_tokens: 100,
            output_tokens: 100,
        },
    )
    .await;

    assert_eq!(planned.len(), 1);
    assert_eq!(planned[0].provider_id, "local");
}

#[tokio::test]
async fn test_policy_quality_first_considers_context_window() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();

    let mut config = Config::default();
    config.routing.objective = tokenscavenger::config::schema::RoutingObjective::QualityFirst;
    config.providers = vec![
        tokenscavenger::config::schema::ProviderConfig {
            id: "small-context".into(),
            free_only: true,
            ..Default::default()
        },
        tokenscavenger::config::schema::ProviderConfig {
            id: "large-context".into(),
            free_only: true,
            ..Default::default()
        },
    ];
    let state = AppState::new(
        config,
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );
    seed_policy_provider_model(&state, "small-context", "model", true, 100, true, true).await;
    seed_policy_provider_model(&state, "large-context", "model", true, 100, true, true).await;
    sqlx::query(
        "UPDATE models SET metadata_json = ? WHERE provider_id = ? AND upstream_model_id = ?",
    )
    .bind(serde_json::json!({"context_window": 8192}).to_string())
    .bind("small-context")
    .bind("model")
    .execute(&state.db)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE models SET metadata_json = ? WHERE provider_id = ? AND upstream_model_id = ?",
    )
    .bind(serde_json::json!({"context_window": 2_000_000}).to_string())
    .bind("large-context")
    .bind("model")
    .execute(&state.db)
    .await
    .unwrap();

    let policy = tokenscavenger::router::policy::RoutePolicy::from_config(&state.config());
    let plan = vec![
        tokenscavenger::router::selection::RouteAttempt {
            provider_id: "small-context".into(),
            model_id: "model".into(),
            priority: 0,
        },
        tokenscavenger::router::selection::RouteAttempt {
            provider_id: "large-context".into(),
            model_id: "model".into(),
            priority: 1,
        },
    ];

    let planned = tokenscavenger::router::selection::apply_policy_engine(
        plan,
        &state,
        &policy,
        "long-context-agent",
        tokenscavenger::providers::traits::EndpointKind::ChatCompletions,
        tokenscavenger::router::selection::TokenEstimate {
            input_tokens: 50_000,
            output_tokens: 4_000,
        },
    )
    .await;

    assert_eq!(planned[0].provider_id, "large-context");
}

#[tokio::test]
async fn test_policy_quality_first_agentic_harness_prefers_tool_json_capable_route() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();

    let mut config = Config::default();
    config.routing.objective = tokenscavenger::config::schema::RoutingObjective::QualityFirst;
    config.providers = vec![
        tokenscavenger::config::schema::ProviderConfig {
            id: "mistral".into(),
            free_only: true,
            ..Default::default()
        },
        tokenscavenger::config::schema::ProviderConfig {
            id: "groq".into(),
            free_only: true,
            ..Default::default()
        },
    ];
    let state = AppState::new(
        config,
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );
    seed_policy_provider_model(&state, "mistral", "basic-chat", true, 10, false, false).await;
    seed_policy_provider_model(
        &state,
        "groq",
        "hermes-agentic-tools",
        true,
        100,
        true,
        true,
    )
    .await;

    let policy = tokenscavenger::router::policy::RoutePolicy::from_config(&state.config());
    let plan = vec![
        tokenscavenger::router::selection::RouteAttempt {
            provider_id: "mistral".into(),
            model_id: "basic-chat".into(),
            priority: 0,
        },
        tokenscavenger::router::selection::RouteAttempt {
            provider_id: "groq".into(),
            model_id: "hermes-agentic-tools".into(),
            priority: 1,
        },
    ];

    let planned = tokenscavenger::router::selection::apply_policy_engine(
        plan,
        &state,
        &policy,
        "hermes-agent",
        tokenscavenger::providers::traits::EndpointKind::ChatCompletions,
        tokenscavenger::router::selection::TokenEstimate {
            input_tokens: 2_000,
            output_tokens: 1_000,
        },
    )
    .await;

    assert_eq!(planned[0].provider_id, "groq");
    assert_eq!(planned[0].model_id, "hermes-agentic-tools");
}

#[tokio::test]
async fn test_model_intelligence_filters_context_and_vision_requirements() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();

    let state = AppState::new(
        Config::default(),
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );
    seed_policy_provider_model(&state, "small", "model", true, 100, true, true).await;
    seed_policy_provider_model(&state, "vision", "model", true, 100, true, true).await;
    sqlx::query("UPDATE models SET metadata_json = ?, supports_vision = ? WHERE provider_id = ?")
        .bind(serde_json::json!({"context_window": 4096}).to_string())
        .bind(false)
        .bind("small")
        .execute(&state.db)
        .await
        .unwrap();
    sqlx::query("UPDATE models SET metadata_json = ?, supports_vision = ? WHERE provider_id = ?")
        .bind(serde_json::json!({"context_window": 128000, "supports_vision": true}).to_string())
        .bind(true)
        .bind("vision")
        .execute(&state.db)
        .await
        .unwrap();

    let plan = vec![
        tokenscavenger::router::selection::RouteAttempt {
            provider_id: "small".into(),
            model_id: "model".into(),
            priority: 0,
        },
        tokenscavenger::router::selection::RouteAttempt {
            provider_id: "vision".into(),
            model_id: "model".into(),
            priority: 1,
        },
    ];

    let filtered = tokenscavenger::discovery::model_intelligence::filter_by_model_intelligence(
        plan,
        &state,
        tokenscavenger::discovery::model_intelligence::ModelRequestRequirements {
            requires_tools: false,
            requires_json_mode: false,
            requires_vision: true,
            required_context_tokens: Some(10_000),
        },
    )
    .await;

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].provider_id, "vision");
}

#[tokio::test]
async fn test_smart_model_groups_are_seeded_without_overwriting_operator_groups() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO model_groups (name, target_json, enabled) VALUES ('fast:chat', '[\"operator-model\"]', 1)",
    )
    .execute(&pool)
    .await
    .unwrap();

    tokenscavenger::discovery::model_intelligence::seed_smart_model_groups(&pool).await;

    let fast = sqlx::query_as::<_, (String,)>(
        "SELECT target_json FROM model_groups WHERE name = 'fast:chat'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let reasoning = sqlx::query_as::<_, (String,)>(
        "SELECT target_json FROM model_groups WHERE name = 'reasoning:deep'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(fast.0, "[\"operator-model\"]");
    assert!(reasoning.0.contains("grok-4.20-reasoning"));
}

#[tokio::test]
async fn test_public_model_list_includes_intelligence_metadata() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();

    let state = AppState::new(
        Config::default(),
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );
    seed_policy_provider_model(&state, "google", "gemini-2.0-flash", true, 100, true, true).await;

    let response = tokenscavenger::discovery::merge::build_model_list(&state).await;
    let gemini = response
        .data
        .iter()
        .find(|model| {
            model.provider_id.as_deref() == Some("google") && model.id == "gemini-2.0-flash"
        })
        .expect("gemini model present");

    assert!(gemini.context_window.is_some());
    assert!(
        gemini
            .modalities
            .as_ref()
            .is_some_and(|modalities| modalities.contains(&"vision".to_string()))
    );
    assert!(gemini.freshness.is_some());
}

#[tokio::test]
async fn test_admin_config_save_updates_model_intelligence_overrides() {
    let (app, state) = build_test_app().await;
    seed_policy_provider_model(&state, "mock", "plain", true, 100, true, true).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/admin/config")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "models": [{
                            "provider_id": "mock",
                            "model_id": "plain",
                            "supports_vision": true,
                            "metadata": {
                                "context_window": 64000,
                                "task_tags": ["chat", "vision"]
                            }
                        }]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let row = sqlx::query_as::<_, (bool, String)>(
        "SELECT supports_vision, metadata_json FROM models WHERE provider_id = 'mock' AND upstream_model_id = 'plain'",
    )
    .fetch_one(&state.db)
    .await
    .unwrap();
    assert!(row.0);
    assert!(row.1.contains("64000"));
}

async fn seed_policy_provider_model(
    state: &AppState,
    provider_id: &str,
    model_id: &str,
    free_tier: bool,
    priority: i64,
    supports_tools: bool,
    supports_json_mode: bool,
) {
    sqlx::query(
        "INSERT OR REPLACE INTO providers (provider_id, display_name, enabled, free_only)
         VALUES (?, ?, 1, ?)",
    )
    .bind(provider_id)
    .bind(provider_id)
    .bind(free_tier)
    .execute(&state.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT OR REPLACE INTO models
         (provider_id, upstream_model_id, public_model_id, enabled, free_tier, supports_tools, supports_json_mode, priority, metadata_json)
         VALUES (?, ?, ?, 1, ?, ?, ?, ?, ?)",
    )
    .bind(provider_id)
    .bind(model_id)
    .bind(model_id)
    .bind(free_tier)
    .bind(supports_tools)
    .bind(supports_json_mode)
    .bind(priority)
    .bind(serde_json::json!({"context_window": 8192}).to_string())
    .execute(&state.db)
    .await
    .unwrap();
}

async fn seed_policy_rate(
    state: &AppState,
    provider_id: &str,
    model_id: &str,
    input_per_1m: f64,
    output_per_1m: f64,
) {
    sqlx::query(
        "INSERT INTO model_pricing
         (provider_id, model_id, input_per_1m, output_per_1m, source_kind, confidence)
         VALUES (?, ?, ?, ?, 'operator_override', 'provider_published')",
    )
    .bind(provider_id)
    .bind(model_id)
    .bind(input_per_1m)
    .bind(output_per_1m)
    .execute(&state.db)
    .await
    .unwrap();
}

async fn seed_request_log(
    state: &AppState,
    request_id: &str,
    provider_id: &str,
    model_id: &str,
    status: &str,
    latency_ms: i64,
) {
    sqlx::query(
        "INSERT INTO request_log
         (request_id, endpoint_kind, requested_model, selected_provider_id, selected_model_id, status, http_status, latency_ms)
         VALUES (?, 'chat', ?, ?, ?, ?, 200, ?)",
    )
    .bind(request_id)
    .bind(model_id)
    .bind(provider_id)
    .bind(model_id)
    .bind(status)
    .bind(latency_ms)
    .execute(&state.db)
    .await
    .unwrap();
}

async fn seed_success_usage(
    state: &AppState,
    request_id: &str,
    requested_model: &str,
    provider_id: &str,
    model_id: &str,
    cost: f64,
) {
    sqlx::query(
        "INSERT INTO request_log
         (request_id, endpoint_kind, requested_model, selected_provider_id, selected_model_id, status, http_status, latency_ms)
         VALUES (?, 'chat', ?, ?, ?, 'success', 200, 20)",
    )
    .bind(request_id)
    .bind(requested_model)
    .bind(provider_id)
    .bind(model_id)
    .execute(&state.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO usage_events
         (request_id, provider_id, model_id, input_tokens, output_tokens, estimated_cost_usd, cost_confidence, free_tier)
         VALUES (?, ?, ?, 1, 1, ?, 'provider_published', 0)",
    )
    .bind(request_id)
    .bind(provider_id)
    .bind(model_id)
    .bind(cost)
    .execute(&state.db)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_paid_providers_require_paid_fallback_policy() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let mut config = Config::default();
    config.routing.provider_order = vec!["groq".into(), "deepseek".into(), "xai".into()];
    config.routing.allow_paid_fallback = false;
    config.providers = vec![
        tokenscavenger::config::schema::ProviderConfig {
            id: "groq".into(),
            enabled: true,
            free_only: true,
            ..Default::default()
        },
        tokenscavenger::config::schema::ProviderConfig {
            id: "deepseek".into(),
            enabled: true,
            free_only: false,
            ..Default::default()
        },
        tokenscavenger::config::schema::ProviderConfig {
            id: "xai".into(),
            enabled: true,
            free_only: false,
            ..Default::default()
        },
    ];

    let state = AppState::new(
        config,
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );
    state.provider_registry.init_from_config(&state).await;

    let policy = tokenscavenger::router::policy::RoutePolicy::from_config(&state.config());
    let plan = tokenscavenger::router::selection::build_attempt_plan(
        &policy,
        &state.provider_registry,
        "shared-model",
        tokenscavenger::providers::traits::EndpointKind::ChatCompletions,
    )
    .await;
    let filtered = tokenscavenger::router::selection::filter_by_paid_policy(plan, &state);

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].provider_id, "groq");

    let mut updated = (*state.config()).clone();
    updated.routing.allow_paid_fallback = true;
    state.runtime_config.store(std::sync::Arc::new(updated));
    let policy = tokenscavenger::router::policy::RoutePolicy::from_config(&state.config());
    let plan = tokenscavenger::router::selection::build_attempt_plan(
        &policy,
        &state.provider_registry,
        "shared-model",
        tokenscavenger::providers::traits::EndpointKind::ChatCompletions,
    )
    .await;
    let filtered = tokenscavenger::router::selection::filter_by_paid_policy(plan, &state);

    assert_eq!(filtered.len(), 3);
    assert_eq!(filtered[1].provider_id, "deepseek");
    assert_eq!(filtered[2].provider_id, "xai");
}
