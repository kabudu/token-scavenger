use crate::api::error::ApiError;
use crate::api::openai::chat::*;
use crate::api::openai::embeddings::*;
use crate::app::state::AppState;
use crate::config::schema::Config;
use crate::providers::registry::ProviderRegistry;
use crate::providers::traits::{EndpointKind, ProviderContext};
use crate::router::fallback::{FallbackDecision, should_fallback};
use crate::router::policy::RoutePolicy;
use crate::router::selection::{build_attempt_plan, filter_by_health, filter_by_model_enabled};
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
    let config = state.config();
    let registry = &state.provider_registry;
    let policy = RoutePolicy::from_config(&config);

    // Resolve model alias
    let resolved_model = crate::router::aliases::resolve_alias(&state, &request.model)
        .await
        .unwrap_or_else(|| request.model.clone());

    // Build attempt plan
    let plan = build_attempt_plan(
        &policy,
        registry,
        &resolved_model,
        EndpointKind::ChatCompletions,
    )
    .await;

    if plan.is_empty() {
        return Err(ApiError::RouteExhausted(format!(
            "No available providers for model: {}",
            resolved_model
        )));
    }

    // Filter by health and breaker state
    let plan = filter_by_model_enabled(filter_by_health(plan, &state), &state).await;

    if plan.is_empty() {
        return Err(ApiError::RouteExhausted(format!(
            "All providers for model '{}' are unhealthy or blocked",
            resolved_model
        )));
    }

    // Trace the plan
    info!(
        request_model = %request.model,
        resolved_model = %resolved_model,
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
                let usage_ref =
                    response
                        .usage
                        .as_ref()
                        .map(|u| crate::api::openai::chat::UsageResponse {
                            prompt_tokens: u.prompt_tokens,
                            completion_tokens: u.completion_tokens,
                            total_tokens: u.total_tokens,
                        });
                let _ = crate::usage::accounting::record_usage(
                    &state,
                    crate::usage::accounting::UsageRecord {
                        provider_id,
                        model_id: &response.model_id,
                        usage: usage_ref.as_ref(),
                        latency_ms: response.latency_ms,
                        free_tier: true,
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
                        .map(|u| crate::api::openai::chat::UsageResponse {
                            prompt_tokens: u.prompt_tokens,
                            completion_tokens: u.completion_tokens,
                            total_tokens: u.total_tokens,
                        }),
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
                                    let usage_ref = response.usage.as_ref().map(|u| {
                                        crate::api::openai::chat::UsageResponse {
                                            prompt_tokens: u.prompt_tokens,
                                            completion_tokens: u.completion_tokens,
                                            total_tokens: u.total_tokens,
                                        }
                                    });
                                    let _ = crate::usage::accounting::record_usage(
                                        &state,
                                        crate::usage::accounting::UsageRecord {
                                            provider_id,
                                            model_id: &response.model_id,
                                            usage: usage_ref.as_ref(),
                                            latency_ms: response.latency_ms,
                                            free_tier: true,
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
                                        usage: response.usage.map(|u| {
                                            crate::api::openai::chat::UsageResponse {
                                                prompt_tokens: u.prompt_tokens,
                                                completion_tokens: u.completion_tokens,
                                                total_tokens: u.total_tokens,
                                            }
                                        }),
                                    });
                                }
                                Err(e2) => {
                                    warn!(provider = %provider_id, error = %e2, "Retry failed");
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
                                let usage_ref = response.usage.as_ref().map(|u| {
                                    crate::api::openai::chat::UsageResponse {
                                        prompt_tokens: u.prompt_tokens,
                                        completion_tokens: u.completion_tokens,
                                        total_tokens: u.total_tokens,
                                    }
                                });
                                let _ = crate::usage::accounting::record_usage(
                                    &state,
                                    crate::usage::accounting::UsageRecord {
                                        provider_id,
                                        model_id: &response.model_id,
                                        usage: usage_ref.as_ref(),
                                        latency_ms: response.latency_ms,
                                        free_tier: true,
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
                                    usage: response.usage.map(|u| {
                                        crate::api::openai::chat::UsageResponse {
                                            prompt_tokens: u.prompt_tokens,
                                            completion_tokens: u.completion_tokens,
                                            total_tokens: u.total_tokens,
                                        }
                                    }),
                                });
                            }
                            Err(_e2) => { /* fall through to next provider */ }
                        }
                    }
                    FallbackDecision::TryNextProvider => { /* continue to next provider */ }
                    FallbackDecision::Fail => {
                        return Err(ApiError::RouteExhausted(format!(
                            "Provider {} failed",
                            provider_id
                        )));
                    }
                }
            }
        }
    }

    // All providers exhausted
    let msg = match last_error {
        Some(ref e) => format!("All providers failed. Last error: {}", e),
        None => format!("No available providers for model: {}", resolved_model),
    };

    Err(ApiError::RouteExhausted(msg))
}

/// Route an embeddings request through the provider chain.
pub async fn route_embeddings_request(
    state: AppState,
    request: NormalizedEmbeddingsRequest,
    request_id: String,
) -> Result<EmbeddingsResponse, ApiError> {
    let config = state.config();
    let registry = &state.provider_registry;
    let policy = RoutePolicy::from_config(&config);

    let resolved_model = crate::router::aliases::resolve_alias(&state, &request.model)
        .await
        .unwrap_or_else(|| request.model.clone());

    let plan =
        build_attempt_plan(&policy, registry, &resolved_model, EndpointKind::Embeddings).await;

    if plan.is_empty() {
        return Err(ApiError::RouteExhausted(format!(
            "No available providers for embeddings model: {}",
            resolved_model
        )));
    }

    let plan = filter_by_model_enabled(filter_by_health(plan, &state), &state).await;
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
                };
                let _ = crate::usage::accounting::record_usage(
                    &state,
                    crate::usage::accounting::UsageRecord {
                        provider_id,
                        model_id: &response.model_id,
                        usage: Some(&usage),
                        latency_ms: response.latency_ms,
                        free_tier: true,
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

    Err(ApiError::RouteExhausted(format!(
        "No embeddings provider available. Last error: {:?}",
        last_error
    )))
}
