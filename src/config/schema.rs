use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    pub security: SecurityConfig,
    #[serde(default)]
    pub retention: RetentionConfig,
    #[serde(default)]
    pub updates: UpdateConfig,
    #[serde(default)]
    pub routing: RoutingConfig,
    #[serde(default)]
    pub resilience: ResilienceConfig,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecurityConfig {
    #[serde(default)]
    pub credential_encryption: CredentialEncryptionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialEncryptionConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_credential_key_env")]
    pub key_env: String,
}

impl Default for CredentialEncryptionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            key_env: default_credential_key_env(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionConfig {
    #[serde(default = "default_usage_retention_days")]
    pub usage_days: u32,
    #[serde(default = "default_health_retention_days")]
    pub health_event_days: u32,
    #[serde(default = "default_audit_retention_days")]
    pub audit_days: u32,
    #[serde(default = "default_trace_retention_days")]
    pub request_trace_days: u32,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            usage_days: default_usage_retention_days(),
            health_event_days: default_health_retention_days(),
            audit_days: default_audit_retention_days(),
            request_trace_days: default_trace_retention_days(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_update_repo")]
    pub github_repo: String,
    #[serde(default = "default_update_check_interval_secs")]
    pub check_interval_secs: u64,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            github_repo: default_update_repo(),
            check_interval_secs: default_update_check_interval_secs(),
        }
    }
}

/// HTTP server binding and auth settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default)]
    pub master_api_key: String,
    #[serde(default)]
    pub external_identity: ExternalIdentityConfig,
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
            external_identity: ExternalIdentityConfig::default(),
            allowed_cors_origins: Vec::new(),
            allow_query_api_keys: false,
            ui_session_auth: false,
            ui_enabled: true,
            ui_path: default_ui_path(),
            request_timeout_ms: default_request_timeout_ms(),
        }
    }
}

/// Trusted reverse-proxy identity headers for admin UI/API access.
///
/// TokenScavenger does not perform the OAuth/OIDC browser dance itself. Instead,
/// an operator can place it behind an identity-aware proxy such as oauth2-proxy,
/// Dex, Authelia, Keycloak, Zitadel, or a cloud load balancer that authenticates
/// Google, GitHub, Microsoft, or another OIDC provider and forwards identity
/// headers to TokenScavenger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalIdentityConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_external_user_header")]
    pub user_header: String,
    #[serde(default = "default_external_email_header")]
    pub email_header: String,
    #[serde(default = "default_external_name_header")]
    pub name_header: String,
    #[serde(default = "default_external_groups_header")]
    pub groups_header: String,
    #[serde(default = "default_external_group_delimiter")]
    pub group_delimiter: String,
    #[serde(default)]
    pub read_only_groups: Vec<String>,
    #[serde(default)]
    pub operator_groups: Vec<String>,
    #[serde(default)]
    pub config_editor_groups: Vec<String>,
    #[serde(default)]
    pub credential_manager_groups: Vec<String>,
    #[serde(default)]
    pub admin_groups: Vec<String>,
}

impl Default for ExternalIdentityConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            user_header: default_external_user_header(),
            email_header: default_external_email_header(),
            name_header: default_external_name_header(),
            groups_header: default_external_groups_header(),
            group_delimiter: default_external_group_delimiter(),
            read_only_groups: Vec::new(),
            operator_groups: Vec::new(),
            config_editor_groups: Vec::new(),
            credential_manager_groups: Vec::new(),
            admin_groups: Vec::new(),
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
    #[serde(default)]
    pub objective: RoutingObjective,
    #[serde(default)]
    pub model_group_objectives: HashMap<String, RoutingObjective>,
    #[serde(default)]
    pub budgets: RoutingBudgetConfig,
    #[serde(
        default = "default_model_group_strategy",
        alias = "default_alias_strategy"
    )]
    pub default_model_group_strategy: String,
    #[serde(default)]
    pub provider_order: Vec<String>,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            free_first: true,
            allow_paid_fallback: false,
            objective: RoutingObjective::default(),
            model_group_objectives: HashMap::new(),
            budgets: RoutingBudgetConfig::default(),
            default_model_group_strategy: default_model_group_strategy(),
            provider_order: Vec::new(),
        }
    }
}

/// Policy objective used to score otherwise eligible provider/model attempts.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RoutingObjective {
    MinCost,
    MinLatency,
    #[default]
    Balanced,
    QualityFirst,
    LocalOnly,
}

/// Hard budget limits. All values are USD.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RoutingBudgetConfig {
    #[serde(default)]
    pub max_cost_per_request_usd: Option<f64>,
    #[serde(default)]
    pub max_cost_per_day_usd: Option<f64>,
    #[serde(default)]
    pub max_cost_per_provider_per_day_usd: HashMap<String, f64>,
    #[serde(default)]
    pub max_cost_per_model_group_per_day_usd: HashMap<String, f64>,
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
    #[serde(default)]
    pub embedding_support: ProviderEmbeddingSupport,
}

/// How OpenAI-compatible local adapters should advertise embeddings support.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderEmbeddingSupport {
    /// Probe `/embeddings` during discovery before marking a local model as embedding-capable.
    #[default]
    Auto,
    /// Mark discovered models as embedding-capable without probing.
    Enabled,
    /// Do not advertise embeddings support for discovered local models.
    Disabled,
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
fn default_credential_key_env() -> String {
    "TOKENSCAVENGER_CREDENTIAL_KEY".to_string()
}
fn default_usage_retention_days() -> u32 {
    30
}
fn default_health_retention_days() -> u32 {
    30
}
fn default_audit_retention_days() -> u32 {
    90
}
fn default_trace_retention_days() -> u32 {
    30
}
fn default_update_repo() -> String {
    "kabudu/token-scavenger".to_string()
}
fn default_update_check_interval_secs() -> u64 {
    21_600
}
fn default_external_user_header() -> String {
    "x-auth-request-user".to_string()
}
fn default_external_email_header() -> String {
    "x-auth-request-email".to_string()
}
fn default_external_name_header() -> String {
    "x-auth-request-preferred-username".to_string()
}
fn default_external_groups_header() -> String {
    "x-auth-request-groups".to_string()
}
fn default_external_group_delimiter() -> String {
    ",".to_string()
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
fn default_model_group_strategy() -> String {
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
