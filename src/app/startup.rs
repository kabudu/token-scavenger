use crate::app::state::AppState;
use crate::config::loader::load_config;
use crate::config::overrides;
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
    let mut config: Config = load_config(config_path)?;

    // 1b. Merge any runtime overrides saved from a previous session
    if let Some(overrides) = overrides::load_runtime_overrides(config_path) {
        info!("Merging runtime overrides from previous session");
        // Merge providers from overrides (file config is the base, overrides add/update)
        for ov_provider in &overrides.providers {
            if let Some(existing) = config.providers.iter_mut().find(|p| p.id == ov_provider.id) {
                *existing = ov_provider.clone();
            } else {
                config.providers.push(ov_provider.clone());
            }
        }
        config.server = overrides.server;
        config.database = overrides.database;
        config.metrics = overrides.metrics;
        config.routing = overrides.routing;
        config.resilience = overrides.resilience;
        info!("Runtime overrides merged successfully");
    }

    info!(
        "Config loaded from {}: server.bind={}, providers={}",
        config_path.display(),
        config.server.bind,
        config.providers.len()
    );

    // 2. Create log broadcast channel (must come before tracing init so the layer can hold the sender)
    let (log_tx, _log_rx) = tokio::sync::broadcast::channel::<String>(1024);

    // 3. Initialize tracing (installs BroadcastLayer so UI stream receives log events)
    init_tracing(&config, log_tx.clone());
    info!("Tracing initialized");

    // 4. Open SQLite and run migrations
    let db: SqlitePool =
        db_models::init_db_with_pool_size(&config.database.path, config.database.max_connections)
            .await?;
    info!("Database initialized at {}", config.database.path);

    // 5. Build AppState (hand over the pre-created log_tx)
    let state = AppState::new(config, db, config_path.to_path_buf(), log_tx);
    info!("AppState created");

    // 5. Load DB-persisted config overrides before building runtime registries.
    load_db_config_overrides(&state).await;

    // 5b. Seed curated model catalog so routing can find baseline models
    //     before the first discovery cycle completes.
    crate::discovery::curated::seed_curated_models(&state.db).await;
    info!("Curated models seeded");

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
    let disc_handle = tokio::spawn(async move {
        let mut shutdown_rx = state_for_disc.shutdown_rx.clone();
        tokio::select! {
            _ = crate::discovery::refresh::refresh_all(&state_for_disc) => {
                info!("Initial provider discovery complete");
            }
            _ = shutdown_rx.changed() => {
                info!("Initial provider discovery cancelled by shutdown");
            }
        }
    });
    state.background_handles.lock().unwrap().push(disc_handle);

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
    let handle = tokio::spawn(async move {
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
    base_state.background_handles.lock().unwrap().push(handle);

    // Health probe loop
    let s = base_state.clone();
    let handle = tokio::spawn(async move {
        let interval = s.config().resilience.health_probe_interval_secs;
        loop {
            let mut shutdown_rx = s.shutdown_rx.clone();
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(interval)) => {
                    for provider_id in s.config().providers.iter().filter(|p| p.enabled).map(|p| p.id.clone()) {
                        crate::resilience::health::probe_provider(&s, &provider_id).await;
                    }
                }
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() { break; }
                }
            }
        }
        info!("Health probe loop stopped");
    });
    base_state.background_handles.lock().unwrap().push(handle);

    // Circuit breaker decay/reset loop
    let s = base_state.clone();
    let cooldown = base_state.config().resilience.breaker_cooldown_secs;
    let handle = tokio::spawn(async move {
        loop {
            let mut shutdown_rx = s.shutdown_rx.clone();
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(cooldown)) => {
                    crate::resilience::health::recover_open_breakers(&s).await;
                }
                _ = shutdown_rx.changed() => { if *shutdown_rx.borrow() { break; } }
            }
        }
        info!("Breaker decay loop stopped");
    });
    base_state.background_handles.lock().unwrap().push(handle);

    // Usage aggregation flush loop
    let s = base_state.clone();
    let handle = tokio::spawn(async move {
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
    base_state.background_handles.lock().unwrap().push(handle);

    // Pricing catalog refresh loop. Runs in the background so startup/readiness
    // are not blocked by provider pricing pages.
    let s = base_state.clone();
    let handle = tokio::spawn(async move {
        match crate::usage::pricing_catalog::refresh_pricing_sources(&s.db, &s.http_client, false)
            .await
        {
            Ok(_) => info!("Startup pricing catalog refresh complete"),
            Err(error) => tracing::warn!(%error, "Startup pricing catalog refresh failed"),
        }
        loop {
            let mut shutdown_rx = s.shutdown_rx.clone();
            let jitter_secs = rand::random_range(0..900);
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(86_400 + jitter_secs)) => {
                    match crate::usage::pricing_catalog::refresh_pricing_sources(&s.db, &s.http_client, false).await {
                        Ok(_) => info!("Pricing catalog refresh complete"),
                        Err(error) => tracing::warn!(%error, "Pricing catalog refresh failed"),
                    }
                }
                _ = shutdown_rx.changed() => { if *shutdown_rx.borrow() { break; } }
            }
        }
        info!("Pricing catalog refresh loop stopped");
    });
    base_state.background_handles.lock().unwrap().push(handle);

    // Retention cleanup loop
    let s = base_state.clone();
    let handle = tokio::spawn(async move {
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
    base_state.background_handles.lock().unwrap().push(handle);

    info!("All background tasks started");
}

/// Build the full Axum router with all routes and middleware.
pub fn build_router(state: AppState) -> Router {
    let public = Router::new()
        .route("/healthz", axum::routing::get(crate::api::routes::healthz))
        .route("/readyz", axum::routing::get(crate::api::routes::readyz))
        .route(
            "/ui/logo.png",
            axum::routing::get(crate::api::routes::ui_logo),
        )
        .route(
            "/favicon.ico",
            axum::routing::get(crate::api::routes::favicon),
        )
        .route(
            "/admin/session",
            axum::routing::post(crate::api::routes::admin_session),
        )
        .route(
            "/ui/login",
            axum::routing::get(crate::api::routes::ui_login),
        );

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
            "/admin/route-plan",
            axum::routing::get(crate::api::routes::admin_route_plan),
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
        .route(
            "/admin/config/rollback",
            axum::routing::post(crate::api::routes::admin_config_rollback),
        )
        .route(
            "/admin/model-groups",
            axum::routing::get(crate::api::routes::admin_model_groups_list),
        )
        .route(
            "/admin/model-groups/{name}",
            axum::routing::delete(crate::api::routes::admin_delete_model_group),
        )
        .route(
            "/admin/analytics/traffic",
            axum::routing::get(crate::api::routes::admin_analytics_traffic),
        )
        .route(
            "/admin/analytics/distribution",
            axum::routing::get(crate::api::routes::admin_analytics_distribution),
        )
        .route(
            "/admin/analytics/summary",
            axum::routing::get(crate::api::routes::admin_analytics_summary),
        )
        .route(
            "/admin/analytics/metrics",
            axum::routing::get(crate::api::routes::admin_analytics_metrics),
        )
        .route(
            "/admin/pricing",
            axum::routing::get(crate::api::routes::admin_pricing),
        )
        .route(
            "/admin/pricing/refresh",
            axum::routing::post(crate::api::routes::admin_pricing_refresh),
        )
        .route(
            "/admin/pricing/backfill",
            axum::routing::post(crate::api::routes::admin_pricing_backfill),
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
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
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
/// Installs a `BroadcastLayer` that forwards every log event into `log_tx`
/// so the browser UI SSE stream receives live log output.
fn init_tracing(config: &Config, log_tx: tokio::sync::broadcast::Sender<String>) {
    use crate::util::broadcast_layer::BroadcastLayer;
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{EnvFilter, fmt};

    let fmt_layer = fmt::layer().with_target(true).with_thread_ids(false);

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.logging.level));

    let broadcast = BroadcastLayer::new(log_tx);

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .with(broadcast)
        .init();
}
