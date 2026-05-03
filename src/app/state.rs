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
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tokio::sync::{broadcast, watch};

/// Shared application state, accessible from all route handlers and background tasks.
#[derive(Clone)]
pub struct AppState {
    /// Immutable boot configuration.
    pub boot_config: Arc<Config>,

    /// Hot-reloadable effective runtime config.
    pub runtime_config: Arc<ArcSwap<Config>>,

    /// Path to the boot configuration file (for saving overrides).
    pub boot_config_file: PathBuf,

    /// SQLite connection pool.
    pub db: SqlitePool,

    /// Shared HTTP client for outbound requests.
    pub http_client: reqwest::Client,

    /// Provider adapter registry.
    pub provider_registry: Arc<ProviderRegistry>,

    /// Route planning engine (wrapped in RwLock for hot-reload).
    pub route_engine: Arc<RwLock<RouteEngine>>,

    /// Per-provider model catalog cache (provider_id -> cached models JSON).
    pub model_cache: Arc<Cache<String, String>>,

    /// Per-provider health state.
    pub health_states: Arc<DashMap<String, ProviderHealthState>>,

    /// Per-provider circuit breaker state.
    pub breaker_states: Arc<DashMap<String, CircuitBreakerState>>,

    /// In-memory UI browser sessions for optional cookie auth.
    pub ui_sessions: Arc<DashMap<String, i64>>,

    /// Broadcast channel for live log events (UI streaming).
    /// Wrapped in Option so shutdown can take-and-drop the sender to close
    /// the channel, which breaks the SSE circular dependency during drain.
    pub log_tx: Arc<std::sync::Mutex<Option<broadcast::Sender<String>>>>,

    /// Broadcast channel for health events (UI streaming).
    pub health_event_tx: broadcast::Sender<String>,

    /// Watch channel for config snapshot updates.
    pub config_watch_tx: watch::Sender<Arc<Config>>,
    pub config_watch_rx: watch::Receiver<Arc<Config>>,

    /// Shutdown signal.
    pub shutdown_tx: tokio::sync::watch::Sender<bool>,
    pub shutdown_rx: tokio::sync::watch::Receiver<bool>,

    /// JoinHandles for background tasks, drained and awaited during graceful shutdown.
    pub background_handles: Arc<std::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>>,

    /// Start time for uptime tracking.
    pub start_time: std::time::Instant,
}

impl AppState {
    /// Create a new AppState with the given configuration and database pool.
    /// `log_tx` should be created before tracing is initialized so the broadcast
    /// layer can forward events into the same channel.
    pub fn new(
        config: Config,
        db: SqlitePool,
        config_path: PathBuf,
        log_tx: broadcast::Sender<String>,
    ) -> Self {
        let config = Arc::new(config);
        let (config_watch_tx, config_watch_rx) = watch::channel(Arc::clone(&config));
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let (health_event_tx, _health_event_rx) = broadcast::channel(256);

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(
                config.server.request_timeout_ms,
            ))
            .build()
            .expect("Failed to build HTTP client");

        let provider_registry = Arc::new(ProviderRegistry::new());
        let route_engine = Arc::new(RwLock::new(RouteEngine::new(
            Arc::clone(&provider_registry),
            Arc::clone(&config),
        )));

        Self {
            boot_config: Arc::clone(&config),
            runtime_config: Arc::new(ArcSwap::new(Arc::clone(&config))),
            boot_config_file: config_path,
            db,
            http_client,
            provider_registry,
            route_engine,
            model_cache: Arc::new(Cache::new(10_000)),
            health_states: Arc::new(DashMap::new()),
            breaker_states: Arc::new(DashMap::new()),
            ui_sessions: Arc::new(DashMap::new()),
            log_tx: Arc::new(std::sync::Mutex::new(Some(log_tx))),
            health_event_tx,
            config_watch_tx,
            config_watch_rx,
            shutdown_tx,
            shutdown_rx,
            background_handles: Arc::new(std::sync::Mutex::new(Vec::new())),
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
