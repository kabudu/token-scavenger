use crate::config::schema::Config;
use crate::providers::registry::ProviderRegistry;
use crate::resilience::breaker::CircuitBreakerState;
use crate::resilience::health::ProviderHealthState;
use crate::router::engine::RouteEngine;
use arc_swap::ArcSwap;
use axum::extract::FromRef;
use dashmap::DashMap;
use moka::future::Cache;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::{broadcast, watch};

/// Shared application state, accessible from all route handlers and background tasks.
#[derive(Clone)]
pub struct AppState {
    /// Immutable boot configuration.
    pub boot_config: Arc<Config>,

    /// Hot-reloadable effective runtime config.
    pub runtime_config: Arc<ArcSwap<Config>>,

    /// SQLite connection pool.
    pub db: SqlitePool,

    /// Shared HTTP client for outbound requests.
    pub http_client: reqwest::Client,

    /// Provider adapter registry.
    pub provider_registry: Arc<ProviderRegistry>,

    /// Route planning engine.
    pub route_engine: Arc<RouteEngine>,

    /// Per-provider model catalog cache (provider_id -> cached models JSON).
    pub model_cache: Arc<Cache<String, String>>,

    /// Per-provider health state.
    pub health_states: Arc<DashMap<String, ProviderHealthState>>,

    /// Per-provider circuit breaker state.
    pub breaker_states: Arc<DashMap<String, CircuitBreakerState>>,

    /// Broadcast channel for live log events (UI streaming).
    pub log_tx: broadcast::Sender<String>,

    /// Broadcast channel for health events (UI streaming).
    pub health_event_tx: broadcast::Sender<String>,

    /// Watch channel for config snapshot updates.
    pub config_watch_tx: watch::Sender<Arc<Config>>,
    pub config_watch_rx: watch::Receiver<Arc<Config>>,

    /// Shutdown signal.
    pub shutdown_tx: tokio::sync::watch::Sender<bool>,
    pub shutdown_rx: tokio::sync::watch::Receiver<bool>,

    /// Start time for uptime tracking.
    pub start_time: std::time::Instant,
}

impl AppState {
    /// Create a new AppState with the given configuration and database pool.
    pub fn new(config: Config, db: SqlitePool) -> Self {
        let config = Arc::new(config);
        let (config_watch_tx, config_watch_rx) = watch::channel(Arc::clone(&config));
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let (log_tx, _log_rx) = broadcast::channel(1024);
        let (health_event_tx, _health_event_rx) = broadcast::channel(256);

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(
                config.server.request_timeout_ms,
            ))
            .build()
            .expect("Failed to build HTTP client");

        let provider_registry = Arc::new(ProviderRegistry::new());
        let route_engine = Arc::new(RouteEngine::new(
            Arc::clone(&provider_registry),
            Arc::clone(&config),
        ));

        Self {
            boot_config: Arc::clone(&config),
            runtime_config: Arc::new(ArcSwap::new(Arc::clone(&config))),
            db,
            http_client,
            provider_registry,
            route_engine,
            model_cache: Arc::new(Cache::new(10_000)),
            health_states: Arc::new(DashMap::new()),
            breaker_states: Arc::new(DashMap::new()),
            log_tx,
            health_event_tx,
            config_watch_tx,
            config_watch_rx,
            shutdown_tx,
            shutdown_rx,
            start_time: std::time::Instant::now(),
        }
    }

    /// Return the current effective config.
    pub fn config(&self) -> Arc<Config> {
        self.runtime_config.load_full()
    }
}

impl FromRef<AppState> for SqlitePool {
    fn from_ref(state: &AppState) -> Self {
        state.db.clone()
    }
}
