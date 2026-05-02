use crate::app::state::AppState;
use crate::config::loader::load_config;
use crate::config::schema::Config;
use crate::db::models as db_models;
use axum::Router;
use axum::http::{HeaderName, HeaderValue, Method, header};
use axum::middleware::from_fn_with_state;
use sqlx::SqlitePool;
use std::path::Path;
use tokio::net::TcpListener;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

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
    let db: SqlitePool =
        db_models::init_db_with_pool_size(&config.database.path, config.database.max_connections)
            .await?;
    info!("Database initialized at {}", config.database.path);

    // 4. Build AppState
    let state = AppState::new(config, db);
    info!("AppState created");

    // 5. Load DB-persisted config overrides before building runtime registries.
    load_db_config_overrides(&state).await;

    // 6. Initialize the provider registry from effective config
    state.provider_registry.init_from_config(&state).await;
    info!("Provider registry initialized");

    // 7. Build the Axum router
    let router = build_router(state.clone());

    // 8. Bind the HTTP listener
    let addr = state.config().server.bind.clone();
    let listener = TcpListener::bind(&addr).await?;
    info!("HTTP listener bound to {}", addr);

    // 9. Start background tasks (these run in spawned tasks, so .await is not needed)
    spawn_background_tasks(state.clone());

    // 10. Perform initial provider discovery
    let state_for_disc = state.clone();
    tokio::spawn(async move {
        info!("Starting initial provider discovery...");
        crate::discovery::refresh::refresh_all(&state_for_disc).await;
        info!("Initial provider discovery complete");
    });

    Ok(StartupResult {
        state,
        router,
        listener,
    })
}

/// Load DB-persisted mutable operator state and merge with file config.
async fn load_db_config_overrides(state: &AppState) {
    let db = &state.db;

    // Load provider states from DB
    if let Ok(rows) = sqlx::query_as::<_, (String, bool, i32)>(
        "SELECT provider_id, enabled, priority FROM providers",
    )
    .fetch_all(db)
    .await
    {
        let mut config = (*state.runtime_config.load_full()).clone();
        let row_count = rows.len();
        for (pid, enabled, priority) in rows {
            if let Some(provider) = config.providers.iter_mut().find(|p| p.id == pid) {
                provider.enabled = enabled;
            }
            let _ = priority;
        }
        let config = std::sync::Arc::new(config);
        state.runtime_config.store(config.clone());
        let _ = state.config_watch_tx.send(config);
        info!("Loaded {} provider overrides from DB", row_count);
    }

    // Load model states from DB
    if let Ok(rows) = sqlx::query_as::<_, (String, String, bool)>(
        "SELECT provider_id, upstream_model_id, enabled FROM models",
    )
    .fetch_all(db)
    .await
    {
        let row_count = rows.len();
        info!("Loaded {} model records from DB", row_count);
    }

    info!("DB config overrides applied");
}

/// Spawn background task loops.
fn spawn_background_tasks(base_state: AppState) {
    // Discovery refresh loop
    let s = base_state.clone();
    tokio::spawn(async move {
        let interval = s.config().resilience.health_probe_interval_secs * 6;
        loop {
            let mut shutdown_rx = s.shutdown_rx.clone();
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(interval)) => {
                    crate::discovery::refresh::refresh_all(&s).await;
                }
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() { break; }
                }
            }
        }
        info!("Discovery refresh loop stopped");
    });

    // Health probe loop
    let s = base_state.clone();
    tokio::spawn(async move {
        let interval = s.config().resilience.health_probe_interval_secs;
        loop {
            let mut shutdown_rx = s.shutdown_rx.clone();
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(interval)) => {
                    for provider_id in s.config().providers.iter().filter(|p| p.enabled).map(|p| p.id.clone()) {
                        crate::resilience::health::record_success(&s, &provider_id).await;
                    }
                }
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() { break; }
                }
            }
        }
        info!("Health probe loop stopped");
    });

    // Circuit breaker decay/reset loop
    let s = base_state.clone();
    let cooldown = base_state.config().resilience.breaker_cooldown_secs;
    tokio::spawn(async move {
        loop {
            let mut shutdown_rx = s.shutdown_rx.clone();
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(cooldown)) => {
                    for entry in s.breaker_states.iter() {
                        if entry.value().is_open() {}
                    }
                }
                _ = shutdown_rx.changed() => { if *shutdown_rx.borrow() { break; } }
            }
        }
        info!("Breaker decay loop stopped");
    });

    // Usage aggregation flush loop
    let s = base_state.clone();
    tokio::spawn(async move {
        loop {
            let mut shutdown_rx = s.shutdown_rx.clone();
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => {
                    info!("Usage flush: health states up to date");
                }
                _ = shutdown_rx.changed() => { if *shutdown_rx.borrow() { break; } }
            }
        }
        info!("Usage flush loop stopped");
    });

    // Retention cleanup loop
    let s = base_state.clone();
    tokio::spawn(async move {
        loop {
            let mut shutdown_rx = s.shutdown_rx.clone();
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(3600)) => {
                    let _ = sqlx::query("DELETE FROM usage_events WHERE timestamp < datetime('now', '-30 days')").execute(&s.db).await;
                    let _ = sqlx::query("DELETE FROM provider_health_events WHERE recorded_at < datetime('now', '-30 days')").execute(&s.db).await;
                    let _ = sqlx::query("DELETE FROM config_audit_log WHERE created_at < datetime('now', '-90 days')").execute(&s.db).await;
                    info!("Retention cleanup complete");
                }
                _ = shutdown_rx.changed() => { if *shutdown_rx.borrow() { break; } }
            }
        }
        info!("Retention cleanup loop stopped");
    });

    info!("All background tasks started");
}

/// Build the full Axum router with all routes and middleware.
pub fn build_router(state: AppState) -> Router {
    let public = Router::new()
        .route("/healthz", axum::routing::get(crate::api::routes::healthz))
        .route("/readyz", axum::routing::get(crate::api::routes::readyz));

    let protected = Router::new()
        .route("/metrics", axum::routing::get(crate::api::routes::metrics))
        .route(
            "/v1/chat/completions",
            axum::routing::post(crate::api::routes::chat_completions),
        )
        .route(
            "/v1/embeddings",
            axum::routing::post(crate::api::routes::embeddings),
        )
        .route("/v1/models", axum::routing::get(crate::api::routes::models))
        .route("/ui", axum::routing::get(crate::api::routes::ui_index))
        .route(
            "/ui/{*path}",
            axum::routing::get(crate::api::routes::ui_static),
        )
        .route(
            "/admin/providers",
            axum::routing::get(crate::api::routes::admin_providers),
        )
        .route(
            "/admin/config",
            axum::routing::get(crate::api::routes::admin_config),
        )
        .route(
            "/admin/models",
            axum::routing::get(crate::api::routes::admin_models),
        )
        .route(
            "/admin/usage/series",
            axum::routing::get(crate::api::routes::admin_usage_series),
        )
        .route(
            "/admin/logs/stream",
            axum::routing::get(crate::api::routes::admin_logs_stream),
        )
        .route(
            "/admin/health/events",
            axum::routing::get(crate::api::routes::admin_health_events),
        )
        .route(
            "/admin/audit",
            axum::routing::get(crate::api::routes::admin_audit),
        )
        // Admin POST routes (mutations)
        .route(
            "/admin/providers/discovery/refresh",
            axum::routing::post(crate::api::routes::admin_discovery_refresh),
        )
        .route(
            "/admin/providers/{id}/test",
            axum::routing::post(crate::api::routes::admin_provider_test),
        )
        .route(
            "/admin/config",
            axum::routing::put(crate::api::routes::admin_config_save),
        )
        .layer(from_fn_with_state(
            state.clone(),
            crate::api::auth::auth_middleware,
        ));

    public
        .merge(protected)
        .layer(TraceLayer::new_for_http())
        .layer(axum::middleware::from_fn(
            crate::api::middleware::request_id_middleware,
        ))
        .layer(cors_layer(&state.config()))
        .with_state(state)
}

fn cors_layer(config: &Config) -> CorsLayer {
    let mut layer = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::PUT])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            HeaderName::from_static("x-request-id"),
        ]);

    let origins: Vec<HeaderValue> = config
        .server
        .allowed_cors_origins
        .iter()
        .filter_map(|origin| origin.parse::<HeaderValue>().ok())
        .collect();

    if !origins.is_empty() {
        layer = layer.allow_origin(AllowOrigin::list(origins));
    }

    layer
}

/// Initialize the tracing subscriber based on config.
fn init_tracing(config: &Config) {
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{EnvFilter, fmt};

    let fmt_layer = fmt::layer().with_target(true).with_thread_ids(false);

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.logging.level));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .init();
}
