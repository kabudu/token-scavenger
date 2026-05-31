use crate::app::state::AppState;
use crate::app::state::ContextFailureHint;
use crate::config::schema::RoutingObjective;
use crate::providers::registry::ProviderRegistry;
use crate::providers::traits::EndpointKind;
use crate::router::model_groups::ModelTarget;
use crate::router::policy::RoutePolicy;
use crate::usage::pricing_catalog::{PricingUsage, calculate_cost, lookup_rate};

/// A single entry in the attempt plan.
#[derive(Debug, Clone)]
pub struct RouteAttempt {
    pub provider_id: String,
    pub model_id: String,
    pub priority: i32,
}

impl RouteAttempt {
    pub fn label(&self) -> String {
        format!("{}/{}", self.provider_id, self.model_id)
    }
}

/// Build an ordered list of provider-model attempts based on the routing policy.
/// Filters by endpoint capability, enablement, health hints, and circuit breaker state.
pub async fn build_attempt_plan(
    policy: &RoutePolicy,
    registry: &ProviderRegistry,
    model: &str,
    endpoint_kind: EndpointKind,
) -> Vec<RouteAttempt> {
    build_attempt_plan_for_target(
        policy,
        registry,
        &ModelTarget::any_provider(model),
        endpoint_kind,
    )
    .await
}

/// Build an ordered list of provider-model attempts for a normalized target.
pub async fn build_attempt_plan_for_target(
    policy: &RoutePolicy,
    registry: &ProviderRegistry,
    target: &ModelTarget,
    endpoint_kind: EndpointKind,
) -> Vec<RouteAttempt> {
    // Get provider health and breaker states for filtering
    // Note: can't access AppState from here, so filtering is done in engine.rs
    let mut plan: Vec<RouteAttempt> = Vec::new();
    let provider_ids = match &target.provider_id {
        Some(provider_id) => vec![provider_id.clone()],
        None => policy.provider_order.clone(),
    };

    for provider_id in &provider_ids {
        let adapter = match registry.get(provider_id).await {
            Some(a) => a,
            None => continue,
        };

        if !adapter.supports_endpoint(&endpoint_kind) {
            tracing::debug!(provider = %provider_id, "Skipped: endpoint not supported");
            continue;
        }

        plan.push(RouteAttempt {
            provider_id: provider_id.clone(),
            model_id: target.model_id.clone(),
            priority: plan.len() as i32,
        });
    }

    plan
}

/// Normalize priorities after several model-group targets are expanded.
pub fn assign_attempt_priorities(plan: &mut [RouteAttempt]) {
    for (priority, attempt) in plan.iter_mut().enumerate() {
        attempt.priority = priority as i32;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TokenEstimate {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct CandidateScore {
    pub total: f64,
    pub cost_score: f64,
    pub latency_score: f64,
    pub reliability_score: f64,
    pub quality_score: f64,
    pub context_score: f64,
    pub operator_score: f64,
    pub estimated_cost_usd: Option<f64>,
    pub cost_confidence: String,
    pub observed_latency_ms: Option<i64>,
    pub recent_failure_rate: f64,
}

#[derive(Debug, Clone)]
pub struct RoutePlanExplanation {
    pub attempt: RouteAttempt,
    pub objective: RoutingObjective,
    pub included: bool,
    pub reasons: Vec<String>,
    pub score: CandidateScore,
}

/// Apply policy objectives, hard budgets, and scoring to an already capability-
/// and health-filtered attempt plan.
pub async fn apply_policy_engine(
    plan: Vec<RouteAttempt>,
    state: &AppState,
    policy: &RoutePolicy,
    requested_model: &str,
    endpoint_kind: EndpointKind,
    token_estimate: TokenEstimate,
) -> Vec<RouteAttempt> {
    explain_policy_plan(
        plan,
        state,
        policy,
        requested_model,
        endpoint_kind,
        token_estimate,
    )
    .await
    .into_iter()
    .filter(|entry| entry.included)
    .map(|entry| entry.attempt)
    .collect()
}

/// Build scored route-plan explanations. Attempts returned by this function
/// are sorted in the same order the router will use after policy scoring.
pub async fn explain_policy_plan(
    plan: Vec<RouteAttempt>,
    state: &AppState,
    policy: &RoutePolicy,
    requested_model: &str,
    endpoint_kind: EndpointKind,
    token_estimate: TokenEstimate,
) -> Vec<RoutePlanExplanation> {
    let objective = policy.objective_for_model_group(requested_model);
    let total_attempts = plan.len().max(1);
    let mut explanations = Vec::with_capacity(plan.len());

    for attempt in plan {
        let score = score_candidate(
            &attempt,
            state,
            objective,
            endpoint_kind,
            token_estimate,
            total_attempts,
        )
        .await;
        let mut included = true;
        let mut reasons = Vec::new();

        if objective == RoutingObjective::LocalOnly && !is_local_attempt(state, &attempt) {
            included = false;
            reasons.push("filtered by local_only objective".to_string());
        }

        let budget_reasons = budget_skip_reasons(
            state,
            policy,
            requested_model,
            &attempt,
            score.estimated_cost_usd,
        )
        .await;
        if !budget_reasons.is_empty() {
            included = false;
            reasons.extend(budget_reasons);
        }

        if included {
            reasons.push("eligible".to_string());
        }

        explanations.push(RoutePlanExplanation {
            attempt,
            objective,
            included,
            reasons,
            score,
        });
    }

    explanations.sort_by(|left, right| {
        right
            .included
            .cmp(&left.included)
            .then_with(|| {
                right
                    .score
                    .total
                    .partial_cmp(&left.score.total)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.attempt.priority.cmp(&right.attempt.priority))
    });

    for (priority, entry) in explanations.iter_mut().enumerate() {
        entry.attempt.priority = priority as i32;
    }

    explanations
}

async fn score_candidate(
    attempt: &RouteAttempt,
    state: &AppState,
    objective: RoutingObjective,
    endpoint_kind: EndpointKind,
    token_estimate: TokenEstimate,
    total_attempts: usize,
) -> CandidateScore {
    let (estimated_cost_usd, cost_confidence) =
        estimate_attempt_cost(state, attempt, token_estimate).await;
    let observed_latency_ms = observed_latency_ms(state, attempt).await;
    let recent_failure_rate = recent_failure_rate(state, attempt).await;
    let (quality_score, context_score) =
        quality_and_context_score(state, attempt, endpoint_kind).await;
    let cost_score = match estimated_cost_usd {
        Some(cost) => 1.0 / (1.0 + (cost * 1000.0)),
        None => 0.0,
    };
    let latency_score = observed_latency_ms
        .map(|latency| 1.0 / (1.0 + latency.max(0) as f64 / 1000.0))
        .unwrap_or(0.5);
    let reliability_score = (1.0 - recent_failure_rate).clamp(0.0, 1.0);
    let operator_score = if total_attempts <= 1 {
        1.0
    } else {
        1.0 - (attempt.priority.max(0) as f64 / (total_attempts - 1) as f64)
    }
    .clamp(0.0, 1.0);

    let total = match objective {
        RoutingObjective::MinCost => {
            cost_score * 0.55
                + reliability_score * 0.20
                + operator_score * 0.15
                + latency_score * 0.10
        }
        RoutingObjective::MinLatency => {
            latency_score * 0.50
                + reliability_score * 0.25
                + operator_score * 0.15
                + cost_score * 0.10
        }
        RoutingObjective::Balanced => {
            cost_score * 0.25
                + latency_score * 0.25
                + reliability_score * 0.25
                + quality_score * 0.15
                + context_score * 0.05
                + operator_score * 0.05
        }
        RoutingObjective::QualityFirst => {
            quality_score * 0.45
                + reliability_score * 0.25
                + context_score * 0.10
                + latency_score * 0.15
                + operator_score * 0.05
                + cost_score * 0.05
        }
        RoutingObjective::LocalOnly => {
            reliability_score * 0.30
                + latency_score * 0.25
                + quality_score * 0.20
                + context_score * 0.10
                + cost_score * 0.15
        }
    };

    CandidateScore {
        total,
        cost_score,
        latency_score,
        reliability_score,
        quality_score,
        context_score,
        operator_score,
        estimated_cost_usd,
        cost_confidence,
        observed_latency_ms,
        recent_failure_rate,
    }
}

async fn estimate_attempt_cost(
    state: &AppState,
    attempt: &RouteAttempt,
    token_estimate: TokenEstimate,
) -> (Option<f64>, String) {
    let config = state.config();
    let free_only = config
        .providers
        .iter()
        .find(|provider| provider.id == attempt.provider_id)
        .map(|provider| provider.free_only)
        .unwrap_or(true);
    if free_only {
        return (Some(0.0), "free_tier".to_string());
    }

    let usage = PricingUsage {
        input_tokens: token_estimate.input_tokens,
        output_tokens: token_estimate.output_tokens,
        ..Default::default()
    };
    match lookup_rate(&state.db, &attempt.provider_id, &attempt.model_id).await {
        Ok(Some(rate)) => {
            let estimate = calculate_cost(&rate, &usage);
            (Some(estimate.amount_usd), estimate.confidence)
        }
        Ok(None) => (None, "unknown_price".to_string()),
        Err(error) => {
            tracing::warn!(
                provider = %attempt.provider_id,
                model = %attempt.model_id,
                %error,
                "Failed to estimate policy candidate cost"
            );
            (None, "pricing_lookup_error".to_string())
        }
    }
}

async fn observed_latency_ms(state: &AppState, attempt: &RouteAttempt) -> Option<i64> {
    sqlx::query_as::<_, (Option<i64>,)>(
        "SELECT CAST(AVG(latency_ms) AS INTEGER)
         FROM request_log
         WHERE selected_provider_id = ? AND selected_model_id = ? AND status = 'success'
           AND latency_ms IS NOT NULL AND received_at >= datetime('now', '-1 day')",
    )
    .bind(&attempt.provider_id)
    .bind(&attempt.model_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()
    .and_then(|row| row.0)
}

async fn recent_failure_rate(state: &AppState, attempt: &RouteAttempt) -> f64 {
    let Some((failures, total)) = sqlx::query_as::<_, (i64, i64)>(
        "SELECT
            COALESCE(SUM(CASE WHEN status != 'success' THEN 1 ELSE 0 END), 0),
            COUNT(*)
         FROM request_log
         WHERE selected_provider_id = ? AND selected_model_id = ?
           AND received_at >= datetime('now', '-1 day')",
    )
    .bind(&attempt.provider_id)
    .bind(&attempt.model_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten() else {
        return 0.0;
    };

    if total == 0 {
        0.0
    } else {
        (failures as f64 / total as f64).clamp(0.0, 1.0)
    }
}

async fn quality_and_context_score(
    state: &AppState,
    attempt: &RouteAttempt,
    endpoint_kind: EndpointKind,
) -> (f64, f64) {
    let caps = sqlx::query_as::<_, (bool, bool, bool, i64, Option<String>)>(
        "SELECT supports_tools, supports_json_mode, supports_vision, priority, metadata_json
         FROM models WHERE provider_id = ? AND upstream_model_id = ?",
    )
    .bind(&attempt.provider_id)
    .bind(&attempt.model_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()
    .unwrap_or((true, true, false, 100, None));

    let provider_quality =
        (tool_reliability_rank(&attempt.provider_id) as f64 / 100.0).clamp(0.0, 1.0);
    let capability_score = match endpoint_kind {
        EndpointKind::ChatCompletions => {
            (if caps.0 { 0.35 } else { 0.0 })
                + (if caps.1 { 0.25 } else { 0.0 })
                + (if caps.2 { 0.10 } else { 0.0 })
                + 0.30
        }
        EndpointKind::Embeddings => 0.65,
        EndpointKind::ModelList => 0.50,
    };
    let model_priority_score = 1.0 / (1.0 + caps.3.max(0) as f64 / 100.0);

    let context_score = context_score(caps.4.as_deref());

    (
        (provider_quality * 0.40
            + capability_score * 0.30
            + model_priority_score * 0.15
            + context_score * 0.15)
            .clamp(0.0, 1.0),
        context_score,
    )
}

fn context_score(metadata_json: Option<&str>) -> f64 {
    let Some(metadata_json) = metadata_json else {
        return 0.5;
    };
    let Some(context_window) = serde_json::from_str::<serde_json::Value>(metadata_json)
        .ok()
        .and_then(|metadata| {
            metadata
                .get("context_window")
                .and_then(|value| value.as_u64())
        })
    else {
        return 0.5;
    };
    if context_window == 0 {
        return 0.0;
    }
    ((context_window as f64).log10() / (2_000_000_f64).log10()).clamp(0.0, 1.0)
}

async fn budget_skip_reasons(
    state: &AppState,
    policy: &RoutePolicy,
    requested_model: &str,
    attempt: &RouteAttempt,
    estimated_cost_usd: Option<f64>,
) -> Vec<String> {
    if !has_hard_budget(policy, requested_model, &attempt.provider_id) {
        return Vec::new();
    }

    let Some(estimated_cost_usd) = estimated_cost_usd else {
        return vec!["filtered by hard budget because paid price is unknown".to_string()];
    };

    let mut reasons = Vec::new();
    if let Some(limit) = policy.budgets.max_cost_per_request_usd {
        if estimated_cost_usd > limit {
            reasons.push(format!(
                "filtered by per-request budget: estimate {:.6} > limit {:.6}",
                estimated_cost_usd, limit
            ));
        }
    }

    if let Some(limit) = policy.budgets.max_cost_per_day_usd {
        let spent = spent_today(state, None, None).await;
        if spent + estimated_cost_usd > limit {
            reasons.push(format!(
                "filtered by daily budget: projected {:.6} > limit {:.6}",
                spent + estimated_cost_usd,
                limit
            ));
        }
    }

    if let Some(limit) = policy
        .budgets
        .max_cost_per_provider_per_day_usd
        .get(&attempt.provider_id)
    {
        let spent = spent_today(state, Some(&attempt.provider_id), None).await;
        if spent + estimated_cost_usd > *limit {
            reasons.push(format!(
                "filtered by provider daily budget: projected {:.6} > limit {:.6}",
                spent + estimated_cost_usd,
                limit
            ));
        }
    }

    if let Some(limit) = policy
        .budgets
        .max_cost_per_model_group_per_day_usd
        .get(requested_model)
    {
        let spent = spent_today(state, None, Some(requested_model)).await;
        if spent + estimated_cost_usd > *limit {
            reasons.push(format!(
                "filtered by model-group daily budget: projected {:.6} > limit {:.6}",
                spent + estimated_cost_usd,
                limit
            ));
        }
    }

    reasons
}

fn has_hard_budget(policy: &RoutePolicy, requested_model: &str, provider_id: &str) -> bool {
    policy.budgets.max_cost_per_request_usd.is_some()
        || policy.budgets.max_cost_per_day_usd.is_some()
        || policy
            .budgets
            .max_cost_per_provider_per_day_usd
            .contains_key(provider_id)
        || policy
            .budgets
            .max_cost_per_model_group_per_day_usd
            .contains_key(requested_model)
}

async fn spent_today(
    state: &AppState,
    provider_id: Option<&str>,
    requested_model: Option<&str>,
) -> f64 {
    let mut query = String::from(
        "SELECT COALESCE(SUM(usage_events.estimated_cost_usd), 0.0)
         FROM usage_events
         JOIN request_log USING (request_id)
         WHERE usage_events.timestamp >= date('now')",
    );
    if provider_id.is_some() {
        query.push_str(" AND usage_events.provider_id = ?");
    }
    if requested_model.is_some() {
        query.push_str(" AND request_log.requested_model = ?");
    }

    let mut sql = sqlx::query_as::<_, (f64,)>(&query);
    if let Some(provider_id) = provider_id {
        sql = sql.bind(provider_id);
    }
    if let Some(requested_model) = requested_model {
        sql = sql.bind(requested_model);
    }

    sql.fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .map(|row| row.0)
        .unwrap_or(0.0)
}

fn is_local_attempt(state: &AppState, attempt: &RouteAttempt) -> bool {
    if matches!(
        attempt.provider_id.as_str(),
        "local" | "ollama" | "llama-cpp"
    ) {
        return true;
    }
    let config = state.config();
    let Some(base_url) = config
        .providers
        .iter()
        .find(|provider| provider.id == attempt.provider_id)
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

fn route_hint_key(provider_id: &str, model_id: &str) -> String {
    format!("{provider_id}\0{model_id}")
}

pub fn should_skip_for_context_hint(
    state: &AppState,
    attempt: &RouteAttempt,
    prompt_size_hint: usize,
) -> bool {
    let key = route_hint_key(&attempt.provider_id, &attempt.model_id);
    let now = chrono::Utc::now().timestamp();
    let Some(hint) = state.context_failure_hints.get(&key) else {
        return false;
    };
    if hint.expires_at <= now {
        drop(hint);
        state.context_failure_hints.remove(&key);
        return false;
    }
    if prompt_size_hint >= hint.prompt_size_hint {
        tracing::info!(
            provider = %attempt.provider_id,
            model = %attempt.model_id,
            prompt_size_hint,
            failed_prompt_size_hint = hint.prompt_size_hint,
            "Skipping: recent context budget failure for equal-or-larger prompt"
        );
        return true;
    }
    false
}

pub fn record_context_failure_hint(
    state: &AppState,
    provider_id: &str,
    model_id: &str,
    prompt_size_hint: usize,
) {
    let ttl_seconds = 30 * 60;
    state.context_failure_hints.insert(
        route_hint_key(provider_id, model_id),
        ContextFailureHint {
            prompt_size_hint,
            expires_at: chrono::Utc::now().timestamp() + ttl_seconds,
        },
    );
    tracing::info!(
        provider = %provider_id,
        model = %model_id,
        prompt_size_hint,
        ttl_seconds,
        "Recorded context budget failure hint"
    );
}

pub fn should_skip_for_stream_silence_hint(
    state: &AppState,
    attempt: &RouteAttempt,
    prompt_size_hint: usize,
) -> bool {
    let key = route_hint_key(&attempt.provider_id, &attempt.model_id);
    let now = chrono::Utc::now().timestamp();
    let Some(hint) = state.stream_silence_hints.get(&key) else {
        return false;
    };
    if hint.expires_at <= now {
        drop(hint);
        state.stream_silence_hints.remove(&key);
        return false;
    }
    if prompt_size_hint >= hint.prompt_size_hint {
        tracing::info!(
            provider = %attempt.provider_id,
            model = %attempt.model_id,
            prompt_size_hint,
            failed_prompt_size_hint = hint.prompt_size_hint,
            "Skipping: recent streaming silence for equal-or-larger prompt"
        );
        return true;
    }
    false
}

pub fn record_stream_silence_hint(
    state: &AppState,
    provider_id: &str,
    model_id: &str,
    prompt_size_hint: usize,
) {
    let ttl_seconds = 10 * 60;
    state.stream_silence_hints.insert(
        route_hint_key(provider_id, model_id),
        ContextFailureHint {
            prompt_size_hint,
            expires_at: chrono::Utc::now().timestamp() + ttl_seconds,
        },
    );
    tracing::info!(
        provider = %provider_id,
        model = %model_id,
        prompt_size_hint,
        ttl_seconds,
        "Recorded streaming silence route hint"
    );
}

pub fn should_skip_for_rate_limit_hint(state: &AppState, attempt: &RouteAttempt) -> bool {
    let key = route_hint_key(&attempt.provider_id, &attempt.model_id);
    let now = chrono::Utc::now().timestamp();
    let Some(hint) = state.route_rate_limit_hints.get(&key) else {
        return false;
    };
    if hint.expires_at <= now {
        drop(hint);
        state.route_rate_limit_hints.remove(&key);
        return false;
    }

    tracing::info!(
        provider = %attempt.provider_id,
        model = %attempt.model_id,
        ttl_remaining_seconds = hint.expires_at - now,
        "Skipping: recent provider/model rate limit"
    );
    true
}

pub fn record_rate_limit_hint(
    state: &AppState,
    provider_id: &str,
    model_id: &str,
    retry_after: Option<u64>,
) {
    let ttl_seconds = retry_after.unwrap_or(60).clamp(5, 300);
    state.route_rate_limit_hints.insert(
        route_hint_key(provider_id, model_id),
        ContextFailureHint {
            prompt_size_hint: 0,
            expires_at: chrono::Utc::now().timestamp() + ttl_seconds as i64,
        },
    );
    tracing::info!(
        provider = %provider_id,
        model = %model_id,
        retry_after_seconds = retry_after,
        ttl_seconds,
        "Recorded provider/model rate-limit route hint"
    );
}

/// Filter an attempt plan by health state and circuit breaker status.
/// Returns a filtered plan with unhealthy/blocked providers removed.
pub fn filter_by_health(plan: Vec<RouteAttempt>, state: &AppState) -> Vec<RouteAttempt> {
    plan.into_iter().filter(|attempt| {
        let pid = &attempt.provider_id;

        // Check circuit breaker state
        if let Some(breaker) = state.breaker_states.get(pid) {
            if breaker.is_open() {
                tracing::info!(provider = %pid, "Filtered out: circuit breaker open");
                return false;
            }
        }

        // Check health state
        if let Some(health) = state.health_states.get(pid) {
            let hs = health.value();
            match hs.state {
                crate::resilience::health::HealthState::Disabled
                | crate::resilience::health::HealthState::Unhealthy => {
                    tracing::info!(provider = %pid, "Filtered out: health state = {:?}", hs.state);
                    return false;
                }
                crate::resilience::health::HealthState::QuotaExhausted if policy_is_free_first(state) => {
                    tracing::info!(provider = %pid, "Filtered out: quota exhausted (free-first mode)");
                    return false;
                }
                _ => {} // Healthy, Degraded: allow
            }
        }

        true
    }).collect()
}

/// Filter paid providers unless routing policy explicitly allows paid fallback.
pub fn filter_by_paid_policy(plan: Vec<RouteAttempt>, state: &AppState) -> Vec<RouteAttempt> {
    let config = state.config();
    if config.routing.allow_paid_fallback {
        return plan;
    }

    plan.into_iter()
        .filter(|attempt| {
            let is_free_only = config
                .providers
                .iter()
                .find(|provider| provider.id == attempt.provider_id)
                .map(|provider| provider.free_only)
                .unwrap_or(true);

            if !is_free_only {
                tracing::info!(
                    provider = %attempt.provider_id,
                    model = %attempt.model_id,
                    "Filtered out: paid fallback disabled"
                );
            }

            is_free_only
        })
        .collect()
}

/// Filter an attempt plan by persisted model enablement.
/// Only models with an explicit row in the models table are routable.
/// Missing rows (model never discovered/seeded for this provider) are excluded.
pub async fn filter_by_model_enabled(
    plan: Vec<RouteAttempt>,
    state: &AppState,
) -> Vec<RouteAttempt> {
    let mut filtered = Vec::with_capacity(plan.len());

    for attempt in plan {
        let enabled = sqlx::query_as::<_, (bool,)>(
            "SELECT enabled FROM models WHERE provider_id = ? AND upstream_model_id = ?",
        )
        .bind(&attempt.provider_id)
        .bind(&attempt.model_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .map(|row| row.0);

        match enabled {
            Some(true) => filtered.push(attempt),
            Some(false) => {
                tracing::info!(
                    provider = %attempt.provider_id,
                    model = %attempt.model_id,
                    "Filtered out: model disabled"
                );
            }
            None => {
                tracing::debug!(
                    provider = %attempt.provider_id,
                    model = %attempt.model_id,
                    "Filtered out: model not in provider catalog"
                );
            }
        }
    }

    filtered
}

/// Reorder eligible attempts for agentic/tool-call requests.
///
/// Normal chat keeps the operator/model-group order. Tool-bearing requests only
/// move catalog entries that are explicitly marked as not tool-capable behind
/// entries that can handle tools. Among tool-capable attempts, preserve the
/// operator's order exactly.
pub async fn prioritize_for_tool_use(
    mut plan: Vec<RouteAttempt>,
    state: &AppState,
) -> Vec<RouteAttempt> {
    if plan.len() <= 1 {
        return plan;
    }

    let mut scored = Vec::with_capacity(plan.len());
    for (original_index, attempt) in plan.drain(..).enumerate() {
        let supports_tools = sqlx::query_as::<_, (bool,)>(
            "SELECT supports_tools FROM models WHERE provider_id = ? AND upstream_model_id = ?",
        )
        .bind(&attempt.provider_id)
        .bind(&attempt.model_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .map(|row| row.0)
        .unwrap_or(true);

        let provider_rank = tool_reliability_rank(&attempt.provider_id);
        scored.push((
            attempt,
            ToolAttemptScore {
                supports_tools,
                provider_rank,
                original_index,
            },
        ));
    }

    let before = scored
        .iter()
        .map(|(attempt, _)| (attempt.provider_id.clone(), attempt.model_id.clone()))
        .collect::<Vec<_>>();

    scored.sort_by(|(left_attempt, left_score), (right_attempt, right_score)| {
        right_score
            .supports_tools
            .cmp(&left_score.supports_tools)
            .then_with(|| left_attempt.priority.cmp(&right_attempt.priority))
            .then_with(|| right_score.provider_rank.cmp(&left_score.provider_rank))
            .then_with(|| left_score.original_index.cmp(&right_score.original_index))
    });

    let after = scored
        .iter()
        .map(|(attempt, _)| (attempt.provider_id.clone(), attempt.model_id.clone()))
        .collect::<Vec<_>>();
    if before != after {
        tracing::info!(
            before = ?before,
            after = ?after,
            "Tool request route plan reprioritized"
        );
    }

    scored.into_iter().map(|(attempt, _)| attempt).collect()
}

#[derive(Debug)]
struct ToolAttemptScore {
    supports_tools: bool,
    provider_rank: i32,
    original_index: usize,
}

fn tool_reliability_rank(provider_id: &str) -> i32 {
    match provider_id {
        // Strong OpenAI-compatible tool-call behavior in Hermes-style testing.
        "groq" => 100,
        // Native tool support with provider-specific translation.
        "google" => 90,
        // OpenAI-compatible providers commonly used for agentic workflows.
        "openrouter" | "github-models" => 80,
        "deepseek" | "xai" | "cerebras" | "nvidia" => 70,
        // Supports tools, but observed to sometimes produce prose instead of
        // tool calls in agent turn-taking.
        "mistral" => 40,
        // Unknown providers are allowed, just not preferred.
        _ => 50,
    }
}

fn policy_is_free_first(state: &AppState) -> bool {
    state.config().routing.free_first
}
