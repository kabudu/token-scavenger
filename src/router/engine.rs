use crate::api::error::ApiError;
use crate::api::openai::chat::*;
use crate::api::openai::embeddings::*;
use crate::app::state::AppState;
use crate::config::schema::Config;
use crate::discovery::model_intelligence::{
    ModelRequestRequirements, filter_by_model_intelligence,
};
use crate::providers::registry::ProviderRegistry;
use crate::providers::traits::{EndpointKind, ProviderContext, ProviderError};
use crate::router::fallback::{FallbackDecision, should_fallback};
use crate::router::policy::RoutePolicy;
use crate::router::selection::{
    TokenEstimate, apply_policy_engine, assign_attempt_priorities, build_attempt_plan_for_target,
    filter_by_health, filter_by_model_enabled_for_endpoint, filter_by_paid_policy,
    prioritize_for_tool_use, record_context_failure_hint, record_rate_limit_hint,
    should_skip_for_context_hint, should_skip_for_rate_limit_hint,
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
    request_id: String,
) -> Result<ChatResponse, ApiError> {
    let started_at = std::time::Instant::now();
    let config = state.config();
    let registry = &state.provider_registry;
    let policy = RoutePolicy::from_config(&config);

    // Resolve model group
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

    // Build attempt plan
    let mut plan = Vec::new();
    for target in &resolved_targets {
        let model_plan =
            build_attempt_plan_for_target(&policy, registry, target, EndpointKind::ChatCompletions)
                .await;
        plan.extend(model_plan);
    }
    assign_attempt_priorities(&mut plan);

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

    // Filter by health and breaker state
    let mut plan = filter_by_model_enabled_for_endpoint(
        filter_by_paid_policy(filter_by_health(plan, &state), &state),
        &state,
        EndpointKind::ChatCompletions,
    )
    .await;
    plan = filter_by_model_intelligence(plan, &state, ModelRequestRequirements::for_chat(&request))
        .await;
    if request.tools.is_some() {
        plan = prioritize_for_tool_use(plan, &state).await;
    }
    plan = apply_policy_engine(
        plan,
        &state,
        &policy,
        &request.model,
        EndpointKind::ChatCompletions,
        chat_token_estimate(&request),
    )
    .await;

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
        };

        // Attempt the request
        crate::observability::record_attempt_started(&state, &request_id, "chat", attempt).await;
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
                let _ = crate::usage::accounting::record_usage(
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
                                    let _ = crate::usage::accounting::record_usage(
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
                                let _ = crate::usage::accounting::record_usage(
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

fn chat_token_estimate(request: &NormalizedChatRequest) -> TokenEstimate {
    TokenEstimate {
        input_tokens: chars_to_token_hint(request.prompt_size_hint()),
        output_tokens: request.max_tokens.unwrap_or(1024),
    }
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
        },
    )
    .await;
}
