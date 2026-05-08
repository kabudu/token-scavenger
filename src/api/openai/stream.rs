use crate::api::error::ApiError;
use crate::api::openai::chat::NormalizedChatRequest;
use crate::api::openai::chat::StreamDelta;
use crate::api::openai::chat::UsageResponse;
use crate::app::state::AppState;
use crate::providers::traits::{EndpointKind, ProviderContext};
use crate::router::policy::RoutePolicy;
use crate::router::selection::{
    build_attempt_plan, filter_by_health, filter_by_model_enabled, filter_by_paid_policy,
    prioritize_for_tool_use,
};
use axum::response::sse::Event;
use futures::stream::Stream;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Instant;
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
    /// Done sentinel.
    Done,
}

use serde::Serialize;

#[derive(Debug, Clone)]
struct StreamUsageContext {
    provider_id: String,
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
                "index": index,
                "delta": {
                    "tool_calls": [{
                        "index": 0,
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
        StreamEvent::Done => "[DONE]".to_string(),
    }
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
        StreamEvent::Usage { .. } | StreamEvent::Done => false,
    }
}

/// Create a streaming SSE response for a chat completion request.
/// Uses the routing engine to find a provider, then streams from it.
pub async fn create_chat_stream(
    state: AppState,
    request: NormalizedChatRequest,
    request_id: String,
) -> Result<impl Stream<Item = Result<Event, Infallible>>, ApiError> {
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
        return Err(ApiError::RouteExhausted(format!(
            "No available providers for streaming model(s): {:?}",
            resolved_models
        )));
    }

    let mut plan = filter_by_model_enabled(
        filter_by_paid_policy(filter_by_health(plan, &state), &state),
        &state,
    )
    .await;
    if request.tools.is_some() {
        plan = prioritize_for_tool_use(plan, &state).await;
    }

    if plan.is_empty() {
        return Err(ApiError::RouteExhausted(format!(
            "All providers for streaming model(s) '{:?}' are unavailable, disabled, or paid fallback is disabled",
            resolved_models
        )));
    }

    info!(
        request_model = %request.model,
        resolved_models = ?resolved_models,
        plan = ?plan.iter().map(|p| &p.provider_id).collect::<Vec<_>>(),
        "Stream route plan built"
    );

    // Create a channel for streaming events
    let (tx, mut rx) = tokio::sync::mpsc::channel::<StreamEvent>(256);

    // Try providers in order
    let config_clone = config.clone();
    let registry_clone = state.provider_registry.clone();
    let request_clone = request.clone();
    let usage_context: Arc<Mutex<Option<StreamUsageContext>>> = Arc::new(Mutex::new(None));
    let task_usage_context = usage_context.clone();
    let usage_state = state.clone();

    tokio::spawn(async move {
        for attempt in &plan {
            let provider_id = &attempt.provider_id;
            let model_id = &attempt.model_id;

            if let Some(breaker) = state.breaker_states.get(provider_id) {
                if breaker.is_open() {
                    warn!(
                        provider = %provider_id,
                        model = %model_id,
                        "Skipping streaming: circuit breaker open"
                    );
                    continue;
                }
            }

            let adapter = match registry_clone.get(provider_id).await {
                Some(a) => a,
                None => continue,
            };

            let provider_cfg = match config_clone.providers.iter().find(|p| p.id == *provider_id) {
                Some(c) => c.clone(),
                None => continue,
            };

            let ctx = ProviderContext {
                base_url: adapter.base_url(&provider_cfg),
                api_key: provider_cfg.api_key.clone(),
                config: std::sync::Arc::new(provider_cfg.clone()),
                client: state.http_client.clone(),
            };
            {
                let mut guard = task_usage_context.lock().await;
                *guard = Some(StreamUsageContext {
                    provider_id: provider_id.clone(),
                    free_tier: provider_cfg.free_only,
                    started_at: Instant::now(),
                });
            }

            let (attempt_tx, mut attempt_rx) = tokio::sync::mpsc::channel::<StreamEvent>(256);
            let attempt_request = NormalizedChatRequest {
                model: attempt.model_id.clone(),
                ..request_clone.clone()
            };
            let attempt_ctx = ctx.clone();
            let attempt_adapter = adapter.clone();
            let mut attempt_task = tokio::spawn(async move {
                attempt_adapter
                    .stream_chat_completions(&attempt_ctx, attempt_request, attempt_tx)
                    .await
            });
            let mut buffered = Vec::new();
            let mut forwarded_meaningful_event = false;

            loop {
                tokio::select! {
                    biased;
                    event = attempt_rx.recv() => {
                        let Some(event) = event else {
                            if forwarded_meaningful_event {
                                info!(
                                    provider = %provider_id,
                                    model = %model_id,
                                    "Streaming completed"
                                );
                                let _ = tx.send(StreamEvent::Done).await;
                                return;
                            }
                            warn!(
                                provider = %provider_id,
                                model = %model_id,
                                "Streaming attempt closed without content; trying next planned attempt"
                            );
                            attempt_task.abort();
                            break;
                        };

                        if forwarded_meaningful_event {
                            let done = matches!(event, StreamEvent::Done);
                            let _ = tx.send(event).await;
                            if done {
                                info!(
                                    provider = %provider_id,
                                    model = %model_id,
                                    "Streaming completed"
                                );
                                return;
                            }
                            continue;
                        }

                        if stream_event_has_content(&event) {
                            forwarded_meaningful_event = true;
                            for buffered_event in buffered.drain(..) {
                                let _ = tx.send(buffered_event).await;
                            }
                            let _ = tx.send(event).await;
                        } else if matches!(event, StreamEvent::Done) {
                            warn!(
                                provider = %provider_id,
                                model = %model_id,
                                "Streaming attempt ended without content; trying next planned attempt"
                            );
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
                                let _ = tx.send(StreamEvent::Done).await;
                                return;
                            }
                            Ok(Ok(())) => {
                                warn!(
                                    provider = %provider_id,
                                    model = %model_id,
                                    "Streaming attempt completed without content; trying next planned attempt"
                                );
                            }
                            Ok(Err(e)) => {
                                warn!(
                                    provider = %provider_id,
                                    model = %model_id,
                                    error = %e,
                                    "Streaming attempt failed before content; trying next planned attempt"
                                );
                                crate::resilience::health::record_failure(&state, provider_id).await;
                            }
                            Err(e) => {
                                warn!(
                                    provider = %provider_id,
                                    model = %model_id,
                                    error = %e,
                                    "Streaming attempt task failed before content; trying next planned attempt"
                                );
                                crate::resilience::health::record_failure(&state, provider_id).await;
                            }
                        }
                        break;
                    }
                }
            }
        }

        // All providers failed pre-stream or streaming failed
        let _ = tx.send(StreamEvent::Done).await;
    });

    // Convert channel receiver into an SSE stream
    let mut usage_recorded = false;
    let stream = async_stream::stream! {
        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::Done => {
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
                            if let Err(error) = crate::usage::accounting::record_usage(
                                &usage_state,
                                crate::usage::accounting::UsageRecord {
                                    provider_id: &ctx.provider_id,
                                    model_id: &model,
                                    usage: Some(&usage),
                                    latency_ms: ctx.started_at.elapsed().as_millis() as i64,
                                    free_tier: ctx.free_tier,
                                    request_id: &request_id,
                                    endpoint_kind: "chat",
                                    streaming: true,
                                },
                            )
                            .await
                            {
                                warn!(%error, "Failed to record streaming usage");
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
