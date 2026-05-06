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
    build_attempt_plan, filter_by_health, filter_by_model_enabled, filter_by_paid_policy,
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
    let resolved_models = crate::router::model_groups::resolve_model_group(&state, &request.model)
        .await
        .unwrap_or_else(|| vec![request.model.clone()]);

    // Build attempt plan
    let mut plan = Vec::new();
    for model in &resolved_models {
        let model_plan =
            build_attempt_plan(&policy, registry, model, EndpointKind::ChatCompletions).await;
        plan.extend(model_plan);
    }

    if plan.is_empty() {
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
    let plan = filter_by_model_enabled(
        filter_by_paid_policy(filter_by_health(plan, &state), &state),
        &state,
    )
    .await;

    if plan.is_empty() {
        record_route_failure(
            &state,
            RouteFailure {
                request_id: &request_id,
                endpoint_kind: "chat",
                requested_model: &request.model,
                selected_provider_id: None,
                selected_model_id: Some(&resolved_models.join(", ")),
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
        plan = ?plan.iter().map(|p| &p.provider_id).collect::<Vec<_>>(),
        "Route plan built"
    );

    // Execute the plan: try providers in order
    let mut last_error = None;
    for attempt in &plan {
        let provider_id = &attempt.provider_id;

        // Skip if provider is unhealthy or breaker is open
        if let Some(breaker) = state.breaker_states.get(provider_id) {
            if breaker.is_open() {
                info!(provider = %provider_id, "Skipping: circuit breaker open");
                continue;
            }
        }

        // Get the adapter
        let adapter = match registry.get(provider_id).await {
            Some(a) => a,
            None => {
                warn!(provider = %provider_id, "Provider not found in registry");
                continue;
            }
        };

        let capabilities = adapter.capabilities();
        if request.tools.is_some() && !capabilities.supports_tools {
            info!(provider = %provider_id, "Skipping: tools not supported");
            continue;
        }
        if request.response_format.is_some() && !capabilities.supports_json_mode {
            info!(provider = %provider_id, "Skipping: response_format/json mode not supported");
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

                // Record usage and metrics
                let usage_ref = response.usage.as_ref().map(provider_usage_to_openai_usage);
                let _ = crate::usage::accounting::record_usage(
                    &state,
                    crate::usage::accounting::UsageRecord {
                        provider_id,
                        model_id: &response.model_id,
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

                // Record failure in health state
                crate::resilience::health::record_failure(&state, provider_id).await;

                // Use fallback engine to decide next action
                let decision = should_fallback(&state, &e).await;
                last_error = Some(e);

                match decision {
                    FallbackDecision::Retry { max_attempts } => {
                        // Simple: retry same provider up to N times
                        let mut retries = 0;
                        while retries < max_attempts {
                            warn!(provider = %provider_id, retry = retries + 1, "Retrying same provider");
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
                                    let usage_ref =
                                        response.usage.as_ref().map(provider_usage_to_openai_usage);
                                    let _ = crate::usage::accounting::record_usage(
                                        &state,
                                        crate::usage::accounting::UsageRecord {
                                            provider_id,
                                            model_id: &response.model_id,
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
                                    last_error = Some(e2);
                                    retries += 1;
                                }
                            }
                        }
                    }
                    FallbackDecision::RetryWithDelay { delay_ms } => {
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
                                let usage_ref =
                                    response.usage.as_ref().map(provider_usage_to_openai_usage);
                                let _ = crate::usage::accounting::record_usage(
                                    &state,
                                    crate::usage::accounting::UsageRecord {
                                        provider_id,
                                        model_id: &response.model_id,
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
                                last_error = Some(e2);
                                /* fall through to next provider */
                            }
                        }
                    }
                    FallbackDecision::TryNextProvider => { /* continue to next provider */ }
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

    let resolved_models = crate::router::model_groups::resolve_model_group(&state, &request.model)
        .await
        .unwrap_or_else(|| vec![request.model.clone()]);

    let mut plan = Vec::new();
    for model in &resolved_models {
        let model_plan =
            build_attempt_plan(&policy, registry, model, EndpointKind::Embeddings).await;
        plan.extend(model_plan);
    }

    if plan.is_empty() {
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

    let plan = filter_by_model_enabled(
        filter_by_paid_policy(filter_by_health(plan, &state), &state),
        &state,
    )
    .await;
    if plan.is_empty() {
        record_route_failure(
            &state,
            RouteFailure {
                request_id: &request_id,
                endpoint_kind: "embeddings",
                requested_model: &request.model,
                selected_provider_id: None,
                selected_model_id: Some(&resolved_models.join(", ")),
                status: "route_exhausted",
                http_status: 503,
                started_at,
                streaming: false,
            },
        )
        .await;
    }
    let mut last_error = None;
    for attempt in &plan {
        let provider_id = &attempt.provider_id;

        if let Some(breaker) = state.breaker_states.get(provider_id) {
            if breaker.is_open() {
                continue;
            }
        }

        let adapter = match registry.get(provider_id).await {
            Some(a) => a,
            None => continue,
        };

        if !adapter.supports_endpoint(&EndpointKind::Embeddings) {
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
                last_error = Some(e);
                crate::resilience::health::record_failure(&state, provider_id).await;
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

fn api_error_for_exhausted(message: String, last_error: Option<&ProviderError>) -> ApiError {
    match last_error {
        Some(ProviderError::RateLimited { retry_after }) => ApiError::RateLimited {
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
