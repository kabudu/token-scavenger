use crate::api::error::ApiError;
use crate::app::state::AppState;
use crate::router::selection::{RouteAttempt, TokenEstimate};
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use std::collections::HashSet;

const KEY_HASH_CONTEXT: &str = "tokenscavenger-project-key-v1";
pub const DEFAULT_PROJECT_ID: &str = "default";
pub const MASTER_KEY_PREFIX: &str = "master";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyProfile {
    Default,
    LocalOnly,
    FreeOnly,
}

impl PrivacyProfile {
    fn from_db(value: &str) -> Self {
        match value {
            "local_only" => Self::LocalOnly,
            "free_only" => Self::FreeOnly,
            _ => Self::Default,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::LocalOnly => "local_only",
            Self::FreeOnly => "free_only",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectPolicy {
    pub project_id: String,
    pub display_name: String,
    pub enabled: bool,
    pub organization_id: Option<String>,
    pub environment: Option<String>,
    pub owner_subject: Option<String>,
    pub owner_email: Option<String>,
    pub allowed_model_groups: Vec<String>,
    pub allow_paid_fallback: bool,
    pub provider_allowlist: Vec<String>,
    pub provider_denylist: Vec<String>,
    pub privacy_profile: PrivacyProfile,
    pub max_cost_per_request_usd: Option<f64>,
    pub max_cost_per_org_per_day_usd: Option<f64>,
    pub max_cost_per_environment_per_day_usd: Option<f64>,
    pub max_cost_per_day_usd: Option<f64>,
    pub max_requests_per_day: Option<i64>,
    pub max_input_tokens_per_day: Option<i64>,
    pub max_output_tokens_per_day: Option<i64>,
    pub sliding_window_seconds: Option<i64>,
    pub max_requests_per_window: Option<i64>,
    pub max_tokens_per_window: Option<i64>,
    pub webhook_url: Option<String>,
    pub webhook_events: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ClientProjectContext {
    pub project_id: String,
    pub display_name: String,
    pub api_key_prefix: String,
    pub enforce_policy: bool,
}

impl ClientProjectContext {
    pub fn master_default() -> Self {
        Self {
            project_id: DEFAULT_PROJECT_ID.to_string(),
            display_name: "Default project".to_string(),
            api_key_prefix: MASTER_KEY_PREFIX.to_string(),
            enforce_policy: false,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ProjectUpsert {
    pub project_id: Option<String>,
    pub display_name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub organization_id: Option<String>,
    #[serde(default)]
    pub environment: Option<String>,
    #[serde(default)]
    pub owner_subject: Option<String>,
    #[serde(default)]
    pub owner_email: Option<String>,
    #[serde(default)]
    pub allowed_model_groups: Vec<String>,
    #[serde(default)]
    pub allow_paid_fallback: bool,
    #[serde(default)]
    pub provider_allowlist: Vec<String>,
    #[serde(default)]
    pub provider_denylist: Vec<String>,
    #[serde(default)]
    pub privacy_profile: Option<PrivacyProfile>,
    #[serde(default)]
    pub max_cost_per_request_usd: Option<f64>,
    #[serde(default)]
    pub max_cost_per_org_per_day_usd: Option<f64>,
    #[serde(default)]
    pub max_cost_per_environment_per_day_usd: Option<f64>,
    #[serde(default)]
    pub max_cost_per_day_usd: Option<f64>,
    #[serde(default)]
    pub max_requests_per_day: Option<i64>,
    #[serde(default)]
    pub max_input_tokens_per_day: Option<i64>,
    #[serde(default)]
    pub max_output_tokens_per_day: Option<i64>,
    #[serde(default)]
    pub sliding_window_seconds: Option<i64>,
    #[serde(default)]
    pub max_requests_per_window: Option<i64>,
    #[serde(default)]
    pub max_tokens_per_window: Option<i64>,
    #[serde(default)]
    pub webhook_url: Option<String>,
    #[serde(default)]
    pub webhook_events: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ProjectPatch {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub organization_id: Option<String>,
    #[serde(default)]
    pub environment: Option<String>,
    #[serde(default)]
    pub owner_subject: Option<String>,
    #[serde(default)]
    pub owner_email: Option<String>,
    #[serde(default)]
    pub allowed_model_groups: Option<Vec<String>>,
    #[serde(default)]
    pub allow_paid_fallback: Option<bool>,
    #[serde(default)]
    pub provider_allowlist: Option<Vec<String>>,
    #[serde(default)]
    pub provider_denylist: Option<Vec<String>>,
    #[serde(default)]
    pub privacy_profile: Option<PrivacyProfile>,
    #[serde(default)]
    pub max_cost_per_request_usd: Option<f64>,
    #[serde(default)]
    pub max_cost_per_org_per_day_usd: Option<f64>,
    #[serde(default)]
    pub max_cost_per_environment_per_day_usd: Option<f64>,
    #[serde(default)]
    pub max_cost_per_day_usd: Option<f64>,
    #[serde(default)]
    pub max_requests_per_day: Option<i64>,
    #[serde(default)]
    pub max_input_tokens_per_day: Option<i64>,
    #[serde(default)]
    pub max_output_tokens_per_day: Option<i64>,
    #[serde(default)]
    pub sliding_window_seconds: Option<i64>,
    #[serde(default)]
    pub max_requests_per_window: Option<i64>,
    #[serde(default)]
    pub max_tokens_per_window: Option<i64>,
    #[serde(default)]
    pub webhook_url: Option<String>,
    #[serde(default)]
    pub webhook_events: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct IssueKeyRequest {
    pub label: String,
    #[serde(default)]
    pub owner_subject: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub rotation_grace_until: Option<String>,
    #[serde(default)]
    pub max_requests_per_day: Option<i64>,
    #[serde(default)]
    pub max_tokens_per_day: Option<i64>,
    #[serde(default)]
    pub max_cost_per_day_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IssuedProjectKey {
    pub project_id: String,
    pub key_prefix: String,
    pub api_key: String,
}

fn default_true() -> bool {
    true
}

pub fn generate_project_api_key() -> String {
    format!(
        "tsproj_{}_{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

pub fn key_prefix(api_key: &str) -> String {
    api_key.chars().take(18).collect()
}

pub fn hash_project_api_key(api_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(KEY_HASH_CONTEXT.as_bytes());
    hasher.update([0]);
    hasher.update(api_key.as_bytes());
    hex_digest(hasher.finalize().as_slice())
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

pub async fn authenticate_project_key(
    state: &AppState,
    api_key: &str,
) -> Result<Option<ClientProjectContext>, ApiError> {
    let hash = hash_project_api_key(api_key);
    let row = sqlx::query(
        "SELECT p.project_id, p.display_name, k.key_prefix
         FROM project_api_keys k
         JOIN projects p ON p.project_id = k.project_id
         WHERE k.key_hash = ?
           AND k.revoked_at IS NULL
           AND p.enabled = 1
           AND (
                k.expires_at IS NULL
                OR datetime(k.expires_at) > datetime('now')
                OR (k.rotation_grace_until IS NOT NULL AND datetime(k.rotation_grace_until) > datetime('now'))
           )",
    )
    .bind(hash)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| ApiError::InternalError(error.to_string()))?;

    let Some(row) = row else {
        return Ok(None);
    };

    let project_id: String = row.get(0);
    let display_name: String = row.get(1);
    let api_key_prefix: String = row.get(2);

    let _ = sqlx::query(
        "UPDATE project_api_keys SET last_used_at = datetime('now') WHERE key_prefix = ?",
    )
    .bind(&api_key_prefix)
    .execute(&state.db)
    .await;

    Ok(Some(ClientProjectContext {
        project_id,
        display_name,
        api_key_prefix,
        enforce_policy: true,
    }))
}

pub fn register_request_project(state: &AppState, request_id: &str, project: ClientProjectContext) {
    state
        .request_projects
        .insert(request_id.to_string(), project);
}

pub fn project_for_request(state: &AppState, request_id: &str) -> Option<ClientProjectContext> {
    state
        .request_projects
        .get(request_id)
        .map(|project| project.value().clone())
}

pub fn remove_request_project(state: &AppState, request_id: &str) -> Option<ClientProjectContext> {
    state
        .request_projects
        .remove(request_id)
        .map(|(_, project)| project)
}

pub async fn load_project_policy(
    db: &SqlitePool,
    project_id: &str,
) -> Result<Option<ProjectPolicy>, ApiError> {
    let row = sqlx::query(
        "SELECT project_id, display_name, enabled, organization_id, environment, owner_subject, owner_email,
                allowed_model_groups_json, allow_paid_fallback, provider_allowlist_json,
                provider_denylist_json, privacy_profile, max_cost_per_request_usd,
                max_cost_per_org_per_day_usd, max_cost_per_environment_per_day_usd,
                max_cost_per_day_usd, max_requests_per_day, max_input_tokens_per_day,
                max_output_tokens_per_day, sliding_window_seconds, max_requests_per_window,
                max_tokens_per_window, webhook_url, webhook_events_json
         FROM projects WHERE project_id = ?",
    )
    .bind(project_id)
    .fetch_optional(db)
    .await
    .map_err(|error| ApiError::InternalError(error.to_string()))?;

    Ok(row.map(policy_from_row))
}

fn policy_from_row(row: sqlx::sqlite::SqliteRow) -> ProjectPolicy {
    ProjectPolicy {
        project_id: row.get(0),
        display_name: row.get(1),
        enabled: row.get(2),
        organization_id: row.get(3),
        environment: row.get(4),
        owner_subject: row.get(5),
        owner_email: row.get(6),
        allowed_model_groups: json_vec(row.get::<String, _>(7).as_str()),
        allow_paid_fallback: row.get(8),
        provider_allowlist: json_vec(row.get::<String, _>(9).as_str()),
        provider_denylist: json_vec(row.get::<String, _>(10).as_str()),
        privacy_profile: PrivacyProfile::from_db(row.get::<String, _>(11).as_str()),
        max_cost_per_request_usd: row.get(12),
        max_cost_per_org_per_day_usd: row.get(13),
        max_cost_per_environment_per_day_usd: row.get(14),
        max_cost_per_day_usd: row.get(15),
        max_requests_per_day: row.get(16),
        max_input_tokens_per_day: row.get(17),
        max_output_tokens_per_day: row.get(18),
        sliding_window_seconds: row.get(19),
        max_requests_per_window: row.get(20),
        max_tokens_per_window: row.get(21),
        webhook_url: row.get(22),
        webhook_events: json_vec(row.get::<String, _>(23).as_str()),
    }
}

fn json_vec(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

fn json_array(values: &[String]) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "[]".to_string())
}

pub async fn filter_project_policy(
    plan: Vec<RouteAttempt>,
    state: &AppState,
    request_id: &str,
    requested_model: &str,
    token_estimate: TokenEstimate,
) -> Result<Vec<RouteAttempt>, ApiError> {
    let Some(context) = project_for_request(state, request_id) else {
        return Ok(plan);
    };
    if !context.enforce_policy {
        return Ok(plan);
    }
    let Some(policy) = load_project_policy(&state.db, &context.project_id).await? else {
        return Ok(Vec::new());
    };

    let mut filtered = Vec::with_capacity(plan.len());
    for attempt in plan {
        let reasons = project_policy_skip_reasons(
            state,
            &policy,
            Some(&context.api_key_prefix),
            requested_model,
            &attempt,
            token_estimate,
        )
        .await?;
        if reasons.is_empty() {
            filtered.push(attempt);
        } else {
            tracing::info!(
                project_id = %policy.project_id,
                provider = %attempt.provider_id,
                model = %attempt.model_id,
                reasons = ?reasons,
                "Filtered out by project policy"
            );
            emit_project_webhook(
                state,
                &policy,
                "project_policy_block",
                serde_json::json!({
                    "requested_model": requested_model,
                    "provider_id": attempt.provider_id,
                    "model_id": attempt.model_id,
                    "reasons": reasons,
                }),
            )
            .await;
        }
    }

    Ok(filtered)
}

pub async fn project_policy_skip_reasons(
    state: &AppState,
    policy: &ProjectPolicy,
    api_key_prefix: Option<&str>,
    requested_model: &str,
    attempt: &RouteAttempt,
    token_estimate: TokenEstimate,
) -> Result<Vec<String>, ApiError> {
    if !policy.enabled {
        return Ok(vec!["filtered by disabled project".to_string()]);
    }

    let mut reasons = Vec::new();
    if !policy.allowed_model_groups.is_empty()
        && !policy
            .allowed_model_groups
            .iter()
            .any(|model| model == requested_model)
    {
        reasons.push(format!(
            "filtered by project model-group policy: {requested_model} is not allowed"
        ));
    }

    let provider_allowlist: HashSet<_> = policy.provider_allowlist.iter().collect();
    if !provider_allowlist.is_empty() && !provider_allowlist.contains(&attempt.provider_id) {
        reasons.push("filtered by project provider allowlist".to_string());
    }
    if policy
        .provider_denylist
        .iter()
        .any(|provider| provider == &attempt.provider_id)
    {
        reasons.push("filtered by project provider denylist".to_string());
    }

    let free_only = provider_is_free_only(state, &attempt.provider_id);
    if !free_only && !policy.allow_paid_fallback {
        reasons.push("filtered by project paid-fallback policy".to_string());
    }
    match policy.privacy_profile {
        PrivacyProfile::Default => {}
        PrivacyProfile::FreeOnly if !free_only => {
            reasons.push("filtered by project free-only privacy profile".to_string());
        }
        PrivacyProfile::LocalOnly if !is_local_provider(state, &attempt.provider_id) => {
            reasons.push("filtered by project local-only privacy profile".to_string());
        }
        _ => {}
    }

    let has_cost_budget =
        policy.max_cost_per_request_usd.is_some() || policy.max_cost_per_day_usd.is_some();
    if !free_only && has_cost_budget {
        let Some(estimated_cost) = estimate_project_attempt_cost(
            state,
            &attempt.provider_id,
            &attempt.model_id,
            token_estimate,
        )
        .await?
        else {
            reasons
                .push("filtered by project hard budget because paid price is unknown".to_string());
            return Ok(reasons);
        };
        if let Some(limit) = policy.max_cost_per_request_usd {
            if estimated_cost > limit {
                reasons.push(format!(
                    "filtered by project per-request budget: estimate {:.6} > limit {:.6}",
                    estimated_cost, limit
                ));
            }
        }
        if let Some(limit) = policy.max_cost_per_day_usd {
            let spent = project_spend_today(state, &policy.project_id).await?;
            if spent + estimated_cost > limit {
                reasons.push(format!(
                    "filtered by project daily budget: projected {:.6} > limit {:.6}",
                    spent + estimated_cost,
                    limit
                ));
            }
        }
        if let (Some(org_id), Some(limit)) = (
            policy.organization_id.as_deref(),
            policy.max_cost_per_org_per_day_usd,
        ) {
            let spent = scoped_spend_today(state, "organization_id", org_id).await?;
            if spent + estimated_cost > limit {
                reasons.push(format!(
                    "filtered by organization daily budget: projected {:.6} > limit {:.6}",
                    spent + estimated_cost,
                    limit
                ));
            }
        }
        if let (Some(environment), Some(limit)) = (
            policy.environment.as_deref(),
            policy.max_cost_per_environment_per_day_usd,
        ) {
            let spent = scoped_spend_today(state, "environment", environment).await?;
            if spent + estimated_cost > limit {
                reasons.push(format!(
                    "filtered by environment daily budget: projected {:.6} > limit {:.6}",
                    spent + estimated_cost,
                    limit
                ));
            }
        }
    }

    if let Some(limit) = policy.max_requests_per_day {
        let used = project_requests_since(state, &policy.project_id, "date('now')").await?;
        if used + 1 > limit {
            reasons.push(format!(
                "filtered by project daily request limit: projected {} > limit {}",
                used + 1,
                limit
            ));
        }
    }
    if let Some(limit) = policy.max_input_tokens_per_day {
        let used = project_tokens_since(state, &policy.project_id, "date('now')", "input").await?;
        if used + i64::from(token_estimate.input_tokens) > limit {
            reasons.push(format!(
                "filtered by project daily input-token limit: projected {} > limit {}",
                used + i64::from(token_estimate.input_tokens),
                limit
            ));
        }
    }
    if let Some(limit) = policy.max_output_tokens_per_day {
        let used = project_tokens_since(state, &policy.project_id, "date('now')", "output").await?;
        if used + i64::from(token_estimate.output_tokens) > limit {
            reasons.push(format!(
                "filtered by project daily output-token limit: projected {} > limit {}",
                used + i64::from(token_estimate.output_tokens),
                limit
            ));
        }
    }
    if let Some(window) = policy.sliding_window_seconds {
        let window_expr = format!("datetime('now', '-{} seconds')", window.max(1));
        if let Some(limit) = policy.max_requests_per_window {
            let used = project_requests_since(state, &policy.project_id, &window_expr).await?;
            if used + 1 > limit {
                reasons.push(format!(
                    "filtered by project sliding request limit: projected {} > limit {}",
                    used + 1,
                    limit
                ));
            }
        }
        if let Some(limit) = policy.max_tokens_per_window {
            let used =
                project_tokens_since(state, &policy.project_id, &window_expr, "total").await?;
            let projected = used
                + i64::from(token_estimate.input_tokens)
                + i64::from(token_estimate.output_tokens);
            if projected > limit {
                reasons.push(format!(
                    "filtered by project sliding token limit: projected {} > limit {}",
                    projected, limit
                ));
            }
        }
    }
    if let Some(prefix) = api_key_prefix {
        let estimated_for_key = if provider_is_free_only(state, &attempt.provider_id) {
            Some(0.0)
        } else {
            estimate_project_attempt_cost(
                state,
                &attempt.provider_id,
                &attempt.model_id,
                token_estimate,
            )
            .await?
        };
        reasons.extend(
            key_budget_skip_reasons(state, prefix, estimated_for_key, token_estimate).await?,
        );
    }

    Ok(reasons)
}

async fn estimate_project_attempt_cost(
    state: &AppState,
    provider_id: &str,
    model_id: &str,
    token_estimate: TokenEstimate,
) -> Result<Option<f64>, ApiError> {
    let usage = crate::usage::pricing_catalog::PricingUsage {
        input_tokens: token_estimate.input_tokens,
        output_tokens: token_estimate.output_tokens,
        ..Default::default()
    };
    let rate = crate::usage::pricing_catalog::lookup_rate(&state.db, provider_id, model_id)
        .await
        .map_err(|error| ApiError::InternalError(error.to_string()))?;
    Ok(rate.map(|rate| crate::usage::pricing_catalog::calculate_cost(&rate, &usage).amount_usd))
}

fn provider_is_free_only(state: &AppState, provider_id: &str) -> bool {
    state
        .config()
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .map(|provider| provider.free_only)
        .unwrap_or(true)
}

fn is_local_provider(state: &AppState, provider_id: &str) -> bool {
    if matches!(provider_id, "local" | "ollama" | "llama-cpp" | "lmstudio") {
        return true;
    }
    let config = state.config();
    let Some(base_url) = config
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .and_then(|provider| provider.base_url.as_deref())
    else {
        return false;
    };
    let Ok(url) = url::Url::parse(base_url) else {
        return false;
    };
    matches!(
        url.host_str(),
        Some("localhost") | Some("127.0.0.1") | Some("::1")
    )
}

async fn project_spend_today(state: &AppState, project_id: &str) -> Result<f64, ApiError> {
    sqlx::query_as::<_, (f64,)>(
        "SELECT COALESCE(SUM(estimated_cost_usd), 0.0)
         FROM usage_events
         WHERE project_id = ? AND timestamp >= date('now')",
    )
    .bind(project_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| ApiError::InternalError(error.to_string()))
    .map(|row| row.map(|row| row.0).unwrap_or(0.0))
}

async fn scoped_spend_today(
    state: &AppState,
    scope_column: &str,
    scope_id: &str,
) -> Result<f64, ApiError> {
    let column = match scope_column {
        "organization_id" => "organization_id",
        "environment" => "environment",
        _ => return Ok(0.0),
    };
    let query = format!(
        "SELECT COALESCE(SUM(u.estimated_cost_usd), 0.0)
         FROM usage_events u
         JOIN projects p ON p.project_id = u.project_id
         WHERE p.{column} = ? AND u.timestamp >= date('now')"
    );
    sqlx::query_as::<_, (f64,)>(&query)
        .bind(scope_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|error| ApiError::InternalError(error.to_string()))
        .map(|row| row.map(|row| row.0).unwrap_or(0.0))
}

async fn key_budget_skip_reasons(
    state: &AppState,
    api_key_prefix: &str,
    estimated_cost: Option<f64>,
    token_estimate: TokenEstimate,
) -> Result<Vec<String>, ApiError> {
    let Some((request_limit, token_limit, cost_limit)) =
        sqlx::query_as::<_, (Option<i64>, Option<i64>, Option<f64>)>(
            "SELECT max_requests_per_day, max_tokens_per_day, max_cost_per_day_usd
         FROM project_api_keys
         WHERE key_prefix = ?",
        )
        .bind(api_key_prefix)
        .fetch_optional(&state.db)
        .await
        .map_err(|error| ApiError::InternalError(error.to_string()))?
    else {
        return Ok(Vec::new());
    };

    let mut reasons = Vec::new();
    if let Some(limit) = request_limit {
        let used = sqlx::query_as::<_, (i64,)>(
            "SELECT COUNT(*) FROM request_log
             WHERE api_key_prefix = ? AND received_at >= date('now')",
        )
        .bind(api_key_prefix)
        .fetch_optional(&state.db)
        .await
        .map_err(|error| ApiError::InternalError(error.to_string()))?
        .map(|row| row.0)
        .unwrap_or(0);
        if used + 1 > limit {
            reasons.push(format!(
                "filtered by key daily request limit: projected {} > limit {}",
                used + 1,
                limit
            ));
        }
    }
    if let Some(limit) = token_limit {
        let used = sqlx::query_as::<_, (i64,)>(
            "SELECT COALESCE(SUM(input_tokens + output_tokens), 0)
             FROM usage_events
             WHERE api_key_prefix = ? AND timestamp >= date('now')",
        )
        .bind(api_key_prefix)
        .fetch_optional(&state.db)
        .await
        .map_err(|error| ApiError::InternalError(error.to_string()))?
        .map(|row| row.0)
        .unwrap_or(0);
        let projected =
            used + i64::from(token_estimate.input_tokens) + i64::from(token_estimate.output_tokens);
        if projected > limit {
            reasons.push(format!(
                "filtered by key daily token limit: projected {} > limit {}",
                projected, limit
            ));
        }
    }
    if let Some(limit) = cost_limit {
        let Some(estimated_cost) = estimated_cost else {
            return Ok(vec![
                "filtered by key daily cost budget because paid price is unknown".to_string(),
            ]);
        };
        let spent = sqlx::query_as::<_, (f64,)>(
            "SELECT COALESCE(SUM(estimated_cost_usd), 0.0)
             FROM usage_events
             WHERE api_key_prefix = ? AND timestamp >= date('now')",
        )
        .bind(api_key_prefix)
        .fetch_optional(&state.db)
        .await
        .map_err(|error| ApiError::InternalError(error.to_string()))?
        .map(|row| row.0)
        .unwrap_or(0.0);
        if spent + estimated_cost > limit {
            reasons.push(format!(
                "filtered by key daily cost budget: projected {:.6} > limit {:.6}",
                spent + estimated_cost,
                limit
            ));
        }
    }
    Ok(reasons)
}

async fn project_requests_since(
    state: &AppState,
    project_id: &str,
    since_expr: &str,
) -> Result<i64, ApiError> {
    let query = format!(
        "SELECT COUNT(*) FROM request_log WHERE project_id = ? AND received_at >= {since_expr}"
    );
    sqlx::query_as::<_, (i64,)>(&query)
        .bind(project_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|error| ApiError::InternalError(error.to_string()))
        .map(|row| row.map(|row| row.0).unwrap_or(0))
}

async fn project_tokens_since(
    state: &AppState,
    project_id: &str,
    since_expr: &str,
    kind: &str,
) -> Result<i64, ApiError> {
    let column_expr = match kind {
        "input" => "input_tokens",
        "output" => "output_tokens",
        _ => "input_tokens + output_tokens",
    };
    let query = format!(
        "SELECT COALESCE(SUM({column_expr}), 0)
         FROM usage_events
         WHERE project_id = ? AND timestamp >= {since_expr}"
    );
    sqlx::query_as::<_, (i64,)>(&query)
        .bind(project_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|error| ApiError::InternalError(error.to_string()))
        .map(|row| row.map(|row| row.0).unwrap_or(0))
}

pub async fn list_projects(state: &AppState) -> Result<serde_json::Value, ApiError> {
    let rows = sqlx::query(
        "SELECT project_id, display_name, description, enabled, organization_id, environment, owner_subject, owner_email,
                allowed_model_groups_json, allow_paid_fallback, provider_allowlist_json,
                provider_denylist_json, privacy_profile, max_cost_per_request_usd,
                max_cost_per_org_per_day_usd, max_cost_per_environment_per_day_usd,
                max_cost_per_day_usd, max_requests_per_day, max_input_tokens_per_day,
                max_output_tokens_per_day, sliding_window_seconds, max_requests_per_window,
                max_tokens_per_window, webhook_url, webhook_events_json, created_at, updated_at
         FROM projects ORDER BY project_id",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|error| ApiError::InternalError(error.to_string()))?;

    let mut projects = Vec::with_capacity(rows.len());
    for row in rows {
        let project_id: String = row.get(0);
        let keys = list_project_keys_value(state, &project_id).await?;
        projects.push(project_value(row, keys));
    }
    Ok(serde_json::json!({ "projects": projects }))
}

fn project_value(row: sqlx::sqlite::SqliteRow, keys: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "project_id": row.get::<String, _>(0),
        "display_name": row.get::<String, _>(1),
        "description": row.get::<Option<String>, _>(2),
        "enabled": row.get::<bool, _>(3),
        "organization_id": row.get::<Option<String>, _>(4),
        "environment": row.get::<Option<String>, _>(5),
        "owner_subject": row.get::<Option<String>, _>(6),
        "owner_email": row.get::<Option<String>, _>(7),
        "allowed_model_groups": json_vec(row.get::<String, _>(8).as_str()),
        "allow_paid_fallback": row.get::<bool, _>(9),
        "provider_allowlist": json_vec(row.get::<String, _>(10).as_str()),
        "provider_denylist": json_vec(row.get::<String, _>(11).as_str()),
        "privacy_profile": row.get::<String, _>(12),
        "max_cost_per_request_usd": row.get::<Option<f64>, _>(13),
        "max_cost_per_org_per_day_usd": row.get::<Option<f64>, _>(14),
        "max_cost_per_environment_per_day_usd": row.get::<Option<f64>, _>(15),
        "max_cost_per_day_usd": row.get::<Option<f64>, _>(16),
        "max_requests_per_day": row.get::<Option<i64>, _>(17),
        "max_input_tokens_per_day": row.get::<Option<i64>, _>(18),
        "max_output_tokens_per_day": row.get::<Option<i64>, _>(19),
        "sliding_window_seconds": row.get::<Option<i64>, _>(20),
        "max_requests_per_window": row.get::<Option<i64>, _>(21),
        "max_tokens_per_window": row.get::<Option<i64>, _>(22),
        "webhook_url": row.get::<Option<String>, _>(23).map(|_| "<redacted>"),
        "webhook_events": json_vec(row.get::<String, _>(24).as_str()),
        "created_at": row.get::<String, _>(25),
        "updated_at": row.get::<String, _>(26),
        "keys": keys,
    })
}

pub async fn create_project(
    state: &AppState,
    body: ProjectUpsert,
    actor: &str,
) -> Result<serde_json::Value, ApiError> {
    validate_project_input(&body)?;
    let project_id = body
        .project_id
        .unwrap_or_else(|| slugify_project_id(&body.display_name));
    let privacy = body.privacy_profile.unwrap_or(PrivacyProfile::Default);
    sqlx::query(
        "INSERT INTO projects (
            project_id, display_name, description, enabled, organization_id, environment, owner_subject, owner_email,
            allowed_model_groups_json, allow_paid_fallback, provider_allowlist_json,
            provider_denylist_json, privacy_profile, max_cost_per_request_usd,
            max_cost_per_org_per_day_usd, max_cost_per_environment_per_day_usd,
            max_cost_per_day_usd, max_requests_per_day, max_input_tokens_per_day,
            max_output_tokens_per_day, sliding_window_seconds, max_requests_per_window,
            max_tokens_per_window, webhook_url, webhook_events_json, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))",
    )
    .bind(&project_id)
    .bind(&body.display_name)
    .bind(&body.description)
    .bind(body.enabled)
    .bind(&body.organization_id)
    .bind(&body.environment)
    .bind(&body.owner_subject)
    .bind(&body.owner_email)
    .bind(json_array(&body.allowed_model_groups))
    .bind(body.allow_paid_fallback)
    .bind(json_array(&body.provider_allowlist))
    .bind(json_array(&body.provider_denylist))
    .bind(privacy.as_str())
    .bind(body.max_cost_per_request_usd)
    .bind(body.max_cost_per_org_per_day_usd)
    .bind(body.max_cost_per_environment_per_day_usd)
    .bind(body.max_cost_per_day_usd)
    .bind(body.max_requests_per_day)
    .bind(body.max_input_tokens_per_day)
    .bind(body.max_output_tokens_per_day)
    .bind(body.sliding_window_seconds)
    .bind(body.max_requests_per_window)
    .bind(body.max_tokens_per_window)
    .bind(body.webhook_url.as_deref())
    .bind(json_array(&body.webhook_events))
    .execute(&state.db)
    .await
    .map_err(|error| ApiError::InvalidRequest(format!("Project create failed: {error}")))?;

    audit_project(state, actor, "project_create", &project_id).await;
    Ok(serde_json::json!({ "status": "ok", "project_id": project_id }))
}

fn validate_project_input(body: &ProjectUpsert) -> Result<(), ApiError> {
    validate_non_negative("max_cost_per_request_usd", body.max_cost_per_request_usd)?;
    validate_non_negative(
        "max_cost_per_org_per_day_usd",
        body.max_cost_per_org_per_day_usd,
    )?;
    validate_non_negative(
        "max_cost_per_environment_per_day_usd",
        body.max_cost_per_environment_per_day_usd,
    )?;
    validate_non_negative("max_cost_per_day_usd", body.max_cost_per_day_usd)?;
    validate_non_negative_i64("max_requests_per_day", body.max_requests_per_day)?;
    validate_non_negative_i64("max_input_tokens_per_day", body.max_input_tokens_per_day)?;
    validate_non_negative_i64("max_output_tokens_per_day", body.max_output_tokens_per_day)?;
    validate_non_negative_i64("sliding_window_seconds", body.sliding_window_seconds)?;
    validate_non_negative_i64("max_requests_per_window", body.max_requests_per_window)?;
    validate_non_negative_i64("max_tokens_per_window", body.max_tokens_per_window)?;
    Ok(())
}

fn validate_non_negative(name: &str, value: Option<f64>) -> Result<(), ApiError> {
    if value.is_some_and(|value| value < 0.0) {
        return Err(ApiError::InvalidRequest(format!("{name} must be >= 0")));
    }
    Ok(())
}

fn validate_non_negative_i64(name: &str, value: Option<i64>) -> Result<(), ApiError> {
    if value.is_some_and(|value| value < 0) {
        return Err(ApiError::InvalidRequest(format!("{name} must be >= 0")));
    }
    Ok(())
}

fn slugify_project_id(name: &str) -> String {
    let slug = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        format!("project-{}", uuid::Uuid::new_v4().simple())
    } else {
        slug
    }
}

pub async fn update_project(
    state: &AppState,
    project_id: &str,
    body: ProjectPatch,
    actor: &str,
) -> Result<serde_json::Value, ApiError> {
    let current = load_project_policy(&state.db, project_id).await?;
    if current.is_none() {
        return Err(ApiError::InvalidRequest(format!(
            "project {project_id} not found"
        )));
    }
    sqlx::query(
        "UPDATE projects SET
            display_name = COALESCE(?, display_name),
            description = COALESCE(?, description),
            enabled = COALESCE(?, enabled),
            organization_id = COALESCE(?, organization_id),
            environment = COALESCE(?, environment),
            owner_subject = COALESCE(?, owner_subject),
            owner_email = COALESCE(?, owner_email),
            allowed_model_groups_json = COALESCE(?, allowed_model_groups_json),
            allow_paid_fallback = COALESCE(?, allow_paid_fallback),
            provider_allowlist_json = COALESCE(?, provider_allowlist_json),
            provider_denylist_json = COALESCE(?, provider_denylist_json),
            privacy_profile = COALESCE(?, privacy_profile),
            max_cost_per_request_usd = COALESCE(?, max_cost_per_request_usd),
            max_cost_per_org_per_day_usd = COALESCE(?, max_cost_per_org_per_day_usd),
            max_cost_per_environment_per_day_usd = COALESCE(?, max_cost_per_environment_per_day_usd),
            max_cost_per_day_usd = COALESCE(?, max_cost_per_day_usd),
            max_requests_per_day = COALESCE(?, max_requests_per_day),
            max_input_tokens_per_day = COALESCE(?, max_input_tokens_per_day),
            max_output_tokens_per_day = COALESCE(?, max_output_tokens_per_day),
            sliding_window_seconds = COALESCE(?, sliding_window_seconds),
            max_requests_per_window = COALESCE(?, max_requests_per_window),
            max_tokens_per_window = COALESCE(?, max_tokens_per_window),
            webhook_url = COALESCE(?, webhook_url),
            webhook_events_json = COALESCE(?, webhook_events_json),
            updated_at = datetime('now')
         WHERE project_id = ?",
    )
    .bind(body.display_name.as_deref())
    .bind(body.description.as_deref())
    .bind(body.enabled)
    .bind(body.organization_id.as_deref())
    .bind(body.environment.as_deref())
    .bind(body.owner_subject.as_deref())
    .bind(body.owner_email.as_deref())
    .bind(body.allowed_model_groups.as_ref().map(|v| json_array(v)))
    .bind(body.allow_paid_fallback)
    .bind(body.provider_allowlist.as_ref().map(|v| json_array(v)))
    .bind(body.provider_denylist.as_ref().map(|v| json_array(v)))
    .bind(body.privacy_profile.as_ref().map(PrivacyProfile::as_str))
    .bind(body.max_cost_per_request_usd)
    .bind(body.max_cost_per_org_per_day_usd)
    .bind(body.max_cost_per_environment_per_day_usd)
    .bind(body.max_cost_per_day_usd)
    .bind(body.max_requests_per_day)
    .bind(body.max_input_tokens_per_day)
    .bind(body.max_output_tokens_per_day)
    .bind(body.sliding_window_seconds)
    .bind(body.max_requests_per_window)
    .bind(body.max_tokens_per_window)
    .bind(body.webhook_url.as_deref())
    .bind(body.webhook_events.as_ref().map(|v| json_array(v)))
    .bind(project_id)
    .execute(&state.db)
    .await
    .map_err(|error| ApiError::InternalError(error.to_string()))?;
    audit_project(state, actor, "project_update", project_id).await;
    Ok(serde_json::json!({ "status": "ok", "project_id": project_id }))
}

pub async fn delete_project(
    state: &AppState,
    project_id: &str,
    actor: &str,
) -> Result<serde_json::Value, ApiError> {
    if project_id == DEFAULT_PROJECT_ID {
        return Err(ApiError::InvalidRequest(
            "default project cannot be deleted".into(),
        ));
    }
    sqlx::query(
        "UPDATE projects SET enabled = 0, updated_at = datetime('now') WHERE project_id = ?",
    )
    .bind(project_id)
    .execute(&state.db)
    .await
    .map_err(|error| ApiError::InternalError(error.to_string()))?;
    sqlx::query("UPDATE project_api_keys SET revoked_at = COALESCE(revoked_at, datetime('now')) WHERE project_id = ?")
        .bind(project_id)
        .execute(&state.db)
        .await
        .map_err(|error| ApiError::InternalError(error.to_string()))?;
    audit_project(state, actor, "project_disable", project_id).await;
    Ok(serde_json::json!({ "status": "ok", "project_id": project_id }))
}

pub async fn issue_project_key(
    state: &AppState,
    project_id: &str,
    body: IssueKeyRequest,
    actor: &str,
) -> Result<IssuedProjectKey, ApiError> {
    if body.label.trim().is_empty() {
        return Err(ApiError::InvalidRequest("key label is required".into()));
    }
    if load_project_policy(&state.db, project_id).await?.is_none() {
        return Err(ApiError::InvalidRequest(format!(
            "project {project_id} not found"
        )));
    }
    let api_key = generate_project_api_key();
    let prefix = key_prefix(&api_key);
    let hash = hash_project_api_key(&api_key);
    sqlx::query(
        "INSERT INTO project_api_keys (
            project_id, label, owner_subject, key_prefix, key_hash, expires_at, rotation_grace_until,
            max_requests_per_day, max_tokens_per_day, max_cost_per_day_usd
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(project_id)
    .bind(&body.label)
    .bind(body.owner_subject.as_deref())
    .bind(&prefix)
    .bind(hash)
    .bind(body.expires_at.as_deref())
    .bind(body.rotation_grace_until.as_deref())
    .bind(body.max_requests_per_day)
    .bind(body.max_tokens_per_day)
    .bind(body.max_cost_per_day_usd)
    .execute(&state.db)
    .await
    .map_err(|error| ApiError::InternalError(error.to_string()))?;
    audit_project(state, actor, "project_key_issue", project_id).await;
    Ok(IssuedProjectKey {
        project_id: project_id.to_string(),
        key_prefix: prefix,
        api_key,
    })
}

pub async fn revoke_project_key(
    state: &AppState,
    project_id: &str,
    key_prefix: &str,
    actor: &str,
) -> Result<serde_json::Value, ApiError> {
    sqlx::query(
        "UPDATE project_api_keys
         SET revoked_at = COALESCE(revoked_at, datetime('now'))
         WHERE project_id = ? AND key_prefix = ?",
    )
    .bind(project_id)
    .bind(key_prefix)
    .execute(&state.db)
    .await
    .map_err(|error| ApiError::InternalError(error.to_string()))?;
    audit_project(state, actor, "project_key_revoke", project_id).await;
    Ok(serde_json::json!({ "status": "ok", "project_id": project_id, "key_prefix": key_prefix }))
}

async fn list_project_keys_value(
    state: &AppState,
    project_id: &str,
) -> Result<serde_json::Value, ApiError> {
    let rows = sqlx::query(
        "SELECT label, owner_subject, key_prefix, created_at, expires_at, rotation_grace_until,
                max_requests_per_day, max_tokens_per_day, max_cost_per_day_usd,
                revoked_at, last_used_at
         FROM project_api_keys
         WHERE project_id = ?
         ORDER BY created_at DESC",
    )
    .bind(project_id)
    .fetch_all(&state.db)
    .await
    .map_err(|error| ApiError::InternalError(error.to_string()))?;
    Ok(serde_json::Value::Array(
        rows.into_iter()
            .map(|row| {
                serde_json::json!({
                    "label": row.get::<String, _>(0),
                    "owner_subject": row.get::<Option<String>, _>(1),
                    "key_prefix": row.get::<String, _>(2),
                    "created_at": row.get::<String, _>(3),
                    "expires_at": row.get::<Option<String>, _>(4),
                    "rotation_grace_until": row.get::<Option<String>, _>(5),
                    "max_requests_per_day": row.get::<Option<i64>, _>(6),
                    "max_tokens_per_day": row.get::<Option<i64>, _>(7),
                    "max_cost_per_day_usd": row.get::<Option<f64>, _>(8),
                    "revoked_at": row.get::<Option<String>, _>(9),
                    "last_used_at": row.get::<Option<String>, _>(10),
                })
            })
            .collect(),
    ))
}

pub async fn project_usage(
    state: &AppState,
    project_id: &str,
) -> Result<serde_json::Value, ApiError> {
    let summary = sqlx::query_as::<_, (i64, i64, i64, f64)>(
        "SELECT COUNT(DISTINCT r.request_id),
                COALESCE(SUM(u.input_tokens), 0),
                COALESCE(SUM(u.output_tokens), 0),
                COALESCE(SUM(u.estimated_cost_usd), 0.0)
         FROM request_log r
         LEFT JOIN usage_events u ON u.request_id = r.request_id
         WHERE r.project_id = ? AND r.received_at >= date('now')",
    )
    .bind(project_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| ApiError::InternalError(error.to_string()))?
    .unwrap_or((0, 0, 0, 0.0));
    let recent = project_usage_rows(state, project_id, 100).await?;
    Ok(serde_json::json!({
        "project_id": project_id,
        "today": {
            "requests": summary.0,
            "input_tokens": summary.1,
            "output_tokens": summary.2,
            "estimated_cost_usd": summary.3,
        },
        "recent": recent
    }))
}

async fn project_usage_rows(
    state: &AppState,
    project_id: &str,
    limit: i64,
) -> Result<Vec<serde_json::Value>, ApiError> {
    let rows = sqlx::query(
        "SELECT r.request_id, r.received_at, r.endpoint_kind, r.requested_model,
                r.selected_provider_id, r.selected_model_id, r.status, r.http_status,
                r.api_key_prefix, COALESCE(u.input_tokens, 0), COALESCE(u.output_tokens, 0),
                COALESCE(u.estimated_cost_usd, 0.0)
         FROM request_log r
         LEFT JOIN usage_events u ON u.request_id = r.request_id
         WHERE r.project_id = ?
         ORDER BY r.received_at DESC
         LIMIT ?",
    )
    .bind(project_id)
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map_err(|error| ApiError::InternalError(error.to_string()))?;
    Ok(rows
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "request_id": row.get::<String, _>(0),
                "received_at": row.get::<String, _>(1),
                "endpoint_kind": row.get::<String, _>(2),
                "requested_model": row.get::<Option<String>, _>(3),
                "selected_provider_id": row.get::<Option<String>, _>(4),
                "selected_model_id": row.get::<Option<String>, _>(5),
                "status": row.get::<String, _>(6),
                "http_status": row.get::<Option<i64>, _>(7),
                "api_key_prefix": row.get::<Option<String>, _>(8),
                "input_tokens": row.get::<i64, _>(9),
                "output_tokens": row.get::<i64, _>(10),
                "estimated_cost_usd": row.get::<f64, _>(11),
            })
        })
        .collect())
}

pub async fn project_export_csv(state: &AppState, project_id: &str) -> Result<String, ApiError> {
    let rows = project_usage_rows(state, project_id, 10_000).await?;
    let mut out = String::from(
        "request_id,received_at,endpoint_kind,requested_model,provider,model,status,http_status,api_key_prefix,input_tokens,output_tokens,estimated_cost_usd\n",
    );
    for row in rows {
        let fields = vec![
            row["request_id"].as_str().unwrap_or_default().to_string(),
            row["received_at"].as_str().unwrap_or_default().to_string(),
            row["endpoint_kind"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            row["requested_model"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            row["selected_provider_id"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            row["selected_model_id"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            row["status"].as_str().unwrap_or_default().to_string(),
            row["http_status"].as_i64().unwrap_or_default().to_string(),
            row["api_key_prefix"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            row["input_tokens"].as_i64().unwrap_or_default().to_string(),
            row["output_tokens"]
                .as_i64()
                .unwrap_or_default()
                .to_string(),
            row["estimated_cost_usd"]
                .as_f64()
                .unwrap_or_default()
                .to_string(),
        ];
        out.push_str(&csv_line(&fields));
        out.push('\n');
    }
    Ok(out)
}

fn csv_line(fields: &[String]) -> String {
    fields
        .iter()
        .map(|field| format!("\"{}\"", field.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(",")
}

pub async fn project_diagnostic_bundle(
    state: &AppState,
    project_id: &str,
) -> Result<serde_json::Value, ApiError> {
    Ok(serde_json::json!({
        "project": load_project_policy(&state.db, project_id).await?,
        "usage": project_usage(state, project_id).await?,
        "recent_traces": project_usage_rows(state, project_id, 25).await?,
        "redaction": "project diagnostic bundles include no plaintext API keys"
    }))
}

pub async fn can_manage_project(
    state: &AppState,
    auth: Option<&crate::api::auth::AuthContext>,
    project_id: &str,
) -> Result<bool, ApiError> {
    let Some(auth) = auth else {
        return Ok(true);
    };
    if auth.role >= crate::api::auth::AdminRole::ConfigEditor || auth.role.can_manage_credentials()
    {
        return Ok(true);
    }
    let Some(policy) = load_project_policy(&state.db, project_id).await? else {
        return Ok(false);
    };
    Ok(policy
        .owner_subject
        .as_ref()
        .is_some_and(|owner| owner == &auth.subject))
}

async fn audit_project(state: &AppState, actor: &str, action: &str, project_id: &str) {
    let _ = sqlx::query(
        "INSERT INTO config_audit_log (actor, action, target_type, target_id)
         VALUES (?, ?, 'project', ?)",
    )
    .bind(actor)
    .bind(action)
    .bind(project_id)
    .execute(&state.db)
    .await;
}

pub async fn emit_project_webhook(
    state: &AppState,
    policy: &ProjectPolicy,
    event: &str,
    payload: serde_json::Value,
) {
    let Some(url) = policy.webhook_url.clone() else {
        return;
    };
    if !policy.webhook_events.is_empty() && !policy.webhook_events.iter().any(|e| e == event) {
        return;
    }
    let client = state.http_client.clone();
    let project_id = policy.project_id.clone();
    let event = event.to_string();
    tokio::spawn(async move {
        let body = serde_json::json!({
            "event": event,
            "project_id": project_id,
            "payload": payload,
        });
        if let Err(error) = client.post(url).json(&body).send().await {
            tracing::warn!(project_id = %project_id, %error, "Project webhook delivery failed");
        }
    });
}

pub fn encode_diagnostic_bundle(value: &serde_json::Value) -> String {
    base64::engine::general_purpose::STANDARD.encode(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_key_hash_is_stable_and_not_plaintext() {
        let key = "tsproj_abc";
        let first = hash_project_api_key(key);
        let second = hash_project_api_key(key);
        assert_eq!(first, second);
        assert_ne!(first, key);
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn generated_project_key_has_safe_prefix() {
        let key = generate_project_api_key();
        assert!(key.starts_with("tsproj_"));
        assert_eq!(key_prefix(&key).len(), 18);
    }
}
