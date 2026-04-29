use crate::api::error::ApiError;
use crate::api::openai::chat::NormalizedChatRequest;
use crate::api::openai::chat::StreamDelta;
use crate::app::state::AppState;
use crate::providers::traits::{EndpointKind, ProviderContext};
use crate::router::policy::RoutePolicy;
use crate::router::selection::build_attempt_plan;
use axum::response::sse::Event;
use futures::stream::Stream;
use std::convert::Infallible;
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

/// Create a streaming SSE response for a chat completion request.
/// Uses the routing engine to find a provider, then streams from it.
pub async fn create_chat_stream(
    state: AppState,
    request: NormalizedChatRequest,
) -> Result<impl Stream<Item = Result<Event, Infallible>>, ApiError> {
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
            "No available providers for streaming model: {}",
            resolved_model
        )));
    }

    info!(
        request_model = %request.model,
        resolved_model = %resolved_model,
        plan = ?plan.iter().map(|p| &p.provider_id).collect::<Vec<_>>(),
        "Stream route plan built"
    );

    // Create a channel for streaming events
    let (tx, mut rx) = tokio::sync::mpsc::channel::<StreamEvent>(256);

    // Try providers in order
    let config_clone = config.clone();
    let registry_clone = state.provider_registry.clone();
    let request_clone = request.clone();

    tokio::spawn(async move {
        // Per spec: no mid-stream fallback. Only try the first healthy provider.
        // If streaming fails, send Done and stop — don't try next provider.
        for attempt in &plan {
            let provider_id = &attempt.provider_id;

            if let Some(breaker) = state.breaker_states.get(provider_id) {
                if breaker.is_open() {
                    warn!(provider = %provider_id, "Skipping streaming: circuit breaker open");
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
                config: std::sync::Arc::new(provider_cfg),
                client: state.http_client.clone(),
            };

            match adapter
                .stream_chat_completions(
                    &ctx,
                    NormalizedChatRequest {
                        model: attempt.model_id.clone(),
                        ..request_clone.clone()
                    },
                    tx.clone(),
                )
                .await
            {
                Ok(()) => {
                    info!(provider = %provider_id, "Streaming completed");
                    return;
                }
                Err(e) => {
                    warn!(provider = %provider_id, error = %e, "Stream failed, no fallback per spec");
                    crate::resilience::health::record_failure(&state, provider_id).await;
                    // Don't try next provider — mid-stream fallback prohibited
                }
            }
        }

        // All providers failed pre-stream or streaming failed
        let _ = tx.send(StreamEvent::Done).await;
    });

    // Convert channel receiver into an SSE stream
    let stream = async_stream::stream! {
        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::Done => {
                    yield Ok(Event::default().data("[DONE]"));
                    break;
                }
                _ => {
                    yield Ok(Event::default().data(format_sse_payload(&event)));
                }
            }
        }
    };

    Ok(stream)
}
