use crate::api::error::ApiError;
use crate::api::openai::chat::*;
use crate::api::openai::embeddings::*;
use crate::app::state::AppState;
use crate::config::schema::Config;
use crate::providers::registry::ProviderRegistry;
use crate::providers::traits::{EndpointKind, ProviderContext, ProviderError};
use crate::router::fallback::{FallbackDecision, should_fallback};
use crate::router::policy::RoutePolicy;
use crate::router::selection::{
    TokenEstimate, apply_policy_engine, assign_attempt_priorities, build_attempt_plan_for_target,
    filter_by_health, filter_by_model_enabled_for_endpoint, filter_by_paid_policy,
    record_context_failure_hint, record_rate_limit_hint, should_skip_for_context_hint,
    should_skip_for_rate_limit_hint,
};
use std::sync::Arc;
use tracing::{info, warn};

/// The route planning and execution engine.
pub struct RouteEngine {
    config: Arc<Config>,
}

impl RouteEngine {
    pub fn new(_provider_registry: Arc<ProviderRegistry>, config: Arc<Config>) -> Self {
        Self { config }
    }

    /// Build the route policy from the current config.
    pub fn policy(&self) -> RoutePolicy {
        RoutePolicy::from_config(&self.config)
    }

    /// Re-initialize with a new config snapshot.
    pub fn update_config(&mut self, config: Arc<Config>) {
        self.config = config;
    }
}

/// Route a chat completion request through the provider chain.
pub async fn route_chat_request(
    state: AppState,
    request: NormalizedChatRequest,
    context: crate::router::context::RoutingContext,
) -> Result<ChatResponse, ApiError> {
    let started_at = std::time::Instant::now();
    let request_id = context.public_request_id.clone();
    let config = state.config();
    let registry = &state.provider_registry;
    let prepared = crate::router::planner::prepare_chat_plan(
        &state,
        &request,
        &context,
        crate::router::planner::PlanMode::Execute,
    )
    .await?;
    let lock_candidate = prepared
        .decision
        .as_ref()
        .is_some_and(|decision| decision.lock_candidate);
    let agent_decision = prepared.decision;
    let lease = prepared.lease;
    let plan = prepared.attempts;
    let resolved_models = prepared.resolved_models;
    let token_estimate = prepared.token_estimate;
    let adaptive = agent_decision.is_some();
    let resolved_model_label = resolved_models.join(", ");
    if let Some(decision) = &agent_decision {
        crate::metrics::prometheus::record_agent_decision(
            &decision.source,
            &decision.tier,
            &decision.phase,
        );
        crate::metrics::prometheus::record_pin_outcome(&decision.pin);
        crate::metrics::prometheus::record_classifier_outcome(&decision.classifier_status);
        crate::observability::record_agent_decision(&state, &request_id, decision).await;
    }
    if plan.is_empty() {
        crate::observability::record_route_plan(
            &state,
            &request_id,
            "chat",
            &request.model,
            &resolved_models,
            &plan,
        )
        .await;
        record_route_failure(
            &state,
            RouteFailure {
                request_id: &request_id,
                endpoint_kind: "chat",
                requested_model: &request.model,
                selected_provider_id: None,
                selected_model_id: None,
                status: "route_exhausted",
                http_status: 503,
                started_at,
                streaming: false,
            },
        )
        .await;
        return Err(ApiError::RouteExhausted(format!(
            "No available providers for model(s): {:?}",
            resolved_models
        )));
    }

    if plan.is_empty() {
        crate::observability::record_route_plan(
            &state,
            &request_id,
            "chat",
            &request.model,
            &resolved_models,
            &plan,
        )
        .await;
        record_route_failure(
            &state,
            RouteFailure {
                request_id: &request_id,
                endpoint_kind: "chat",
                requested_model: &request.model,
                selected_provider_id: None,
                selected_model_id: Some(&resolved_model_label),
                status: "route_exhausted",
                http_status: 503,
                started_at,
                streaming: false,
            },
        )
        .await;
        return Err(ApiError::RouteExhausted(format!(
            "All providers for model(s) '{:?}' are unhealthy or blocked",
            resolved_models
        )));
    }

    // Trace the plan
    info!(
        request_model = %request.model,
        resolved_models = ?resolved_models,
        plan = ?plan.iter().map(|p| p.label()).collect::<Vec<_>>(),
        "Route plan built"
    );
    crate::observability::record_route_plan(
        &state,
        &request_id,
        "chat",
        &request.model,
        &resolved_models,
        &plan,
    )
    .await;

    // Execute the plan: try providers in order
    let mut last_error = None;
    let mut budget_blocked: Option<ApiError> = None;
    let prompt_size_hint = request.prompt_size_hint();
    for attempt in &plan {
        let attempt_started_at = std::time::Instant::now();
        let provider_id = &attempt.provider_id;
        if should_skip_for_context_hint(&state, attempt, prompt_size_hint) {
            crate::observability::record_skip(
                &state,
                &request_id,
                "chat",
                attempt,
                "recent context budget failure hint",
            )
            .await;
            if lock_candidate {
                return Err(affinity_target_unavailable());
            }
            continue;
        }
        if should_skip_for_rate_limit_hint(&state, attempt) {
            crate::observability::record_skip(
                &state,
                &request_id,
                "chat",
                attempt,
                "recent rate limit hint",
            )
            .await;
            if lock_candidate {
                return Err(affinity_target_unavailable());
            }
            continue;
        }

        // Skip if provider is unhealthy or breaker is open
        if let Some(breaker) = state.breaker_states.get(provider_id) {
            if breaker.is_open() {
                info!(provider = %provider_id, "Skipping: circuit breaker open");
                crate::observability::record_skip(
                    &state,
                    &request_id,
                    "chat",
                    attempt,
                    "circuit breaker open",
                )
                .await;
                if lock_candidate {
                    return Err(affinity_target_unavailable());
                }
                continue;
            }
        }

        // Get the adapter
        let adapter = match registry.get(provider_id).await {
            Some(a) => a,
            None => {
                warn!(provider = %provider_id, "Provider not found in registry");
                crate::observability::record_skip(
                    &state,
                    &request_id,
                    "chat",
                    attempt,
                    "provider adapter missing from registry",
                )
                .await;
                if lock_candidate {
                    return Err(affinity_target_unavailable());
                }
                continue;
            }
        };

        let capabilities = adapter.capabilities();
        if request.tools.is_some() && !capabilities.supports_tools {
            info!(provider = %provider_id, "Skipping: tools not supported");
            crate::observability::record_skip(
                &state,
                &request_id,
                "chat",
                attempt,
                "tools not supported",
            )
            .await;
            if lock_candidate {
                return Err(affinity_target_unavailable());
            }
            continue;
        }
        if request.response_format.is_some() && !capabilities.supports_json_mode {
            info!(provider = %provider_id, "Skipping: response_format/json mode not supported");
            crate::observability::record_skip(
                &state,
                &request_id,
                "chat",
                attempt,
                "json mode not supported",
            )
            .await;
            if lock_candidate {
                return Err(affinity_target_unavailable());
            }
            continue;
        }

        // Build provider context
        let provider_cfg = config
            .providers
            .iter()
            .find(|p| p.id == *provider_id)
            .ok_or_else(|| {
                ApiError::InternalError(format!("Provider config not found: {}", provider_id))
            })?;

        let ctx = ProviderContext {
            base_url: adapter.base_url(provider_cfg),
            api_key: provider_cfg.api_key.clone(),
            config: Arc::new(provider_cfg.clone()),
            client: state.http_client.clone(),
            request_timeout: std::time::Duration::from_millis(
                state.config().server.request_timeout_ms,
            ),
        };

        // Attempt the request
        crate::observability::record_attempt_started(&state, &request_id, "chat", attempt).await;
        let inference_hold = match begin_inference_hold(
            &state,
            adaptive,
            &context.project.project_id,
            &context.project.api_key_prefix,
            &request_id,
            provider_id,
            &attempt.model_id,
            &request.model,
            provider_cfg.free_only,
            &token_estimate,
        )
        .await
        {
            Ok(hold) => hold,
            Err(error) => {
                crate::observability::record_skip(
                    &state,
                    &request_id,
                    "chat",
                    attempt,
                    "budget_denied",
                )
                .await;
                if lock_candidate {
                    return Err(error);
                }
                budget_blocked = Some(error);
                continue;
            }
        };
        match adapter
            .chat_completions(
                &ctx,
                NormalizedChatRequest {
                    model: attempt.model_id.clone(),
                    ..request.clone()
                },
            )
            .await
        {
            Ok(response) => {
                info!(
                    provider = %provider_id,
                    latency_ms = response.latency_ms,
                    "Chat completion succeeded"
                );
                crate::observability::record_attempt_result(
                    &state,
                    &request_id,
                    "chat",
                    attempt,
                    "success",
                    Some(response.latency_ms),
                    None,
                )
                .await;

                // Record usage and metrics
                let usage_ref = response.usage.as_ref().map(provider_usage_to_openai_usage);
                let recorded = crate::usage::accounting::record_usage(
                    &state,
                    crate::usage::accounting::UsageRecord {
                        provider_id,
                        model_id: &response.model_id,
                        requested_model: &request.model,
                        usage: usage_ref.as_ref(),
                        latency_ms: response.latency_ms,
                        free_tier: provider_cfg.free_only,
                        request_id: &request_id,
                        endpoint_kind: "chat",
                        streaming: false,
                    },
                )
                .await;
                settle_if_recorded(inference_hold, recorded.is_ok()).await;
                finalize_agent_success(
                    &state,
                    &request_id,
                    agent_decision.as_ref(),
                    lease.as_ref(),
                    attempt,
                    &response,
                )
                .await;

                return Ok(ChatResponse {
                    id: format!("ts-{}", uuid::Uuid::new_v4()),
                    object: "chat.completion".into(),
                    created: chrono::Utc::now().timestamp(),
                    model: response.model_id,
                    choices: vec![ChatChoice {
                        index: 0,
                        message: ChatResponseMessage {
                            role: "assistant".into(),
                            content: response.content,
                            tool_calls: response.tool_calls,
                        },
                        finish_reason: response.finish_reason,
                        logprobs: None,
                    }],
                    usage: response.usage.as_ref().map(provider_usage_to_openai_usage),
                });
            }
            Err(e) => {
                drop(inference_hold);
                warn!(provider = %provider_id, error = %e, "Provider attempt failed");
                crate::observability::record_attempt_result(
                    &state,
                    &request_id,
                    "chat",
                    attempt,
                    failure_status_for_error(Some(&e)),
                    Some(attempt_started_at.elapsed().as_millis() as i64),
                    Some(&e.to_string()),
                )
                .await;

                // Record provider-health failures only for errors that indicate
                // provider instability rather than request-specific capacity.
                if crate::resilience::health::should_record_provider_failure(&e) {
                    crate::resilience::health::record_failure(&state, provider_id).await;
                }
                if e.is_negative_context_budget_error() {
                    record_context_failure_hint(
                        &state,
                        provider_id,
                        &attempt.model_id,
                        prompt_size_hint,
                    );
                }
                if let ProviderError::RateLimited { retry_after, .. } = &e {
                    record_rate_limit_hint(&state, provider_id, &attempt.model_id, *retry_after);
                }

                // Use fallback engine to decide next action
                let decision = should_fallback(&state, &e).await;
                last_error = Some(e);

                match decision {
                    FallbackDecision::Retry { max_attempts } => {
                        // Simple: retry same provider up to N times
                        let mut retries = 0;
                        while retries < max_attempts {
                            warn!(provider = %provider_id, retry = retries + 1, "Retrying same provider");
                            crate::observability::record_event(
                                &state,
                                crate::observability::TraceEventRecord {
                                    request_id: &request_id,
                                    event_type: "attempt_retry",
                                    provider_id: Some(provider_id),
                                    model_id: Some(&attempt.model_id),
                                    outcome: Some("retrying"),
                                    latency_ms: None,
                                    details: serde_json::json!({
                                        "endpoint_kind": "chat",
                                        "retry": retries + 1,
                                        "max_attempts": max_attempts,
                                    }),
                                },
                            )
                            .await;
                            let retry_hold = match begin_inference_hold(
                                &state,
                                adaptive,
                                &context.project.project_id,
                                &context.project.api_key_prefix,
                                &request_id,
                                provider_id,
                                &attempt.model_id,
                                &request.model,
                                provider_cfg.free_only,
                                &token_estimate,
                            )
                            .await
                            {
                                Ok(hold) => hold,
                                Err(error) => {
                                    if lock_candidate {
                                        return Err(error);
                                    }
                                    budget_blocked = Some(error);
                                    break;
                                }
                            };
                            match adapter
                                .chat_completions(
                                    &ctx,
                                    NormalizedChatRequest {
                                        model: attempt.model_id.clone(),
                                        ..request.clone()
                                    },
                                )
                                .await
                            {
                                Ok(response) => {
                                    info!(provider = %provider_id, "Retry succeeded");
                                    crate::observability::record_attempt_result(
                                        &state,
                                        &request_id,
                                        "chat",
                                        attempt,
                                        "success_after_retry",
                                        Some(response.latency_ms),
                                        None,
                                    )
                                    .await;
                                    let usage_ref =
                                        response.usage.as_ref().map(provider_usage_to_openai_usage);
                                    let recorded = crate::usage::accounting::record_usage(
                                        &state,
                                        crate::usage::accounting::UsageRecord {
                                            provider_id,
                                            model_id: &response.model_id,
                                            requested_model: &request.model,
                                            usage: usage_ref.as_ref(),
                                            latency_ms: response.latency_ms,
                                            free_tier: provider_cfg.free_only,
                                            request_id: &request_id,
                                            endpoint_kind: "chat",
                                            streaming: false,
                                        },
                                    )
                                    .await;
                                    settle_if_recorded(retry_hold, recorded.is_ok()).await;
                                    finalize_agent_success(
                                        &state,
                                        &request_id,
                                        agent_decision.as_ref(),
                                        lease.as_ref(),
                                        attempt,
                                        &response,
                                    )
                                    .await;
                                    return Ok(ChatResponse {
                                        id: format!("ts-{}", uuid::Uuid::new_v4()),
                                        object: "chat.completion".into(),
                                        created: chrono::Utc::now().timestamp(),
                                        model: response.model_id,
                                        choices: vec![ChatChoice {
                                            index: 0,
                                            message: ChatResponseMessage {
                                                role: "assistant".into(),
                                                content: response.content,
                                                tool_calls: response.tool_calls,
                                            },
                                            finish_reason: response.finish_reason,
                                            logprobs: None,
                                        }],
                                        usage: response
                                            .usage
                                            .as_ref()
                                            .map(provider_usage_to_openai_usage),
                                    });
                                }
                                Err(e2) => {
                                    warn!(provider = %provider_id, error = %e2, "Retry failed");
                                    crate::observability::record_attempt_result(
                                        &state,
                                        &request_id,
                                        "chat",
                                        attempt,
                                        failure_status_for_error(Some(&e2)),
                                        None,
                                        Some(&e2.to_string()),
                                    )
                                    .await;
                                    if let ProviderError::RateLimited { retry_after, .. } = &e2 {
                                        record_rate_limit_hint(
                                            &state,
                                            provider_id,
                                            &attempt.model_id,
                                            *retry_after,
                                        );
                                    }
                                    last_error = Some(e2);
                                    retries += 1;
                                }
                            }
                        }
                    }
                    FallbackDecision::RetryWithDelay { delay_ms } => {
                        crate::observability::record_event(
                            &state,
                            crate::observability::TraceEventRecord {
                                request_id: &request_id,
                                event_type: "attempt_retry_delay",
                                provider_id: Some(provider_id),
                                model_id: Some(&attempt.model_id),
                                outcome: Some("delayed_retry"),
                                latency_ms: None,
                                details: serde_json::json!({
                                    "endpoint_kind": "chat",
                                    "delay_ms": delay_ms,
                                }),
                            },
                        )
                        .await;
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                        let delay_hold = match begin_inference_hold(
                            &state,
                            adaptive,
                            &context.project.project_id,
                            &context.project.api_key_prefix,
                            &request_id,
                            provider_id,
                            &attempt.model_id,
                            &request.model,
                            provider_cfg.free_only,
                            &token_estimate,
                        )
                        .await
                        {
                            Ok(hold) => hold,
                            Err(error) => {
                                if lock_candidate {
                                    return Err(error);
                                }
                                budget_blocked = Some(error);
                                continue;
                            }
                        };
                        match adapter
                            .chat_completions(
                                &ctx,
                                NormalizedChatRequest {
                                    model: attempt.model_id.clone(),
                                    ..request.clone()
                                },
                            )
                            .await
                        {
                            Ok(response) => {
                                info!(provider = %provider_id, "Retry after delay succeeded");
                                crate::observability::record_attempt_result(
                                    &state,
                                    &request_id,
                                    "chat",
                                    attempt,
                                    "success_after_delay",
                                    Some(response.latency_ms),
                                    None,
                                )
                                .await;
                                let usage_ref =
                                    response.usage.as_ref().map(provider_usage_to_openai_usage);
                                let recorded = crate::usage::accounting::record_usage(
                                    &state,
                                    crate::usage::accounting::UsageRecord {
                                        provider_id,
                                        model_id: &response.model_id,
                                        requested_model: &request.model,
                                        usage: usage_ref.as_ref(),
                                        latency_ms: response.latency_ms,
                                        free_tier: provider_cfg.free_only,
                                        request_id: &request_id,
                                        endpoint_kind: "chat",
                                        streaming: false,
                                    },
                                )
                                .await;
                                settle_if_recorded(delay_hold, recorded.is_ok()).await;
                                finalize_agent_success(
                                    &state,
                                    &request_id,
                                    agent_decision.as_ref(),
                                    lease.as_ref(),
                                    attempt,
                                    &response,
                                )
                                .await;
                                return Ok(ChatResponse {
                                    id: format!("ts-{}", uuid::Uuid::new_v4()),
                                    object: "chat.completion".into(),
                                    created: chrono::Utc::now().timestamp(),
                                    model: response.model_id,
                                    choices: vec![ChatChoice {
                                        index: 0,
                                        message: ChatResponseMessage {
                                            role: "assistant".into(),
                                            content: response.content,
                                            tool_calls: response.tool_calls,
                                        },
                                        finish_reason: response.finish_reason,
                                        logprobs: None,
                                    }],
                                    usage: response
                                        .usage
                                        .as_ref()
                                        .map(provider_usage_to_openai_usage),
                                });
                            }
                            Err(e2) => {
                                crate::observability::record_attempt_result(
                                    &state,
                                    &request_id,
                                    "chat",
                                    attempt,
                                    failure_status_for_error(Some(&e2)),
                                    None,
                                    Some(&e2.to_string()),
                                )
                                .await;
                                if let ProviderError::RateLimited { retry_after, .. } = &e2 {
                                    record_rate_limit_hint(
                                        &state,
                                        provider_id,
                                        &attempt.model_id,
                                        *retry_after,
                                    );
                                }
                                last_error = Some(e2);
                                /* fall through to next provider */
                            }
                        }
                    }
                    FallbackDecision::TryNextProvider => {
                        crate::observability::record_event(
                            &state,
                            crate::observability::TraceEventRecord {
                                request_id: &request_id,
                                event_type: "fallback_decision",
                                provider_id: Some(provider_id),
                                model_id: Some(&attempt.model_id),
                                outcome: Some("try_next_provider"),
                                latency_ms: None,
                                details: serde_json::json!({"endpoint_kind": "chat"}),
                            },
                        )
                        .await;
                    }
                    FallbackDecision::Fail => {
                        record_route_failure(
                            &state,
                            RouteFailure {
                                request_id: &request_id,
                                endpoint_kind: "chat",
                                requested_model: &request.model,
                                selected_provider_id: Some(provider_id),
                                selected_model_id: Some(&attempt.model_id),
                                status: failure_status_for_error(last_error.as_ref()),
                                http_status: failure_http_status_for_error(last_error.as_ref()),
                                started_at,
                                streaming: false,
                            },
                        )
                        .await;
                        return Err(api_error_for_exhausted(
                            format!("Provider {} failed", provider_id),
                            last_error.as_ref(),
                        ));
                    }
                }
            }
        }
    }

    // All providers exhausted
    if last_error.is_none() {
        if let Some(error) = budget_blocked {
            let (http_status, code) = match &error {
                ApiError::AgentRouting {
                    http_status, code, ..
                } => (*http_status, *code),
                _ => (503, "budget_denied"),
            };
            record_route_failure(
                &state,
                RouteFailure {
                    request_id: &request_id,
                    endpoint_kind: "chat",
                    requested_model: &request.model,
                    selected_provider_id: plan.last().map(|p| p.provider_id.as_str()),
                    selected_model_id: plan.last().map(|p| p.model_id.as_str()),
                    status: code,
                    http_status,
                    started_at,
                    streaming: false,
                },
            )
            .await;
            return Err(error);
        }
    }
    let msg = match last_error {
        Some(ref e) => format!("All providers failed. Last error: {}", e),
        None => format!("No available providers for model(s): {:?}", resolved_models),
    };

    record_route_failure(
        &state,
        RouteFailure {
            request_id: &request_id,
            endpoint_kind: "chat",
            requested_model: &request.model,
            selected_provider_id: plan.last().map(|p| p.provider_id.as_str()),
            selected_model_id: plan.last().map(|p| p.model_id.as_str()),
            status: failure_status_for_error(last_error.as_ref()),
            http_status: failure_http_status_for_error(last_error.as_ref()),
            started_at,
            streaming: false,
        },
    )
    .await;

    Err(api_error_for_exhausted(msg, last_error.as_ref()))
}

/// Route an embeddings request through the provider chain.
pub async fn route_embeddings_request(
    state: AppState,
    request: NormalizedEmbeddingsRequest,
    request_id: String,
) -> Result<EmbeddingsResponse, ApiError> {
    let started_at = std::time::Instant::now();
    let config = state.config();
    let registry = &state.provider_registry;
    let policy = RoutePolicy::from_config(&config);

    let resolved_targets =
        crate::router::model_groups::resolve_model_group_targets(&state, &request.model)
            .await
            .unwrap_or_else(|| {
                vec![crate::router::model_groups::ModelTarget::any_provider(
                    request.model.clone(),
                )]
            });
    let resolved_models = resolved_targets
        .iter()
        .map(|target| target.label())
        .collect::<Vec<_>>();
    let resolved_model_label = resolved_models.join(", ");

    let mut plan = Vec::new();
    for target in &resolved_targets {
        let model_plan =
            build_attempt_plan_for_target(&policy, registry, target, EndpointKind::Embeddings)
                .await;
        plan.extend(model_plan);
    }
    assign_attempt_priorities(&mut plan);

    if plan.is_empty() {
        crate::observability::record_route_plan(
            &state,
            &request_id,
            "embeddings",
            &request.model,
            &resolved_models,
            &plan,
        )
        .await;
        record_route_failure(
            &state,
            RouteFailure {
                request_id: &request_id,
                endpoint_kind: "embeddings",
                requested_model: &request.model,
                selected_provider_id: None,
                selected_model_id: None,
                status: "route_exhausted",
                http_status: 503,
                started_at,
                streaming: false,
            },
        )
        .await;
        return Err(ApiError::RouteExhausted(format!(
            "No available providers for embeddings model(s): {:?}",
            resolved_models
        )));
    }

    let plan = filter_by_model_enabled_for_endpoint(
        filter_by_paid_policy(filter_by_health(plan, &state), &state),
        &state,
        EndpointKind::Embeddings,
    )
    .await;
    let plan = apply_policy_engine(
        plan,
        &state,
        &policy,
        &request.model,
        EndpointKind::Embeddings,
        embeddings_token_estimate(&request),
    )
    .await;
    let plan = crate::projects::filter_project_policy(
        plan,
        &state,
        &request_id,
        &request.model,
        embeddings_token_estimate(&request),
    )
    .await?;
    if plan.is_empty() {
        crate::observability::record_route_plan(
            &state,
            &request_id,
            "embeddings",
            &request.model,
            &resolved_models,
            &plan,
        )
        .await;
        record_route_failure(
            &state,
            RouteFailure {
                request_id: &request_id,
                endpoint_kind: "embeddings",
                requested_model: &request.model,
                selected_provider_id: None,
                selected_model_id: Some(&resolved_model_label),
                status: "route_exhausted",
                http_status: 503,
                started_at,
                streaming: false,
            },
        )
        .await;
    }
    let mut last_error = None;
    crate::observability::record_route_plan(
        &state,
        &request_id,
        "embeddings",
        &request.model,
        &resolved_models,
        &plan,
    )
    .await;
    for attempt in &plan {
        let attempt_started_at = std::time::Instant::now();
        let provider_id = &attempt.provider_id;

        if let Some(breaker) = state.breaker_states.get(provider_id) {
            if breaker.is_open() {
                crate::observability::record_skip(
                    &state,
                    &request_id,
                    "embeddings",
                    attempt,
                    "circuit breaker open",
                )
                .await;
                continue;
            }
        }

        let adapter = match registry.get(provider_id).await {
            Some(a) => a,
            None => {
                crate::observability::record_skip(
                    &state,
                    &request_id,
                    "embeddings",
                    attempt,
                    "provider adapter missing from registry",
                )
                .await;
                continue;
            }
        };

        if !adapter.supports_endpoint(&EndpointKind::Embeddings) {
            crate::observability::record_skip(
                &state,
                &request_id,
                "embeddings",
                attempt,
                "embeddings not supported",
            )
            .await;
            continue;
        }

        let provider_cfg = config
            .providers
            .iter()
            .find(|p| p.id == *provider_id)
            .ok_or_else(|| {
                ApiError::InternalError(format!("Provider config not found: {}", provider_id))
            })?;

        let ctx = ProviderContext {
            base_url: adapter.base_url(provider_cfg),
            api_key: provider_cfg.api_key.clone(),
            config: Arc::new(provider_cfg.clone()),
            client: state.http_client.clone(),
            request_timeout: std::time::Duration::from_millis(
                state.config().server.request_timeout_ms,
            ),
        };

        crate::observability::record_attempt_started(&state, &request_id, "embeddings", attempt)
            .await;
        match adapter
            .embeddings(
                &ctx,
                NormalizedEmbeddingsRequest {
                    model: attempt.model_id.clone(),
                    ..request.clone()
                },
            )
            .await
        {
            Ok(response) => {
                crate::observability::record_attempt_result(
                    &state,
                    &request_id,
                    "embeddings",
                    attempt,
                    "success",
                    Some(response.latency_ms),
                    None,
                )
                .await;
                let usage = crate::api::openai::chat::UsageResponse {
                    prompt_tokens: response.usage.prompt_tokens,
                    completion_tokens: response.usage.completion_tokens,
                    total_tokens: response.usage.total_tokens,
                    prompt_cache_hit_tokens: response.usage.prompt_cache_hit_tokens,
                    prompt_cache_miss_tokens: response.usage.prompt_cache_miss_tokens,
                    reasoning_tokens: response.usage.reasoning_tokens,
                };
                let _ = crate::usage::accounting::record_usage(
                    &state,
                    crate::usage::accounting::UsageRecord {
                        provider_id,
                        model_id: &response.model_id,
                        requested_model: &request.model,
                        usage: Some(&usage),
                        latency_ms: response.latency_ms,
                        free_tier: provider_cfg.free_only,
                        request_id: &request_id,
                        endpoint_kind: "embeddings",
                        streaming: false,
                    },
                )
                .await;
                return Ok(EmbeddingsResponse {
                    object: "list".into(),
                    data: response.data,
                    model: response.model_id,
                    usage,
                });
            }
            Err(e) => {
                crate::observability::record_attempt_result(
                    &state,
                    &request_id,
                    "embeddings",
                    attempt,
                    failure_status_for_error(Some(&e)),
                    Some(attempt_started_at.elapsed().as_millis() as i64),
                    Some(&e.to_string()),
                )
                .await;
                last_error = Some(e);
                if let Some(error) = last_error.as_ref() {
                    if crate::resilience::health::should_record_provider_failure(error) {
                        crate::resilience::health::record_failure(&state, provider_id).await;
                    }
                }
            }
        }
    }

    record_route_failure(
        &state,
        RouteFailure {
            request_id: &request_id,
            endpoint_kind: "embeddings",
            requested_model: &request.model,
            selected_provider_id: plan.last().map(|p| p.provider_id.as_str()),
            selected_model_id: plan.last().map(|p| p.model_id.as_str()),
            status: failure_status_for_error(last_error.as_ref()),
            http_status: failure_http_status_for_error(last_error.as_ref()),
            started_at,
            streaming: false,
        },
    )
    .await;

    Err(api_error_for_exhausted(
        format!(
            "No embeddings provider available. Last error: {:?}",
            last_error
        ),
        last_error.as_ref(),
    ))
}

fn embeddings_token_estimate(request: &NormalizedEmbeddingsRequest) -> TokenEstimate {
    let input_chars = request.input.iter().map(|item| item.len()).sum::<usize>();
    TokenEstimate {
        input_tokens: chars_to_token_hint(input_chars),
        output_tokens: 0,
    }
}

fn chars_to_token_hint(chars: usize) -> u32 {
    chars.div_ceil(4).min(u32::MAX as usize) as u32
}

fn api_error_for_exhausted(message: String, last_error: Option<&ProviderError>) -> ApiError {
    match last_error {
        Some(ProviderError::RateLimited { retry_after, .. }) => ApiError::RateLimited {
            message,
            retry_after: *retry_after,
        },
        Some(ProviderError::QuotaExhausted { reset_at, .. }) => ApiError::RateLimited {
            message,
            retry_after: reset_at.and_then(retry_after_from_epoch),
        },
        _ => ApiError::RouteExhausted(message),
    }
}

fn affinity_target_unavailable() -> ApiError {
    ApiError::AgentRouting {
        http_status: 503,
        code: "affinity_target_unavailable",
        message: "the required affinity target is temporarily unavailable".into(),
        retry_after: Some(1),
    }
}

async fn finalize_agent_success(
    state: &AppState,
    request_id: &str,
    decision: Option<&crate::router::planner::AgentDecision>,
    lease: Option<&crate::router::affinity::AffinityLease>,
    attempt: &crate::router::selection::RouteAttempt,
    response: &crate::api::openai::chat::ProviderChatResponse,
) {
    let Some(decision) = decision else {
        return;
    };
    let tool_ids = response
        .tool_calls
        .as_ref()
        .map(|calls| calls.iter().map(|call| call.id.clone()).collect::<Vec<_>>())
        .unwrap_or_default();
    if let Some(lease) = lease {
        lease.commit_success(
            &attempt.provider_id,
            &attempt.model_id,
            decision.tier_value,
            &decision.revision,
            &tool_ids,
            crate::router::continuation::continuation_support(&attempt.provider_id),
            std::time::Duration::from_secs(state.config().routing.agent.session_idle_ttl_seconds),
        );
    }
    let _ = crate::usage::accounting::annotate_request_decision(
        state,
        request_id,
        crate::usage::accounting::DecisionStamp {
            session_digest: decision.session_digest.as_deref(),
            subtask_digest: decision.subtask_digest.as_deref(),
            task_phase: Some(&decision.phase),
            tier: Some(&decision.tier),
            selection_source: Some(&decision.source),
            profile_revision: Some(&decision.revision),
            classifier_status: Some(&decision.classifier_status),
        },
    )
    .await;
}

fn retry_after_from_epoch(reset_at: i64) -> Option<u64> {
    let now = chrono::Utc::now().timestamp();
    (reset_at > now).then_some((reset_at - now) as u64)
}

fn failure_status_for_error(last_error: Option<&ProviderError>) -> &'static str {
    match last_error {
        Some(ProviderError::RateLimited { .. }) => "rate_limited",
        Some(ProviderError::QuotaExhausted { .. }) => "quota_exhausted",
        _ => "route_exhausted",
    }
}

fn failure_http_status_for_error(last_error: Option<&ProviderError>) -> u16 {
    match last_error {
        Some(ProviderError::RateLimited { .. } | ProviderError::QuotaExhausted { .. }) => 429,
        _ => 503,
    }
}

fn provider_usage_to_openai_usage(
    usage: &crate::api::openai::chat::ProviderUsage,
) -> crate::api::openai::chat::UsageResponse {
    crate::api::openai::chat::UsageResponse {
        prompt_tokens: usage.prompt_tokens,
        completion_tokens: usage.completion_tokens,
        total_tokens: usage.total_tokens,
        prompt_cache_hit_tokens: usage.prompt_cache_hit_tokens,
        prompt_cache_miss_tokens: usage.prompt_cache_miss_tokens,
        reasoning_tokens: usage.reasoning_tokens,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn begin_inference_hold(
    state: &AppState,
    adaptive: bool,
    project_id: &str,
    api_key_prefix: &str,
    request_id: &str,
    provider_id: &str,
    model_id: &str,
    requested_model: &str,
    free_only: bool,
    estimate: &TokenEstimate,
) -> Result<Option<crate::router::admission::PaidHold>, ApiError> {
    if !adaptive {
        return Ok(None);
    }
    crate::router::admission::reserve_adaptive_paid(
        state,
        project_id,
        api_key_prefix,
        request_id,
        "inference",
        provider_id,
        model_id,
        requested_model,
        free_only,
        estimate.input_tokens,
        estimate.output_tokens,
    )
    .await
}

async fn settle_if_recorded(hold: Option<crate::router::admission::PaidHold>, recorded: bool) {
    if recorded {
        if let Some(hold) = hold {
            hold.settle().await;
        }
    }
}

struct RouteFailure<'a> {
    request_id: &'a str,
    endpoint_kind: &'a str,
    requested_model: &'a str,
    selected_provider_id: Option<&'a str>,
    selected_model_id: Option<&'a str>,
    status: &'a str,
    http_status: u16,
    started_at: std::time::Instant,
    streaming: bool,
}

async fn record_route_failure(state: &AppState, failure: RouteFailure<'_>) {
    let _ = crate::usage::accounting::record_failure(
        state,
        crate::usage::accounting::FailureRecord {
            request_id: failure.request_id,
            endpoint_kind: failure.endpoint_kind,
            requested_model: failure.requested_model,
            selected_provider_id: failure.selected_provider_id,
            selected_model_id: failure.selected_model_id,
            status: failure.status,
            http_status: failure.http_status as i64,
            latency_ms: failure.started_at.elapsed().as_millis() as i64,
            streaming: failure.streaming,
            error_code: Some(failure.status),
            error_summary: Some(failure.status),
        },
    )
    .await;
}
