use crate::app::state::AppState;
use crate::providers::registry::ProviderRegistry;
use crate::providers::traits::EndpointKind;
use crate::router::policy::RoutePolicy;

/// A single entry in the attempt plan.
#[derive(Debug, Clone)]
pub struct RouteAttempt {
    pub provider_id: String,
    pub model_id: String,
    pub priority: i32,
}

/// Build an ordered list of provider-model attempts based on the routing policy.
/// Filters by endpoint capability, enablement, health hints, and circuit breaker state.
pub async fn build_attempt_plan(
    policy: &RoutePolicy,
    registry: &ProviderRegistry,
    model: &str,
    endpoint_kind: EndpointKind,
) -> Vec<RouteAttempt> {
    // Get provider health and breaker states for filtering
    // Note: can't access AppState from here, so filtering is done in engine.rs
    let mut plan: Vec<RouteAttempt> = Vec::new();

    for provider_id in &policy.provider_order {
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
            model_id: model.to_string(),
            priority: plan.len() as i32,
        });
    }

    plan
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
                crate::resilience::health::HealthState::RateLimited => {
                    tracing::info!(provider = %pid, "Filtered out: rate limited");
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
        .map(|row| row.0)
        .unwrap_or(false);

        if enabled {
            filtered.push(attempt);
        } else {
            tracing::info!(
                provider = %attempt.provider_id,
                model = %attempt.model_id,
                "Filtered out: model disabled"
            );
        }
    }

    filtered
}

/// Reorder eligible attempts for agentic/tool-call requests.
///
/// Normal chat keeps the operator/model-group order. Tool-bearing requests need
/// a little more help because a model can answer with plain text like "I'll
/// inspect that" instead of emitting a real tool call. This pass keeps only the
/// already-eligible attempts, then prefers catalog entries marked tool-capable
/// and providers with stronger observed tool-call behavior.
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
            .then_with(|| right_score.provider_rank.cmp(&left_score.provider_rank))
            .then_with(|| left_attempt.priority.cmp(&right_attempt.priority))
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
