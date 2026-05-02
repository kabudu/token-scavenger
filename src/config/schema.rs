use serde::{Deserialize, Serialize};

/// Top-level application configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// Configuration version for schema migration compatibility.
    #[serde(default = "default_config_version")]
    pub version: String,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub metrics: MetricsConfig,
    #[serde(default)]
    pub routing: RoutingConfig,
    #[serde(default)]
    pub resilience: ResilienceConfig,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
}

/// HTTP server binding and auth settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default)]
    pub master_api_key: String,
    #[serde(default)]
    pub allowed_cors_origins: Vec<String>,
    #[serde(default)]
    pub allow_query_api_keys: bool,
    #[serde(default)]
    pub ui_session_auth: bool,
    #[serde(default = "default_true")]
    pub ui_enabled: bool,
    #[serde(default = "default_ui_path")]
    pub ui_path: String,
    #[serde(default = "default_request_timeout_ms")]
    pub request_timeout_ms: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            master_api_key: String::new(),
            allowed_cors_origins: Vec::new(),
            allow_query_api_keys: false,
            ui_session_auth: false,
            ui_enabled: true,
            ui_path: default_ui_path(),
            request_timeout_ms: default_request_timeout_ms(),
        }
    }
}

/// SQLite database configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    #[serde(default = "default_db_path")]
    pub path: String,
    #[serde(default = "default_db_max_connections")]
    pub max_connections: u32,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: default_db_path(),
            max_connections: default_db_max_connections(),
        }
    }
}

/// Structured logging configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_format")]
    pub format: String,
    #[serde(default = "default_log_level")]
    pub level: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            format: default_log_format(),
            level: default_log_level(),
        }
    }
}

/// Prometheus metrics configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_metrics_path")]
    pub path: String,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            path: default_metrics_path(),
        }
    }
}

/// Routing policy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingConfig {
    #[serde(default = "default_true")]
    pub free_first: bool,
    #[serde(default)]
    pub allow_paid_fallback: bool,
    #[serde(default = "default_alias_strategy")]
    pub default_alias_strategy: String,
    #[serde(default)]
    pub provider_order: Vec<String>,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            free_first: true,
            allow_paid_fallback: false,
            default_alias_strategy: default_alias_strategy(),
            provider_order: Vec::new(),
        }
    }
}

/// Resilience and circuit-breaker settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResilienceConfig {
    #[serde(default = "default_max_retries")]
    pub max_retries_per_provider: u32,
    #[serde(default = "default_breaker_threshold")]
    pub breaker_failure_threshold: u32,
    #[serde(default = "default_breaker_cooldown_secs")]
    pub breaker_cooldown_secs: u64,
    #[serde(default = "default_health_probe_interval")]
    pub health_probe_interval_secs: u64,
}

impl Default for ResilienceConfig {
    fn default() -> Self {
        Self {
            max_retries_per_provider: default_max_retries(),
            breaker_failure_threshold: default_breaker_threshold(),
            breaker_cooldown_secs: default_breaker_cooldown_secs(),
            health_probe_interval_secs: default_health_probe_interval(),
        }
    }
}

/// A single upstream provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfig {
    pub id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    #[serde(default = "default_true")]
    pub free_only: bool,
    #[serde(default = "default_true")]
    pub discover_models: bool,
}

// Default values
fn default_config_version() -> String {
    "0.1.0".to_string()
}
fn default_bind() -> String {
    "0.0.0.0:8000".to_string()
}
fn default_true() -> bool {
    true
}
fn default_ui_path() -> String {
    "/ui".to_string()
}
fn default_request_timeout_ms() -> u64 {
    120_000
}
fn default_db_path() -> String {
    "tokenscavenger.db".to_string()
}
fn default_db_max_connections() -> u32 {
    8
}
fn default_log_format() -> String {
    "json".to_string()
}
fn default_log_level() -> String {
    "info".to_string()
}
fn default_metrics_path() -> String {
    "/metrics".to_string()
}
fn default_alias_strategy() -> String {
    "provider-priority".to_string()
}
fn default_max_retries() -> u32 {
    2
}
fn default_breaker_threshold() -> u32 {
    3
}
fn default_breaker_cooldown_secs() -> u64 {
    60
}
fn default_health_probe_interval() -> u64 {
    30
}
