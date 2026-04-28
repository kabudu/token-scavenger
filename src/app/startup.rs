use std::path::Path;
use std::sync::Arc;
use axum::Router;
use sqlx::SqlitePool;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;
use crate::app::state::AppState;
use crate::config::schema::Config;
use crate::config::loader::load_config;
use crate::db::models as db_models;

/// Application startup result.
pub struct StartupResult {
    pub state: AppState,
    pub router: Router,
    pub listener: TcpListener,
}

/// Parse CLI args, load config, initialize the database, build AppState, create the router,
/// and bind the HTTP listener. Returns a fully prepared `StartupResult`.
pub async fn startup(config_path: &Path) -> Result<StartupResult, Box<dyn std::error::Error>> {
    // 1. Load and validate config
    let config: Config = load_config(config_path)?;
    info!(
        "Config loaded from {}: server.bind={}, providers={}",
        config_path.display(),
        config.server.bind,
        config.providers.len()
    );

    // 2. Initialize tracing
    init_tracing(&config);
    info!("Tracing initialized");

    // 3. Open SQLite and run migrations
    let db: SqlitePool = db_models::init_db(&config.database.path).await?;
    info!("Database initialized at {}", config.database.path);

    // 4. Build AppState
    let state = AppState::new(config, db);
    info!("AppState created");

    // 5. Initialize the provider registry from config
    state.provider_registry.init_from_config(&state).await;
    info!("Provider registry initialized");

    // 6. Build the Axum router
    let router = build_router(state.clone());

    // 7. Bind the HTTP listener
    let addr = state.config().server.bind.clone();
    let listener = TcpListener::bind(&addr).await?;
    info!("HTTP listener bound to {}", addr);

    Ok(StartupResult { state, router, listener })
}

/// Build the full Axum router with all routes and middleware.
fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::permissive();

    Router::new()
        // Health and readiness endpoints
        .route("/healthz", axum::routing::get(crate::api::routes::healthz))
        .route("/readyz", axum::routing::get(crate::api::routes::readyz))
        .route("/metrics", axum::routing::get(crate::api::routes::metrics))
        // OpenAI-compatible API routes
        .route("/v1/chat/completions", axum::routing::post(crate::api::routes::chat_completions))
        .route("/v1/embeddings", axum::routing::post(crate::api::routes::embeddings))
        .route("/v1/models", axum::routing::get(crate::api::routes::models))
        // UI
        .route("/ui", axum::routing::get(crate::api::routes::ui_index))
        .route("/ui/*path", axum::routing::get(crate::api::routes::ui_static))
        // Admin routes
        .route("/admin/providers", axum::routing::get(crate::api::routes::admin_providers))
        .route("/admin/config", axum::routing::get(crate::api::routes::admin_config))
        .route("/admin/models", axum::routing::get(crate::api::routes::admin_models))
        .route("/admin/usage/series", axum::routing::get(crate::api::routes::admin_usage_series))
        .route("/admin/logs/stream", axum::routing::get(crate::api::routes::admin_logs_stream))
        .route("/admin/health/events", axum::routing::get(crate::api::routes::admin_health_events))
        .route("/admin/audit", axum::routing::get(crate::api::routes::admin_audit))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}

/// Initialize the tracing subscriber based on config.
fn init_tracing(config: &Config) {
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};

    let fmt_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(false);

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.logging.level));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .init();
}
