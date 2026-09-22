use crate::api::error::ApiError;
use crate::api::openai::chat::StreamDelta;
use crate::api::openai::chat::UsageResponse;
use crate::api::openai::chat::{NormalizedChatRequest, ProviderChatResponse};
use crate::app::state::AppState;
use crate::config::schema::Config;
use crate::providers::traits::{ProviderContext, ProviderError};
use crate::router::selection::{
    RouteAttempt, record_context_failure_hint, record_rate_limit_hint, record_stream_silence_hint,
    should_skip_for_context_hint, should_skip_for_rate_limit_hint,
    should_skip_for_stream_silence_hint_with_alternative,
};
use axum::response::sse::Event;
use futures::stream::Stream;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{info, warn};

/// Streaming SSE event types for chat completions.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// A content delta chunk.
    Chunk {
        id: String,
        created: i64,
        model: String,
        delta: StreamDelta,
        finish_reason: Option<String>,
    },
    /// A tool call delta chunk.
    ToolCallChunk {
        id: String,
        created: i64,
        model: String,
        index: u32,
        tool_call_id: Option<String>,
        function_name: Option<String>,
        function_arguments: String,
    },
    /// Final usage metadata event.
    Usage {
        id: String,
        created: i64,
        model: String,
        prompt_tokens: u32,
        completion_tokens: u32,
        total_tokens: u32,
    },
    /// OpenAI-compatible error event emitted after an SSE response has started.
    Error {
        message: String,
        error_type: String,
        code: String,
    },
    /// Done sentinel.
    Done,
}

use serde::Serialize;

#[derive(Debug, Clone)]
struct StreamUsageContext {
    provider_id: String,
    requested_model: String,
    free_tier: bool,
    started_at: Instant,
}

/// Format a stream event as an OpenAI-compatible SSE data payload.
pub fn format_sse_payload(event: &StreamEvent) -> String {
    match event {
        StreamEvent::Chunk {
            id,
            created,
            model,
            delta,
            finish_reason,
        } => {
            #[derive(Serialize)]
            struct ChunkData<'a> {
                id: &'a str,
                object: &'a str,
                created: i64,
                model: &'a str,
                choices: Vec<ChunkChoice<'a>>,
            }
            #[derive(Serialize)]
            struct ChunkChoice<'a> {
                index: u32,
                delta: &'a StreamDelta,
                finish_reason: Option<&'a str>,
            }
            let data = ChunkData {
                id,
                object: "chat.completion.chunk",
                created: *created,
                model,
                choices: vec![ChunkChoice {
                    index: 0,
                    delta,
                    finish_reason: finish_reason.as_deref(),
                }],
            };
            serde_json::to_string(&data).unwrap_or_default()
        }
        StreamEvent::ToolCallChunk {
            id,
            created,
            model,
            index,
            tool_call_id,
            function_name,
            function_arguments,
        } => serde_json::json!({
            "id": id,
            "object": "chat.completion.chunk",
            "created": created,
            "model": model,
            "choices": [{
                "index": 0,
                "delta": {
                    "tool_calls": [{
                        "index": index,
                        "id": tool_call_id.as_deref().unwrap_or(""),
                        "function": {
                            "name": function_name.as_deref().unwrap_or(""),
                            "arguments": function_arguments,
                        }
                    }]
                },
                "finish_reason": null
            }]
        })
        .to_string(),
        StreamEvent::Usage {
            id,
            created,
            model,
            prompt_tokens,
            completion_tokens,
            total_tokens,
        } => serde_json::json!({
            "id": id,
            "object": "chat.completion.chunk",
            "created": created,
            "model": model,
            "choices": [],
            "usage": {
                "prompt_tokens": prompt_tokens,
                "completion_tokens": completion_tokens,
                "total_tokens": total_tokens,
            }
        })
        .to_string(),
        StreamEvent::Error {
            message,
            error_type,
            code,
        } => serde_json::json!({
            "error": {
                "message": message,
                "type": error_type,
                "param": null,
                "code": code,
            }
        })
        .to_string(),
        StreamEvent::Done => "[DONE]".to_string(),
    }
}

async fn mark_stream_complete(
    state: &AppState,
    request_id: &str,
    pin: &mut Option<crate::router::affinity::StreamPin>,
    decision: Option<&crate::router::planner::AgentDecision>,
) {
    if let Some(pin) = pin.as_mut() {
        pin.completed = true;
    }
    if let Some(decision) = decision {
        let _ = crate::usage::accounting::annotate_request_decision(
            state,
            request_id,
            crate::usage::accounting::DecisionStamp {
                session_digest: decision.session_digest.as_deref(),
                subtask_digest: decision.subtask_digest.as_deref(),
                task_phase: Some(decision.phase.as_str()),
                tier: Some(decision.tier.as_str()),
                selection_source: Some(decision.source.as_str()),
                profile_revision: Some(decision.revision.as_str()),
                classifier_status: Some(decision.classifier_status.as_str()),
            },
        )
        .await;
    }
}

fn observe_forwarded_pin(
    pin: &mut Option<crate::router::affinity::StreamPin>,
    provider_id: &str,
    model_id: &str,
    event: &StreamEvent,
) {
    let Some(pin) = pin.as_mut() else {
        return;
    };
    let tool_id = match event {
        StreamEvent::ToolCallChunk { tool_call_id, .. } => tool_call_id.clone(),
        _ => None,
    };
    pin.observe(provider_id, model_id, tool_id.as_deref());
}

fn stream_event_has_content(event: &StreamEvent) -> bool {
    match event {
        StreamEvent::Chunk { delta, .. } => delta
            .content
            .as_ref()
            .is_some_and(|content| !content.is_empty()),
        StreamEvent::ToolCallChunk {
            tool_call_id,
            function_name,
            function_arguments,
            ..
        } => {
            tool_call_id.as_ref().is_some_and(|value| !value.is_empty())
                || function_name
                    .as_ref()
                    .is_some_and(|value| !value.is_empty())
                || !function_arguments.is_empty()
        }
        StreamEvent::Usage { .. } | StreamEvent::Error { .. } | StreamEvent::Done => false,
    }
}

fn recovered_non_stream_events(response: ProviderChatResponse) -> Option<Vec<StreamEvent>> {
    let has_content = response
        .content
        .as_ref()
        .is_some_and(|content| !content.is_empty());
    let has_tool_calls = response
        .tool_calls
        .as_ref()
        .is_some_and(|tool_calls| !tool_calls.is_empty());
    if !has_content && !has_tool_calls {
        return None;
    }

    let id = uuid::Uuid::new_v4().to_string();
    let created = chrono::Utc::now().timestamp();
    let model = response.model_id;
    let mut events = Vec::new();

    if has_content {
        events.push(StreamEvent::Chunk {
            id: id.clone(),
            created,
            model: model.clone(),
            delta: StreamDelta {
                role: Some("assistant".into()),
                content: response.content,
            },
            finish_reason: if has_tool_calls {
                None
            } else {
                response.finish_reason.clone()
            },
        });
    }

    if let Some(tool_calls) = response.tool_calls {
        for (index, tool_call) in tool_calls.into_iter().enumerate() {
            events.push(StreamEvent::ToolCallChunk {
                id: id.clone(),
                created,
                model: model.clone(),
                index: index as u32,
                tool_call_id: Some(tool_call.id),
                function_name: Some(tool_call.function.name),
                function_arguments: tool_call.function.arguments,
            });
        }
        events.push(StreamEvent::Chunk {
            id: id.clone(),
            created,
            model: model.clone(),
            delta: StreamDelta {
                role: None,
                content: None,
            },
            finish_reason: response.finish_reason,
        });
    }

    if let Some(usage) = response.usage {
        events.push(StreamEvent::Usage {
            id,
            created,
            model,
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
        });
    }
    events.push(StreamEvent::Done);
    Some(events)
}

fn stream_first_content_timeout_ms(
    config: &Config,
    requested_model: &str,
    attempt: &RouteAttempt,
) -> u64 {
    let qualified_model = format!("{}/{}", attempt.provider_id, attempt.model_id);
    [
        requested_model,
        qualified_model.as_str(),
        attempt.model_id.as_str(),
    ]
    .into_iter()
    .find_map(|key| config.routing.stream_first_content_timeout_ms.get(key))
    .copied()
    .unwrap_or(config.server.request_timeout_ms)
    .max(1_000)
}

fn record_stream_timing_async(
    state: &AppState,
    request_id: &str,
    provider_id: &str,
    model_id: &str,
    event_type: &'static str,
    outcome: &'static str,
    latency_ms: i64,
) {
    let state = state.clone();
    let request_id = request_id.to_string();
    let provider_id = provider_id.to_string();
    let model_id = model_id.to_string();
    tokio::spawn(async move {
        crate::observability::record_event(
            &state,
            crate::observability::TraceEventRecord {
                request_id: &request_id,
                event_type,
                provider_id: Some(&provider_id),
                model_id: Some(&model_id),
                outcome: Some(outcome),
                latency_ms: Some(latency_ms),
                details: serde_json::json!({"endpoint_kind": "chat"}),
            },
        )
        .await;
    });
}

fn stream_error_event(error: &ProviderError, config: &Config) -> StreamEvent {
    let safe_message = |message: &str| crate::util::redact::redact_config_secrets(config, message);
    match error {
        ProviderError::RateLimited { details, .. } => StreamEvent::Error {
            message: safe_message(details),
            error_type: "rate_limit_error".into(),
            code: "rate_limit_exceeded".into(),
        },
        ProviderError::QuotaExhausted { details, .. } => StreamEvent::Error {
            message: safe_message(details),
            error_type: "quota_error".into(),
            code: "quota_exhausted".into(),
        },
        _ => StreamEvent::Error {
            message: safe_message(&error.to_string()),
            error_type: "provider_error".into(),
            code: "provider_error".into(),
        },
    }
}

/// Create a streaming SSE response for a chat completion request.
/// Uses the routing engine to find a provider, then streams from it.
pub async fn create_chat_stream(
    state: AppState,
    request: NormalizedChatRequest,
    context: crate::router::context::RoutingContext,
) -> Result<impl Stream<Item = Result<Event, Infallible>>, ApiError> {
    let started_at = Instant::now();
    let request_id = context.public_request_id.clone();
    let config = state.config();
    let _registry = &state.provider_registry;
    let prepared = crate::router::planner::prepare_chat_plan(
        &state,
        &request,
        &context,
        crate::router::planner::PlanMode::Execute,
    )
    .await?;
    let idle_ttl =
        std::time::Duration::from_secs(state.config().routing.agent.session_idle_ttl_seconds);
    let pin_tier = prepared
        .decision
        .as_ref()
        .map(|decision| decision.tier_value);
    let pin_revision = prepared
        .decision
        .as_ref()
        .map(|decision| decision.revision.clone());
    let stream_pin = match (prepared.lease, pin_tier, pin_revision) {
        (Some(lease), Some(tier), Some(revision)) => Some(crate::router::affinity::StreamPin::new(
            lease, tier, revision, idle_ttl,
        )),
        _ => None,
    };
    if let Some(decision) = &prepared.decision {
        crate::metrics::prometheus::record_agent_decision(
            &decision.source,
            &decision.tier,
            &decision.phase,
        );
        crate::metrics::prometheus::record_pin_outcome(&decision.pin);
        crate::observability::record_agent_decision(&state, &request_id, decision).await;
    }
    let lock_candidate = prepared
        .decision
        .as_ref()
        .is_some_and(|decision| decision.lock_candidate);
    let plan = prepared.attempts;
    let resolved_models = prepared.resolved_models;
    let resolved_model_label = resolved_models.join(", ");
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
        let _ = crate::usage::accounting::record_failure(
            &state,
            crate::usage::accounting::FailureRecord {
                request_id: &request_id,
                endpoint_kind: "chat",
                requested_model: &request.model,
                selected_provider_id: None,
                selected_model_id: None,
                status: "route_exhausted",
                http_status: 503,
                latency_ms: started_at.elapsed().as_millis() as i64,
                streaming: true,
                error_code: Some("route_exhausted"),
                error_summary: Some("no available providers for requested streaming model"),
            },
        )
        .await;
        return Err(ApiError::RouteExhausted(format!(
            "No available providers for streaming model(s): {:?}",
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
        let _ = crate::usage::accounting::record_failure(
            &state,
            crate::usage::accounting::FailureRecord {
                request_id: &request_id,
                endpoint_kind: "chat",
                requested_model: &request.model,
                selected_provider_id: None,
                selected_model_id: Some(&resolved_model_label),
                status: "route_exhausted",
                http_status: 503,
                latency_ms: started_at.elapsed().as_millis() as i64,
                streaming: true,
                error_code: Some("route_exhausted"),
                error_summary: Some("all streaming providers were filtered by routing policy"),
            },
        )
        .await;
        return Err(ApiError::RouteExhausted(format!(
            "All providers for streaming model(s) '{:?}' are unavailable, disabled, or paid fallback is disabled",
            resolved_models
        )));
    }

    info!(
        request_model = %request.model,
        resolved_models = ?resolved_models,
        plan = ?plan.iter().map(|p| p.label()).collect::<Vec<_>>(),
        "Stream route plan built"
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

    // Create a channel for streaming events
    let (tx, mut rx) = tokio::sync::mpsc::channel::<StreamEvent>(256);

    // Try providers in order
    let config_clone = config.clone();
    let registry_clone = state.provider_registry.clone();
    let request_clone = request.clone();
    let usage_context: Arc<Mutex<Option<StreamUsageContext>>> = Arc::new(Mutex::new(None));
    let task_usage_context = usage_context.clone();
    let usage_state = state.clone();
    let task_request_id = request_id.clone();
    let task_started_at = started_at;

    let stream_decision = prepared.decision.clone();
    let adaptive = stream_decision.is_some();
    let budget_project_id = context.project.project_id.clone();
    let budget_key_prefix = context.project.api_key_prefix.clone();
    let budget_estimate = prepared.token_estimate;
    let budget_hold = std::sync::Arc::new(tokio::sync::Mutex::new(
        None::<crate::router::admission::PaidHold>,
    ));
    let task_budget_hold = budget_hold.clone();
    tokio::spawn(async move {
        let mut affinity_pin = stream_pin;
        let prompt_size_hint = request_clone.prompt_size_hint();
        let mut terminal_error = None;
        let mut terminal_failure_code = if lock_candidate {
            "affinity_target_unavailable"
        } else {
            "route_exhausted"
        };
        let mut terminal_failure_summary =
            "all planned streaming attempts were unavailable".to_string();
        let mut budget_http_status = 429i64;
        'attempts: for attempt in &plan {
            let provider_id = &attempt.provider_id;
            let model_id = &attempt.model_id;
            let pre_content_timeout = Duration::from_millis(stream_first_content_timeout_ms(
                &config_clone,
                &request_clone.model,
                attempt,
            ));
            if should_skip_for_context_hint(&state, attempt, prompt_size_hint) {
                crate::observability::record_skip(
                    &state,
                    &task_request_id,
                    "chat",
                    attempt,
                    "recent context budget failure hint",
                )
                .await;
                continue;
            }
            if should_skip_for_stream_silence_hint_with_alternative(
                &state,
                attempt,
                prompt_size_hint,
                plan.len(),
            ) {
                crate::observability::record_skip(
                    &state,
                    &task_request_id,
                    "chat",
                    attempt,
                    "recent stream silence hint",
                )
                .await;
                continue;
            }
            if should_skip_for_rate_limit_hint(&state, attempt) {
                crate::observability::record_skip(
                    &state,
                    &task_request_id,
                    "chat",
                    attempt,
                    "recent rate limit hint",
                )
                .await;
                continue;
            }

            if let Some(breaker) = state.breaker_states.get(provider_id) {
                if breaker.is_open() {
                    warn!(
                        provider = %provider_id,
                        model = %model_id,
                        "Skipping streaming: circuit breaker open"
                    );
                    crate::observability::record_skip(
                        &state,
                        &task_request_id,
                        "chat",
                        attempt,
                        "circuit breaker open",
                    )
                    .await;
                    continue;
                }
            }

            let adapter = match registry_clone.get(provider_id).await {
                Some(a) => a,
                None => {
                    crate::observability::record_skip(
                        &state,
                        &task_request_id,
                        "chat",
                        attempt,
                        "provider adapter missing from registry",
                    )
                    .await;
                    continue;
                }
            };

            let provider_cfg = match config_clone.providers.iter().find(|p| p.id == *provider_id) {
                Some(c) => c.clone(),
                None => {
                    crate::observability::record_skip(
                        &state,
                        &task_request_id,
                        "chat",
                        attempt,
                        "provider config missing",
                    )
                    .await;
                    continue;
                }
            };

            let ctx = ProviderContext {
                base_url: adapter.base_url(&provider_cfg),
                api_key: provider_cfg.api_key.clone(),
                config: std::sync::Arc::new(provider_cfg.clone()),
                client: state.http_client.clone(),
                // Let the route-level watchdog win the race so failures retain the
                // model-specific `stream_timeout_pre_content` classification.
                request_timeout: pre_content_timeout.saturating_add(Duration::from_secs(5)),
            };
            {
                let mut guard = task_usage_context.lock().await;
                *guard = Some(StreamUsageContext {
                    provider_id: provider_id.clone(),
                    requested_model: request_clone.model.clone(),
                    free_tier: provider_cfg.free_only,
                    started_at: Instant::now(),
                });
            }

            if let Some(previous) = task_budget_hold.lock().await.take() {
                drop(previous);
            }
            match crate::router::engine::begin_inference_hold(
                &state,
                adaptive,
                &budget_project_id,
                &budget_key_prefix,
                &task_request_id,
                provider_id,
                model_id,
                &request_clone.model,
                provider_cfg.free_only,
                &budget_estimate,
            )
            .await
            {
                Ok(hold) => *task_budget_hold.lock().await = hold,
                Err(error) => {
                    terminal_failure_code = "budget_denied";
                    terminal_failure_summary = error.to_string();
                    if let crate::api::error::ApiError::AgentRouting { http_status, .. } = &error {
                        budget_http_status = i64::from(*http_status);
                    }
                    crate::observability::record_skip(
                        &state,
                        &task_request_id,
                        "chat",
                        attempt,
                        "budget_denied",
                    )
                    .await;
                    if lock_candidate {
                        break 'attempts;
                    }
                    continue;
                }
            }

            let (attempt_tx, mut attempt_rx) = tokio::sync::mpsc::channel::<StreamEvent>(256);
            let attempt_request = NormalizedChatRequest {
                model: attempt.model_id.clone(),
                ..request_clone.clone()
            };
            let recovery_request = attempt_request.clone();
            info!(
                provider = %provider_id,
                model = %model_id,
                timeout_ms = pre_content_timeout.as_millis(),
                "Starting streaming attempt"
            );
            crate::observability::record_attempt_started(&state, &task_request_id, "chat", attempt)
                .await;
            let attempt_started_at = Instant::now();
            let attempt_ctx = ctx.clone();
            let attempt_adapter = adapter.clone();
            let mut attempt_task = tokio::spawn(async move {
                attempt_adapter
                    .stream_chat_completions(&attempt_ctx, attempt_request, attempt_tx)
                    .await
            });
            let mut buffered = Vec::new();
            let mut forwarded_meaningful_event = false;
            let mut received_upstream_event = false;
            let mut ended_empty = false;
            let first_content_timeout = tokio::time::sleep(pre_content_timeout);
            tokio::pin!(first_content_timeout);

            loop {
                tokio::select! {
                biased;
                event = attempt_rx.recv() => {
                    let Some(event) = event else {
                        match attempt_task.await {
                                Ok(Ok(())) if forwarded_meaningful_event => {
                                    info!(
                                        provider = %provider_id,
                                        model = %model_id,
                                        "Streaming completed"
                                    );
                                    crate::observability::record_attempt_result(
                                        &state,
                                        &task_request_id,
                                        "chat",
                                        attempt,
                                        "stream_completed",
                                        Some(attempt_started_at.elapsed().as_millis() as i64),
                                        None,
                                    )
                                    .await;
                                    let _ = tx.send(StreamEvent::Done).await;
                                    mark_stream_complete(&state, &task_request_id, &mut affinity_pin, stream_decision.as_ref()).await;
                                    return;
                                }
                                Ok(Ok(())) => {
                                    ended_empty = true;
                                    warn!(
                                        provider = %provider_id,
                                        model = %model_id,
                                        "Streaming attempt completed without content; trying next planned attempt"
                                    );
                                    crate::observability::record_attempt_result(
                                        &state,
                                        &task_request_id,
                                        "chat",
                                        attempt,
                                        "empty_stream",
                                        Some(attempt_started_at.elapsed().as_millis() as i64),
                                        Some("completed without content"),
                                    )
                                    .await;
                                    terminal_failure_code = "empty_stream";
                                    terminal_failure_summary = "upstream completed without content".to_string();
                                }
                                Ok(Err(e)) => {
                                    warn!(
                                        provider = %provider_id,
                                        model = %model_id,
                                        error = %e,
                                        "Streaming attempt failed before content; trying next planned attempt"
                                    );
                                    if e.is_negative_context_budget_error() {
                                        record_context_failure_hint(
                                            &state,
                                            provider_id,
                                            model_id,
                                            prompt_size_hint,
                                        );
                                    }
                                    if let ProviderError::RateLimited { retry_after, .. } = &e {
                                        record_rate_limit_hint(
                                            &state,
                                            provider_id,
                                            model_id,
                                            *retry_after,
                                        );
                                    }
                                    record_streaming_provider_error(&state, provider_id, &e).await;
                                    crate::observability::record_attempt_result(
                                        &state,
                                        &task_request_id,
                                        "chat",
                                        attempt,
                                        if forwarded_meaningful_event {
                                            "stream_failed_after_content"
                                        } else {
                                            "stream_failed_pre_content"
                                        },
                                        Some(attempt_started_at.elapsed().as_millis() as i64),
                                        Some(&e.to_string()),
                                    )
                                    .await;
                                    terminal_failure_code = match &e {
                                        ProviderError::RateLimited { .. } => "rate_limited",
                                        ProviderError::QuotaExhausted { .. } => "quota_exhausted",
                                        _ => "stream_failed_pre_content",
                                    };
                                    terminal_failure_summary = e.to_string();
                                    terminal_error = Some(e);
                                    if forwarded_meaningful_event {
                                        break 'attempts;
                                    }
                                }
                                Err(e) => {
                                    warn!(
                                        provider = %provider_id,
                                        model = %model_id,
                                        error = %e,
                                        "Streaming attempt task failed before content; trying next planned attempt"
                                    );
                                    crate::resilience::health::record_failure(&state, provider_id).await;
                                    crate::observability::record_attempt_result(
                                        &state,
                                        &task_request_id,
                                        "chat",
                                        attempt,
                                        "stream_task_failed",
                                        Some(attempt_started_at.elapsed().as_millis() as i64),
                                        Some(&e.to_string()),
                                    )
                                    .await;
                                    terminal_failure_code = "stream_task_failed";
                                    terminal_failure_summary = e.to_string();
                                }
                            }
                        break;
                    };

                    if !received_upstream_event {
                        received_upstream_event = true;
                        record_stream_timing_async(
                            &state,
                            &task_request_id,
                            provider_id,
                            model_id,
                            "stream_first_event",
                            "received",
                            attempt_started_at.elapsed().as_millis() as i64,
                        );
                    }

                    if forwarded_meaningful_event {
                        observe_forwarded_pin(&mut affinity_pin, provider_id, model_id, &event);
                        let done = matches!(event, StreamEvent::Done);
                        if done {
                            mark_stream_complete(&state, &task_request_id, &mut affinity_pin, stream_decision.as_ref()).await;
                        }
                        let _ = tx.send(event).await;
                        if done {
                            info!(
                                provider = %provider_id,
                                model = %model_id,
                                "Streaming completed"
                            );
                            crate::observability::record_attempt_result(
                                &state,
                                &task_request_id,
                                "chat",
                                attempt,
                                "stream_completed",
                                Some(attempt_started_at.elapsed().as_millis() as i64),
                                None,
                            )
                            .await;
                            return;
                        }
                        continue;
                    }

                    if stream_event_has_content(&event) {
                        forwarded_meaningful_event = true;
                        observe_forwarded_pin(&mut affinity_pin, provider_id, model_id, &event);
                        record_stream_timing_async(
                            &state,
                            &task_request_id,
                            provider_id,
                            model_id,
                            "stream_first_content",
                            "forwarded",
                            attempt_started_at.elapsed().as_millis() as i64,
                        );
                        for buffered_event in buffered.drain(..) {
                            let _ = tx.send(buffered_event).await;
                        }
                        let _ = tx.send(event).await;
                    } else if matches!(event, StreamEvent::Done) {
                            ended_empty = true;
                            warn!(
                                provider = %provider_id,
                                model = %model_id,
                                "Streaming attempt ended without content; trying next planned attempt"
                            );
                            record_stream_silence_hint(
                                &state,
                                provider_id,
                                model_id,
                                prompt_size_hint,
                            );
                            crate::observability::record_attempt_result(
                                &state,
                                &task_request_id,
                                "chat",
                                attempt,
                                "empty_stream",
                                Some(attempt_started_at.elapsed().as_millis() as i64),
                                Some("ended without content"),
                            )
                            .await;
                            terminal_failure_code = "empty_stream";
                            terminal_failure_summary = "upstream ended without content".to_string();
                            attempt_task.abort();
                            break;
                        } else {
                        buffered.push(event);
                    }
                }
                result = &mut attempt_task => {
                    match result {
                        Ok(Ok(())) if forwarded_meaningful_event => {
                            info!(
                                provider = %provider_id,
                                model = %model_id,
                                "Streaming completed"
                            );
                            crate::observability::record_attempt_result(
                                &state,
                                &task_request_id,
                                "chat",
                                attempt,
                                "stream_completed",
                                Some(attempt_started_at.elapsed().as_millis() as i64),
                                None,
                            )
                            .await;
                            let _ = tx.send(StreamEvent::Done).await;
                            mark_stream_complete(&state, &task_request_id, &mut affinity_pin, stream_decision.as_ref()).await;
                            return;
                        }
                        Ok(Ok(())) => {
                            ended_empty = true;
                            warn!(
                                provider = %provider_id,
                                model = %model_id,
                                "Streaming attempt completed without content; trying next planned attempt"
                            );
                            crate::observability::record_attempt_result(
                                &state,
                                &task_request_id,
                                "chat",
                                attempt,
                                "empty_stream",
                                Some(attempt_started_at.elapsed().as_millis() as i64),
                                Some("completed without content"),
                            )
                            .await;
                            terminal_failure_code = "empty_stream";
                            terminal_failure_summary = "upstream completed without content".to_string();
                        }
                        Ok(Err(e)) => {
                            warn!(
                                provider = %provider_id,
                                model = %model_id,
                                error = %e,
                                "Streaming attempt failed before content; trying next planned attempt"
                            );
                            if e.is_negative_context_budget_error() {
                                record_context_failure_hint(
                                    &state,
                                    provider_id,
                                    model_id,
                                    prompt_size_hint,
                                );
                            }
                            if let ProviderError::RateLimited { retry_after, .. } = &e {
                                record_rate_limit_hint(
                                    &state,
                                    provider_id,
                                    model_id,
                                    *retry_after,
                                );
                            }
                            record_streaming_provider_error(&state, provider_id, &e).await;
                            crate::observability::record_attempt_result(
                                &state,
                                &task_request_id,
                                "chat",
                                attempt,
                                if forwarded_meaningful_event {
                                    "stream_failed_after_content"
                                } else {
                                    "stream_failed_pre_content"
                                },
                                Some(attempt_started_at.elapsed().as_millis() as i64),
                                Some(&e.to_string()),
                            )
                            .await;
                            terminal_failure_code = match &e {
                                ProviderError::RateLimited { .. } => "rate_limited",
                                ProviderError::QuotaExhausted { .. } => "quota_exhausted",
                                _ => "stream_failed_pre_content",
                            };
                            terminal_failure_summary = e.to_string();
                            terminal_error = Some(e);
                            if forwarded_meaningful_event {
                                break 'attempts;
                            }
                        }
                        Err(e) => {
                            warn!(
                                provider = %provider_id,
                                model = %model_id,
                                error = %e,
                                "Streaming attempt task failed before content; trying next planned attempt"
                            );
                            crate::resilience::health::record_failure(&state, provider_id).await;
                            crate::observability::record_attempt_result(
                                &state,
                                &task_request_id,
                                "chat",
                                attempt,
                                "stream_task_failed",
                                Some(attempt_started_at.elapsed().as_millis() as i64),
                                Some(&e.to_string()),
                            )
                            .await;
                            terminal_failure_code = "stream_task_failed";
                            terminal_failure_summary = e.to_string();
                        }
                    }
                    break;
                }
                _ = &mut first_content_timeout, if !forwarded_meaningful_event => {
                        warn!(
                            provider = %provider_id,
                            model = %model_id,
                            timeout_ms = pre_content_timeout.as_millis(),
                            "Streaming attempt timed out before content; trying next planned attempt"
                        );
                        record_stream_silence_hint(
                            &state,
                            provider_id,
                            model_id,
                            prompt_size_hint,
                        );
                        crate::observability::record_attempt_result(
                            &state,
                            &task_request_id,
                            "chat",
                            attempt,
                            "stream_timeout_pre_content",
                            Some(pre_content_timeout.as_millis() as i64),
                            Some("timed out before content"),
                        )
                        .await;
                        terminal_failure_code = "stream_timeout_pre_content";
                        terminal_failure_summary = format!(
                            "timed out before content after {} ms",
                            pre_content_timeout.as_millis()
                        );
                        attempt_task.abort();
                        break;
                    }
                }
            }

            if ended_empty && config_clone.routing.recover_empty_stream_with_non_streaming {
                info!(
                    provider = %provider_id,
                    model = %model_id,
                    "Retrying empty upstream stream as a non-streaming request"
                );
                let recovery_started_at = Instant::now();
                if let Some(previous) = task_budget_hold.lock().await.take() {
                    drop(previous);
                }
                match crate::router::engine::begin_inference_hold(
                    &state,
                    adaptive,
                    &budget_project_id,
                    &budget_key_prefix,
                    &task_request_id,
                    provider_id,
                    model_id,
                    &request_clone.model,
                    provider_cfg.free_only,
                    &budget_estimate,
                )
                .await
                {
                    Ok(hold) => *task_budget_hold.lock().await = hold,
                    Err(error) => {
                        terminal_failure_code = "budget_denied";
                        terminal_failure_summary = error.to_string();
                        if let crate::api::error::ApiError::AgentRouting { http_status, .. } =
                            &error
                        {
                            budget_http_status = i64::from(*http_status);
                        }
                        break 'attempts;
                    }
                }
                match adapter.chat_completions(&ctx, recovery_request).await {
                    Ok(response) => {
                        if let Some(events) = recovered_non_stream_events(response) {
                            crate::observability::record_attempt_result(
                                &state,
                                &task_request_id,
                                "chat",
                                attempt,
                                "stream_recovered_non_streaming",
                                Some(recovery_started_at.elapsed().as_millis() as i64),
                                None,
                            )
                            .await;
                            info!(
                                provider = %provider_id,
                                model = %model_id,
                                "Recovered empty upstream stream with non-streaming response"
                            );
                            for event in events {
                                observe_forwarded_pin(
                                    &mut affinity_pin,
                                    provider_id,
                                    model_id,
                                    &event,
                                );
                                let _ = tx.send(event).await;
                            }
                            mark_stream_complete(
                                &state,
                                &task_request_id,
                                &mut affinity_pin,
                                stream_decision.as_ref(),
                            )
                            .await;
                            return;
                        }
                        warn!(
                            provider = %provider_id,
                            model = %model_id,
                            "Non-streaming recovery also completed without content"
                        );
                        crate::observability::record_attempt_result(
                            &state,
                            &task_request_id,
                            "chat",
                            attempt,
                            "empty_stream_recovery_empty",
                            Some(recovery_started_at.elapsed().as_millis() as i64),
                            Some("non-streaming recovery completed without content"),
                        )
                        .await;
                    }
                    Err(error) => {
                        warn!(
                            provider = %provider_id,
                            model = %model_id,
                            error = %error,
                            "Non-streaming recovery failed"
                        );
                        record_streaming_provider_error(&state, provider_id, &error).await;
                        crate::observability::record_attempt_result(
                            &state,
                            &task_request_id,
                            "chat",
                            attempt,
                            "empty_stream_recovery_failed",
                            Some(recovery_started_at.elapsed().as_millis() as i64),
                            Some(&error.to_string()),
                        )
                        .await;
                        terminal_error = Some(error);
                    }
                }
            }
        }

        // All providers failed pre-stream or an upstream ended an active stream with an error.
        // The response consumer settles a successful usage event and retains a hold
        // that never received one, including when this task already failed.
        let (failure_status, failure_http_status, failure_outcome) = if terminal_failure_code
            == "budget_denied"
        {
            ("budget_denied", budget_http_status, "budget_denied")
        } else {
            match terminal_error.as_ref() {
                Some(ProviderError::RateLimited { .. }) => ("rate_limited", 429, "rate_limited"),
                Some(ProviderError::QuotaExhausted { .. }) => {
                    ("quota_exhausted", 429, "quota_exhausted")
                }
                _ => ("route_exhausted", 503, "route_exhausted"),
            }
        };
        let safe_terminal_failure_summary =
            crate::util::redact::redact_config_secrets(&config_clone, &terminal_failure_summary);
        let _ = crate::usage::accounting::record_failure(
            &state,
            crate::usage::accounting::FailureRecord {
                request_id: &task_request_id,
                endpoint_kind: "chat",
                requested_model: &request_clone.model,
                selected_provider_id: plan.last().map(|attempt| attempt.provider_id.as_str()),
                selected_model_id: plan.last().map(|attempt| attempt.model_id.as_str()),
                status: failure_status,
                http_status: failure_http_status,
                latency_ms: task_started_at.elapsed().as_millis() as i64,
                streaming: true,
                error_code: Some(terminal_failure_code),
                error_summary: Some(&safe_terminal_failure_summary),
            },
        )
        .await;
        crate::observability::record_event(
            &state,
            crate::observability::TraceEventRecord {
                request_id: &task_request_id,
                event_type: "route_exhausted",
                provider_id: plan.last().map(|attempt| attempt.provider_id.as_str()),
                model_id: plan.last().map(|attempt| attempt.model_id.as_str()),
                outcome: Some(failure_outcome),
                latency_ms: Some(task_started_at.elapsed().as_millis() as i64),
                details: serde_json::json!({
                    "endpoint_kind": "chat",
                    "streaming": true,
                    "error_code": terminal_failure_code,
                    "error_summary": crate::observability::short_error(&safe_terminal_failure_summary),
                }),
            },
        )
        .await;
        if let Some(error) = terminal_error.as_ref() {
            let _ = tx.send(stream_error_event(error, &config_clone)).await;
        } else {
            let _ = tx
                .send(StreamEvent::Error {
                    message: safe_terminal_failure_summary,
                    error_type: "upstream_error".into(),
                    code: terminal_failure_code.into(),
                })
                .await;
        }
        let _ = tx.send(StreamEvent::Done).await;
    });

    // Convert channel receiver into an SSE stream
    let mut usage_recorded = false;
    let stream = async_stream::stream! {
        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::Done => {
                    if let Some(hold) = budget_hold.lock().await.take() {
                        drop(hold);
                    }
                    yield Ok(Event::default().data("[DONE]"));
                    break;
                }
                StreamEvent::Usage {
                    id,
                    created,
                    model,
                    prompt_tokens,
                    completion_tokens,
                    total_tokens,
                } => {
                    if !usage_recorded {
                        usage_recorded = true;
                        if let Some(ctx) = usage_context.lock().await.clone() {
                            let usage = UsageResponse {
                                prompt_tokens,
                                completion_tokens,
                                total_tokens,
                                prompt_cache_hit_tokens: None,
                                prompt_cache_miss_tokens: None,
                                reasoning_tokens: None,
                            };
                            let recorded = crate::usage::accounting::record_usage(
                                &usage_state,
                                crate::usage::accounting::UsageRecord {
                                    provider_id: &ctx.provider_id,
                                    model_id: &model,
                                    requested_model: &ctx.requested_model,
                                    usage: Some(&usage),
                                    latency_ms: ctx.started_at.elapsed().as_millis() as i64,
                                    free_tier: ctx.free_tier,
                                    request_id: &request_id,
                                    endpoint_kind: "chat",
                                    streaming: true,
                                },
                            )
                            .await;
                            if let Err(error) = &recorded {
                                warn!(%error, "Failed to record streaming usage");
                            } else if let Some(hold) = budget_hold.lock().await.take() {
                                hold.settle().await;
                            }
                        } else {
                            warn!("Streaming usage event received before provider context was set");
                        }
                    }
                    yield Ok(Event::default().data(format_sse_payload(&StreamEvent::Usage {
                        id,
                        created,
                        model,
                        prompt_tokens,
                        completion_tokens,
                        total_tokens,
                    })));
                }
                _ => {
                    yield Ok(Event::default().data(format_sse_payload(&event)));
                }
            }
        }
    };

    Ok(stream)
}

async fn record_streaming_provider_error(
    state: &AppState,
    provider_id: &str,
    error: &ProviderError,
) {
    if crate::resilience::health::should_record_provider_failure(error) {
        crate::resilience::health::record_failure(state, provider_id).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::openai::chat::{ProviderUsage, ToolCall, ToolCallFunction};

    fn attempt() -> RouteAttempt {
        RouteAttempt {
            provider_id: "openrouter".into(),
            model_id: "stealth/ox-alpha".into(),
            priority: 0,
        }
    }

    #[test]
    fn ox_alpha_uses_longer_builtin_first_content_timeout() {
        assert_eq!(
            stream_first_content_timeout_ms(&Config::default(), "preview:ox-alpha", &attempt()),
            180_000
        );
    }

    #[test]
    fn timeout_resolution_prefers_requested_group_then_qualified_and_model_keys() {
        let mut config = Config::default();
        config.server.request_timeout_ms = 12_000;
        config.routing.stream_first_content_timeout_ms = std::collections::HashMap::from([
            ("preview:ox-alpha".into(), 30_000),
            ("openrouter/stealth/ox-alpha".into(), 20_000),
            ("stealth/ox-alpha".into(), 10_000),
        ]);
        assert_eq!(
            stream_first_content_timeout_ms(&config, "preview:ox-alpha", &attempt()),
            30_000
        );
        config
            .routing
            .stream_first_content_timeout_ms
            .remove("preview:ox-alpha");
        assert_eq!(
            stream_first_content_timeout_ms(&config, "preview:ox-alpha", &attempt()),
            20_000
        );
    }

    #[test]
    fn timeout_resolution_falls_back_to_global_and_enforces_minimum() {
        let mut config = Config::default();
        config.routing.stream_first_content_timeout_ms.clear();
        config.server.request_timeout_ms = 500;
        assert_eq!(
            stream_first_content_timeout_ms(&config, "other", &attempt()),
            1_000
        );
    }

    #[test]
    fn non_stream_recovery_requires_content_or_tool_calls() {
        let response = ProviderChatResponse {
            provider_id: "openrouter".into(),
            model_id: "stealth/ox-alpha".into(),
            content: None,
            tool_calls: None,
            finish_reason: Some("stop".into()),
            usage: None,
            latency_ms: 10,
        };
        assert!(recovered_non_stream_events(response).is_none());
    }

    #[test]
    fn non_stream_recovery_translates_text_usage_and_done() {
        let response = ProviderChatResponse {
            provider_id: "openrouter".into(),
            model_id: "stealth/ox-alpha".into(),
            content: Some("recovered".into()),
            tool_calls: None,
            finish_reason: Some("stop".into()),
            usage: Some(ProviderUsage {
                prompt_tokens: 10,
                completion_tokens: 2,
                total_tokens: 12,
                prompt_cache_hit_tokens: None,
                prompt_cache_miss_tokens: None,
                reasoning_tokens: None,
            }),
            latency_ms: 10,
        };
        let events = recovered_non_stream_events(response).unwrap();
        assert!(matches!(
            &events[0],
            StreamEvent::Chunk { delta, finish_reason, .. }
                if delta.content.as_deref() == Some("recovered")
                    && finish_reason.as_deref() == Some("stop")
        ));
        assert!(matches!(
            events[1],
            StreamEvent::Usage {
                total_tokens: 12,
                ..
            }
        ));
        assert!(matches!(events[2], StreamEvent::Done));
    }

    #[test]
    fn non_stream_recovery_preserves_tool_call_indexes() {
        let response = ProviderChatResponse {
            provider_id: "openrouter".into(),
            model_id: "stealth/ox-alpha".into(),
            content: None,
            tool_calls: Some(vec![
                ToolCall {
                    id: "call-0".into(),
                    call_type: "function".into(),
                    function: ToolCallFunction {
                        name: "first".into(),
                        arguments: "{}".into(),
                    },
                },
                ToolCall {
                    id: "call-1".into(),
                    call_type: "function".into(),
                    function: ToolCallFunction {
                        name: "second".into(),
                        arguments: "{}".into(),
                    },
                },
            ]),
            finish_reason: Some("tool_calls".into()),
            usage: None,
            latency_ms: 10,
        };
        let events = recovered_non_stream_events(response).unwrap();
        let payload = format_sse_payload(&events[1]);
        let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(payload["choices"][0]["index"], 0);
        assert_eq!(payload["choices"][0]["delta"]["tool_calls"][0]["index"], 1);
        assert!(matches!(events.last(), Some(StreamEvent::Done)));
    }
}
