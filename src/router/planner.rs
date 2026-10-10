//! Shared chat plan construction for execution, streaming, and preview.
//!
//! Preview does not take leases, call a classifier, reserve budget, or update
//! session expiry. Execution owns an affinity lease when the caller supplied a
//! session and affinity is enabled.

use crate::api::error::ApiError;
use crate::api::openai::chat::NormalizedChatRequest;
use crate::app::state::AppState;
use crate::config::schema::{AffinityMode, AgentRoutingMode, AgentTier, ClassifierScope};
use crate::discovery::model_intelligence::{
    ModelRequestRequirements, filter_by_model_intelligence,
};
use crate::projects::PrivacyProfile;
use crate::providers::traits::EndpointKind;
use crate::router::affinity::{AffinityHintAlias, AffinityLease, PinOutcome, effective_affinity};
use crate::router::classifier::{self, ClassificationOutcome};
use crate::router::context::{RoutingContext, TierHint};
use crate::router::continuation::{self, ContinuationSupport};
use crate::router::policy::RoutePolicy;
use crate::router::selection::{
    RouteAttempt, TokenEstimate, apply_policy_engine, assign_attempt_priorities,
    build_attempt_plan_for_target, filter_by_health, filter_by_model_enabled_for_endpoint,
    filter_by_paid_policy, prioritize_for_tool_use,
};
use crate::router::task_policy::{self, Phase, TierDecision, TierSource};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanMode {
    Execute,
    Preview,
}

#[derive(Debug, Clone)]
pub struct AgentDecision {
    pub adaptive: bool,
    pub profile_id: String,
    pub revision: String,
    pub phase: String,
    pub phase_evidence: String,
    pub tier: String,
    pub source: String,
    pub rule_index: Option<usize>,
    pub classifier_status: String,
    pub classifier_cache: String,
    pub classifier_latency_ms: u64,
    pub pin: String,
    pub affinity: String,
    pub session_digest: Option<String>,
    pub subtask_digest: Option<String>,
    pub task_type: Option<String>,
    pub classification_required: bool,
    pub simulated: bool,
    pub lock_candidate: bool,
    pub tier_value: AgentTier,
    pub continuation: ContinuationSupport,
}

pub struct PreparedChatPlan {
    pub attempts: Vec<RouteAttempt>,
    pub resolved_models: Vec<String>,
    pub decision: Option<AgentDecision>,
    pub lease: Option<AffinityLease>,
    pub token_estimate: TokenEstimate,
}

pub async fn prepare_chat_plan(
    state: &AppState,
    request: &NormalizedChatRequest,
    context: &RoutingContext,
    mode: PlanMode,
) -> Result<PreparedChatPlan, ApiError> {
    let token_estimate = chat_token_estimate(request);
    let config = state.config();
    let profile_name = request.model.clone();
    let profile = config
        .routing
        .agent
        .enabled
        .then(|| config.routing.agent.profiles.get(&profile_name).cloned())
        .flatten();

    let Some(profile) = profile else {
        let (attempts, resolved_models) = standard_attempts(
            state,
            request,
            &context.public_request_id,
            &request.model,
            &request.model,
            None,
        )
        .await?;
        return Ok(PreparedChatPlan {
            attempts,
            resolved_models,
            decision: None,
            lease: None,
            token_estimate,
        });
    };

    if mode == PlanMode::Execute {
        let _ = crate::usage::accounting::ensure_pending_request(
            state,
            &context.public_request_id,
            &request.model,
            request.stream,
        )
        .await;
    }
    let revision = task_policy::policy_revision(
        &serde_json::to_string(&config.routing.agent).unwrap_or_default(),
    );
    authorize_profile_alias(state, context, &profile_name).await?;
    let report = task_policy::detect_phase(request, context.hints.phase);
    if report.phase == Phase::Ambiguous
        && context.hints.affinity == Some(crate::router::context::AffinityHint::Required)
    {
        return Err(agent_error(
            409,
            "ambiguous_continuation",
            "tool history is partial, duplicated, or orphaned and cannot be continued safely",
        ));
    }

    let mut affinity = effective_affinity(
        profile.affinity,
        context.hints.affinity.map(|hint| match hint {
            crate::router::context::AffinityHint::Off => AffinityHintAlias::Off,
            crate::router::context::AffinityHint::Prefer => AffinityHintAlias::Prefer,
            crate::router::context::AffinityHint::Required => AffinityHintAlias::Required,
        }),
        false,
    );
    if affinity == AffinityMode::Required && context.hints.session.is_none() {
        return Err(agent_error(
            400,
            "session_required",
            "required affinity needs an x-ts-session identifier",
        ));
    }

    let mut lease = None;
    let mut pin_outcome = PinOutcome::Miss;
    let mut pin = None;
    if context.hints.session.is_some() && affinity != AffinityMode::Off {
        let key = state.affinity.scope_key(
            &context.project.principal_id,
            &context.project.project_id,
            context.hints.session.as_deref().unwrap_or(""),
            context.hints.subtask.as_deref(),
            &profile_name,
        );
        if mode == PlanMode::Execute {
            lease = Some(state.affinity.try_admit(
                key.clone(),
                &context.project.project_id,
                config.routing.agent.max_sessions,
                config.routing.agent.max_sessions_per_project,
                Duration::from_secs(config.routing.agent.session_idle_ttl_seconds),
                Duration::from_secs(config.routing.agent.session_max_lifetime_seconds),
            )?);
        }
        pin = state.affinity.snapshot(&key);
        pin_outcome = if pin.is_some() {
            PinOutcome::Hit
        } else {
            PinOutcome::Miss
        };
    }
    if pin
        .as_ref()
        .is_some_and(|previous| previous.policy_revision != revision)
    {
        pin = None;
        pin_outcome = PinOutcome::Bypassed {
            reason: "pin_policy_revision",
        };
    }
    if affinity != AffinityMode::Off
        && report.continuation
        && pin
            .as_ref()
            .is_some_and(|pin| !continuation::is_replayable(&pin.provider_id))
    {
        affinity = AffinityMode::Required;
    }

    if report.continuation && affinity == AffinityMode::Required && pin.is_none() {
        return Err(agent_error(
            409,
            "session_state_unavailable",
            "required continuation has no active session state; restart the subtask",
        ));
    }
    if report.continuation
        && affinity == AffinityMode::Required
        && pin.as_ref().is_some_and(|pin| pin.incomplete)
    {
        return Err(agent_error(
            409,
            "session_state_unavailable",
            "the previous streamed continuation did not finish; resend a complete tool history",
        ));
    }

    let continuation_tier =
        (report.continuation && pin.is_some()).then(|| pin.as_ref().unwrap().tier);
    let simulated = matches!(mode, PlanMode::Preview)
        .then_some(context.simulated_tier)
        .flatten();
    let simulated_requested = simulated.is_some();
    let mut decision_tier = task_policy::select_tier_without_classifier(
        &profile,
        &config.routing.agent.rules,
        &profile_name,
        &report,
        context.hints.task_type.as_deref(),
        context.hints.tier,
        continuation_tier,
        simulated,
    );
    let mut classification = ClassificationOutcome {
        tier: None,
        source: TierSource::ClassifierSkipped,
        cache: "miss",
        latency_ms: 0,
        status: "classifier_disabled",
    };
    let boundary = match config.routing.agent.classifier.scope {
        ClassifierScope::SubtaskBoundary => pin.is_none() && !report.continuation,
        ClassifierScope::PerRequest => !report.continuation,
    };
    let unresolved = !task_policy::rule_would_resolve(
        &config.routing.agent.rules,
        &profile_name,
        &report,
        context.hints.task_type.as_deref(),
        context.hints.tier,
        continuation_tier,
    );
    let classification_required = unresolved
        && config.routing.agent.classifier.enabled
        && matches!(
            config.routing.agent.mode,
            AgentRoutingMode::Adaptive | AgentRoutingMode::Shadow
        )
        && boundary;
    if mode == PlanMode::Preview
        && classification_required
        && decision_tier.source == TierSource::Default
    {
        classification.status = "classification_required";
    } else if mode == PlanMode::Execute
        && unresolved
        && boundary
        && config.routing.agent.classifier.enabled
        && matches!(
            config.routing.agent.mode,
            AgentRoutingMode::Adaptive | AgentRoutingMode::Shadow
        )
        && classifier_allowed(state, context).await
    {
        classification = classifier::classify(
            state,
            &config.routing.agent.classifier,
            &context.project.project_id,
            &context.project.principal_id,
            &revision,
            &report,
            context.hints.task_type.as_deref(),
            &request.messages,
            context.deadline,
            &context.public_request_id,
            &request.model,
            &context.project.api_key_prefix,
            config.routing.agent.classifier.scope,
            boundary,
        )
        .await;
        if config.routing.agent.mode == AgentRoutingMode::Adaptive {
            if let Some(tier) = classification.tier {
                decision_tier = TierDecision {
                    tier,
                    source: classification.source.clone(),
                    rule_index: None,
                    confidence: None,
                };
            } else if classification.source != TierSource::ClassifierSkipped {
                decision_tier.source = classification.source.clone();
            }
        }
    }

    let group = profile.group_for(decision_tier.tier).to_string();
    authorize_alias(state, context, &profile_name, &group).await?;
    let (mut attempts, resolved_models) = standard_attempts(
        state,
        request,
        &context.public_request_id,
        &group,
        &profile_name,
        Some(config.routing.agent.max_candidates),
    )
    .await?;
    attempts = order_attempts(state, attempts, &config);
    if affinity != AffinityMode::Off && request.tools.is_some() {
        // An initial tool call on an opaque-continuation adapter would create a
        // conversation that this proxy cannot resume on the next round.
        attempts.retain(|attempt| continuation::is_replayable(&attempt.provider_id));
    }
    let mut lock_candidate = false;
    if let Some(pin) = &pin {
        if report.continuation && !continuation::is_replayable(&pin.provider_id) {
            if affinity == AffinityMode::Required {
                return Err(agent_error(
                    400,
                    "unsupported_continuation",
                    "this provider requires opaque continuation state that is not round-tripped",
                ));
            }
            attempts.retain(|attempt| continuation::is_replayable(&attempt.provider_id));
            pin_outcome = PinOutcome::Bypassed {
                reason: "unsupported_continuation",
            };
        } else if affinity == AffinityMode::Required {
            let target = (pin.provider_id.clone(), pin.model_id.clone());
            if !attempts
                .iter()
                .any(|attempt| attempt.provider_id == target.0 && attempt.model_id == target.1)
            {
                return Err(required_pin_error(state, &target.0));
            }
            attempts
                .retain(|attempt| attempt.provider_id == target.0 && attempt.model_id == target.1);
            lock_candidate = true;
            pin_outcome = PinOutcome::Hit;
        } else if affinity == AffinityMode::Prefer {
            pin_outcome = apply_soft_pin(state, &mut attempts, &pin.provider_id, &pin.model_id);
        }
    }

    let _ = TierHint::Auto;
    Ok(PreparedChatPlan {
        attempts,
        resolved_models,
        decision: Some(AgentDecision {
            adaptive: true,
            profile_id: profile_name,
            revision,
            phase: report.phase.as_str().to_string(),
            phase_evidence: phase_evidence(&report),
            tier: decision_tier.tier.as_str().to_string(),
            source: decision_tier.source.as_str().to_string(),
            rule_index: decision_tier.rule_index,
            classifier_status: classification.status.to_string(),
            classifier_cache: classification.cache.to_string(),
            classifier_latency_ms: classification.latency_ms,
            pin: pin_outcome.as_str().to_string(),
            affinity: affinity.as_str().to_string(),
            session_digest: context
                .hints
                .session
                .as_deref()
                .map(|session| state.affinity.digest(session)),
            subtask_digest: context
                .hints
                .subtask
                .as_deref()
                .map(|subtask| state.affinity.digest(subtask)),
            task_type: context.hints.task_type.clone(),
            classification_required: mode == PlanMode::Preview && classification_required,
            simulated: simulated_requested,
            lock_candidate,
            tier_value: decision_tier.tier,
            continuation: pin
                .as_ref()
                .map(|pin| pin.continuation)
                .unwrap_or(ContinuationSupport::Replayable),
        }),
        lease,
        token_estimate,
    })
}

async fn standard_attempts(
    state: &AppState,
    request: &NormalizedChatRequest,
    request_id: &str,
    target_model: &str,
    policy_model: &str,
    candidate_limit: Option<usize>,
) -> Result<(Vec<RouteAttempt>, Vec<String>), ApiError> {
    let config = state.config();
    let policy = RoutePolicy::from_config(&config);
    let resolved_targets =
        crate::router::model_groups::resolve_model_group_targets(state, target_model)
            .await
            .unwrap_or_else(|| {
                vec![crate::router::model_groups::ModelTarget::any_provider(
                    target_model.to_string(),
                )]
            });
    let resolved_models = resolved_targets
        .iter()
        .map(|target| target.label())
        .collect::<Vec<_>>();
    let mut plan = Vec::new();
    for target in &resolved_targets {
        plan.extend(
            build_attempt_plan_for_target(
                &policy,
                &state.provider_registry,
                target,
                EndpointKind::ChatCompletions,
            )
            .await,
        );
    }
    if let Some(limit) = candidate_limit {
        if plan.len() > limit {
            return Err(ApiError::InvalidRequest(format!(
                "adaptive profile expands to {} candidates which exceeds max_candidates",
                plan.len()
            )));
        }
    }
    assign_attempt_priorities(&mut plan);
    let mut plan = filter_by_model_enabled_for_endpoint(
        filter_by_paid_policy(filter_by_health(plan, state), state),
        state,
        EndpointKind::ChatCompletions,
    )
    .await;
    plan = filter_by_model_intelligence(plan, state, ModelRequestRequirements::for_chat(request))
        .await;
    if request.tools.is_some() {
        plan = prioritize_for_tool_use(plan, state).await;
    }
    let token_estimate = chat_token_estimate(request);
    plan = apply_policy_engine(
        plan,
        state,
        &policy,
        policy_model,
        EndpointKind::ChatCompletions,
        token_estimate,
    )
    .await;
    plan = crate::projects::filter_project_policy(
        plan,
        state,
        request_id,
        policy_model,
        token_estimate,
    )
    .await?;
    dedup_attempts(&mut plan);
    Ok((plan, resolved_models))
}

fn dedup_attempts(plan: &mut Vec<RouteAttempt>) {
    let mut seen = std::collections::HashSet::new();
    plan.retain(|attempt| seen.insert((attempt.provider_id.clone(), attempt.model_id.clone())));
}

fn order_attempts(
    state: &AppState,
    attempts: Vec<RouteAttempt>,
    config: &crate::config::schema::Config,
) -> Vec<RouteAttempt> {
    if !config.routing.free_first {
        return attempts;
    }
    let (free, paid): (Vec<_>, Vec<_>) = attempts
        .into_iter()
        .partition(|attempt| provider_is_free(state, &attempt.provider_id));
    free.into_iter().chain(paid).collect()
}

fn apply_soft_pin(
    state: &AppState,
    attempts: &mut Vec<RouteAttempt>,
    provider_id: &str,
    model_id: &str,
) -> PinOutcome {
    let Some(index) = attempts
        .iter()
        .position(|attempt| attempt.provider_id == provider_id && attempt.model_id == model_id)
    else {
        return PinOutcome::Bypassed {
            reason: "pin_capability_miss",
        };
    };
    let pinned_free = provider_is_free(state, provider_id);
    let free_available = attempts
        .iter()
        .any(|attempt| provider_is_free(state, &attempt.provider_id));
    if !pinned_free && free_available {
        return PinOutcome::Bypassed {
            reason: "pin_policy_denied",
        };
    }
    let attempt = attempts.remove(index);
    let insert_at = if pinned_free {
        0
    } else {
        attempts
            .iter()
            .position(|candidate| !provider_is_free(state, &candidate.provider_id))
            .unwrap_or(0)
    };
    attempts.insert(insert_at, attempt);
    PinOutcome::Hit
}

fn provider_is_free(state: &AppState, provider_id: &str) -> bool {
    state
        .config()
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .map(|provider| provider.free_only)
        .unwrap_or(true)
}

async fn authorize_alias(
    state: &AppState,
    context: &RoutingContext,
    alias: &str,
    group: &str,
) -> Result<(), ApiError> {
    if !context.project.enforce_policy {
        return Ok(());
    }
    let Some(policy) =
        crate::projects::load_project_policy(&state.db, &context.project.project_id).await?
    else {
        return Err(ApiError::Forbidden);
    };
    if !policy.enabled {
        return Err(ApiError::Forbidden);
    }
    if policy.allowed_model_groups.is_empty() {
        return Ok(());
    }
    let alias_allowed = policy.allowed_model_groups.iter().any(|name| name == alias);
    let group_allowed = policy.allowed_model_groups.iter().any(|name| name == group);
    if alias_allowed && group_allowed {
        return Ok(());
    }
    Err(ApiError::Forbidden)
}

async fn authorize_profile_alias(
    state: &AppState,
    context: &RoutingContext,
    alias: &str,
) -> Result<(), ApiError> {
    if !context.project.enforce_policy {
        return Ok(());
    }
    let Some(policy) =
        crate::projects::load_project_policy(&state.db, &context.project.project_id).await?
    else {
        return Err(ApiError::Forbidden);
    };
    if !policy.enabled {
        return Err(ApiError::Forbidden);
    }
    if !policy.allowed_model_groups.is_empty()
        && !policy.allowed_model_groups.iter().any(|name| name == alias)
    {
        return Err(ApiError::Forbidden);
    }
    Ok(())
}

async fn classifier_allowed(state: &AppState, context: &RoutingContext) -> bool {
    let config = state.config();
    let classifier = &config.routing.agent.classifier;
    if !classifier
        .allowed_project_ids
        .iter()
        .any(|allowed| allowed == &context.project.project_id)
    {
        return false;
    }
    let Some(provider) = config
        .providers
        .iter()
        .find(|provider| provider.id == classifier.provider_id)
    else {
        return false;
    };
    if !context.project.enforce_policy {
        return !provider_paid_blocked(&config, provider.free_only);
    }
    let Ok(Some(policy)) =
        crate::projects::load_project_policy(&state.db, &context.project.project_id).await
    else {
        return false;
    };
    if !policy.enabled {
        return false;
    }
    if matches!(policy.privacy_profile, PrivacyProfile::LocalOnly) && !provider_is_local(provider) {
        return false;
    }
    if matches!(policy.privacy_profile, PrivacyProfile::FreeOnly) && !provider.free_only {
        return false;
    }
    if !policy.provider_allowlist.is_empty()
        && !policy
            .provider_allowlist
            .iter()
            .any(|id| id == &provider.id)
    {
        return false;
    }
    if policy.provider_denylist.iter().any(|id| id == &provider.id) {
        return false;
    }
    if !provider.free_only && !policy.allow_paid_fallback {
        return false;
    }
    !provider_paid_blocked(&config, provider.free_only)
}

fn provider_paid_blocked(config: &crate::config::schema::Config, free_only: bool) -> bool {
    !free_only && !config.routing.allow_paid_fallback
}

fn provider_is_local(provider: &crate::config::schema::ProviderConfig) -> bool {
    if matches!(
        provider.id.as_str(),
        "local" | "ollama" | "llama-cpp" | "lmstudio"
    ) {
        return true;
    }
    provider
        .base_url
        .as_deref()
        .and_then(|url| reqwest::Url::parse(url).ok())
        .and_then(|url| url.host_str().map(str::to_string))
        .is_some_and(|host| matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1"))
}

fn required_pin_error(state: &AppState, provider_id: &str) -> ApiError {
    let unhealthy = state.health_states.get(provider_id).is_some_and(|health| {
        matches!(
            health.state,
            crate::resilience::health::HealthState::Unhealthy
                | crate::resilience::health::HealthState::Disabled
        )
    }) || state
        .breaker_states
        .get(provider_id)
        .is_some_and(|breaker| breaker.is_open());
    if unhealthy {
        agent_error(
            503,
            "affinity_target_unavailable",
            "the required affinity target is temporarily unavailable",
        )
    } else {
        ApiError::RouteExhausted(format!(
            "required affinity target {provider_id} is not eligible under current policy"
        ))
    }
}

fn phase_evidence(report: &task_policy::PhaseReport) -> String {
    if report.duplicate_tool_ids {
        "duplicate_tool_ids".into()
    } else if report.orphan_tool_ids {
        "orphan_tool_ids".into()
    } else if report.partial_tool_results {
        "partial_tool_results".into()
    } else if report.continuation {
        "matched_tool_results".into()
    } else if report.tools_required {
        "tools_declared".into()
    } else if report.header_overridden {
        "header_overridden".into()
    } else {
        "messages".into()
    }
}

pub fn chat_token_estimate(request: &NormalizedChatRequest) -> TokenEstimate {
    TokenEstimate {
        input_tokens: request
            .prompt_size_hint()
            .div_ceil(4)
            .min(u32::MAX as usize) as u32,
        output_tokens: request.max_tokens.unwrap_or(1024),
    }
}

fn agent_error(status: u16, code: &'static str, message: &str) -> ApiError {
    ApiError::AgentRouting {
        http_status: status,
        code,
        message: message.to_string(),
        retry_after: if status == 409 { Some(1) } else { None },
    }
}

pub fn decision_json(decision: &AgentDecision) -> serde_json::Value {
    serde_json::json!({
        "adaptive": decision.adaptive,
        "profile_id": decision.profile_id,
        "profile_revision": decision.revision,
        "phase": decision.phase,
        "phase_evidence": decision.phase_evidence,
        "task_type": decision.task_type,
        "tier": decision.tier,
        "source": decision.source,
        "rule_index": decision.rule_index,
        "classifier_status": decision.classifier_status,
        "classifier_cache": decision.classifier_cache,
        "classifier_latency_ms": decision.classifier_latency_ms,
        "pin": decision.pin,
        "affinity": decision.affinity,
        "classification_required": decision.classification_required,
        "simulated": decision.simulated,
        "session_digest": decision.session_digest,
        "subtask_digest": decision.subtask_digest,
    })
}
