use crate::api::openai::chat::{
    NormalizedChatRequest, ProviderChatResponse, ProviderUsage, ToolCall,
};
use crate::config::schema::ProviderConfig;
use crate::discovery::curated::DiscoveredModel;
use crate::providers::http::{ProviderHttp, bearer_auth};
use crate::providers::normalization::parse_rate_limit_headers;
use crate::providers::traits::*;
use url::Url;

/// Ensure a URL has a trailing slash so that `Url::join` appends relative paths
/// as sub-paths rather than replacing the last path segment.
/// E.g. `https://api.groq.com/openai/v1` → `https://api.groq.com/openai/v1/`
pub fn with_trailing_slash(url: &Url) -> Url {
    let s = url.as_str();
    if s.ends_with('/') {
        url.clone()
    } else {
        Url::parse(&format!("{}/", s)).expect("Appending / to valid URL should be valid")
    }
}

fn serialize_chat_messages(
    messages: &[crate::api::openai::chat::ChatMessage],
) -> Vec<serde_json::Value> {
    messages
        .iter()
        .map(|message| {
            let mut serialized = serde_json::json!({
                "role": message.role,
                "content": message.content,
            });
            if let Some(name) = message.name.as_ref() {
                serialized["name"] = serde_json::Value::String(name.clone());
            }
            if let Some(tool_call_id) = message.tool_call_id.as_ref() {
                serialized["tool_call_id"] = serde_json::Value::String(tool_call_id.clone());
            }
            if let Some(tool_calls) = message.tool_calls.as_ref() {
                serialized["tool_calls"] = serde_json::to_value(tool_calls).unwrap_or_default();
            }
            serialized
        })
        .collect()
}

fn openai_chat_body(request: &NormalizedChatRequest, stream: bool) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": request.model,
        "messages": serialize_chat_messages(&request.messages),
        "stream": stream,
    });
    if stream {
        body["stream_options"] = serde_json::json!({ "include_usage": true });
    }
    if let Some(value) = request.temperature {
        body["temperature"] = serde_json::json!(value);
    }
    if let Some(value) = request.top_p {
        body["top_p"] = serde_json::json!(value);
    }
    if let Some(value) = request.max_tokens {
        body["max_tokens"] = serde_json::json!(value);
    }
    if let Some(value) = request.stop.as_ref() {
        body["stop"] = serde_json::json!(value);
    }
    if let Some(value) = request.presence_penalty {
        body["presence_penalty"] = serde_json::json!(value);
    }
    if let Some(value) = request.frequency_penalty {
        body["frequency_penalty"] = serde_json::json!(value);
    }
    if let Some(value) = request.user.as_ref() {
        body["user"] = serde_json::json!(value);
    }
    if let Some(value) = request.response_format.as_ref() {
        body["response_format"] = serde_json::to_value(value).unwrap_or_default();
    }
    if let Some(value) = request.tools.as_ref() {
        body["tools"] = serde_json::to_value(value).unwrap_or_default();
    }
    if let Some(value) = request.tool_choice.as_ref() {
        body["tool_choice"] = value.clone();
    }
    body
}

fn should_retry_with_min_max_tokens(request: &NormalizedChatRequest, body: &str) -> bool {
    let body_lower = body.to_ascii_lowercase();
    request.max_tokens.is_none()
        && body_lower.contains("max_tokens")
        && !body_lower.contains("got -")
        && !body_lower.contains("value=-")
        && (body_lower.contains("at least 1")
            || body_lower.contains("greater than or equal to 1")
            || body_lower.contains("minimum")
            || body_lower.contains("got 0")
            || body_lower.contains("value=0"))
}

fn with_min_max_tokens(mut request: NormalizedChatRequest) -> NormalizedChatRequest {
    request.max_tokens = Some(1);
    request
}

pub fn provider_base_url(
    provider_id: &str,
    config: &ProviderConfig,
    default_base_url: &str,
) -> Url {
    if let Some(u) = config.base_url.as_deref() {
        match Url::parse(u) {
            Ok(parsed) => parsed,
            Err(err) => {
                tracing::warn!(
                    provider = %provider_id,
                    base_url = %u,
                    error = %err,
                    "Invalid provider base_url; falling back to default"
                );
                default_base_url
                    .parse()
                    .expect("default provider base_url must be valid")
            }
        }
    } else {
        default_base_url
            .parse()
            .expect("default provider base_url must be valid")
    }
}

/// Helper to execute an OpenAI-compatible chat completions request.
/// Used by providers like Groq, Cerebras, Mistral, OpenRouter, NVIDIA, etc.
pub async fn openai_chat_completions(
    ctx: &ProviderContext,
    request: NormalizedChatRequest,
    provider_id: &str,
) -> Result<ProviderChatResponse, ProviderError> {
    let url = with_trailing_slash(&ctx.base_url)
        .join("chat/completions")
        .map_err(|e| ProviderError::Other(e.to_string()))?;

    let config = ProviderConfig {
        id: provider_id.into(),
        api_key: ctx.api_key.clone(),
        ..Default::default()
    };

    let start = std::time::Instant::now();
    let body = openai_chat_body(&request, false);
    let mut resp =
        ProviderHttp::post_json(&ctx.client, url.clone(), bearer_auth(&config), &body).await?;
    let latency_ms = start.elapsed().as_millis() as i64;

    let mut rate_limits = parse_rate_limit_headers(resp.headers());
    let mut status = resp.status();
    let mut response_body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ProviderError::MalformedResponse(e.to_string()))?;

    if !status.is_success() {
        let msg = response_body
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("unknown error");
        if should_retry_with_min_max_tokens(&request, msg) {
            tracing::info!(
                provider = %provider_id,
                model = %request.model,
                "Retrying OpenAI-compatible request with max_tokens=1 after upstream rejected omitted token limit"
            );
            let retry_request = with_min_max_tokens(request.clone());
            let retry_body = openai_chat_body(&retry_request, false);
            resp = ProviderHttp::post_json(&ctx.client, url, bearer_auth(&config), &retry_body)
                .await?;
            rate_limits = parse_rate_limit_headers(resp.headers());
            status = resp.status();
            response_body = resp
                .json()
                .await
                .map_err(|e| ProviderError::MalformedResponse(e.to_string()))?;
        }
    }

    if !status.is_success() {
        let msg = response_body
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("unknown error");
        return Err(classify_error_with_rate_limits(
            status.as_u16(),
            msg,
            &rate_limits,
        ));
    }

    let model = response_body
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or(&request.model)
        .to_string();

    let choices = response_body
        .get("choices")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    let message = c.get("message")?;
                    let content = message
                        .get("content")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let role = message
                        .get("role")
                        .and_then(|v| v.as_str())
                        .unwrap_or("assistant")
                        .to_string();
                    let finish_reason = c
                        .get("finish_reason")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let tool_calls =
                        message
                            .get("tool_calls")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|tc| {
                                        let id = tc.get("id")?.as_str()?.to_string();
                                        let func = tc.get("function")?;
                                        let name = func.get("name")?.as_str()?.to_string();
                                        let args = func.get("arguments")?.as_str()?.to_string();
                                        Some(ToolCall {
                                            id,
                                            call_type: "function".to_string(),
                                            function: crate::api::openai::chat::ToolCallFunction {
                                                name,
                                                arguments: args,
                                            },
                                        })
                                    })
                                    .collect()
                            });
                    Some((content, role, finish_reason, tool_calls))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let first = choices.into_iter().next();
    let (content, _role, finish_reason, tool_calls) =
        first.unwrap_or((None, "assistant".into(), None, None));

    let usage = response_body.get("usage").map(|u| ProviderUsage {
        prompt_tokens: u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        completion_tokens: u
            .get("completion_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        total_tokens: u.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        prompt_cache_hit_tokens: u
            .get("prompt_cache_hit_tokens")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32),
        prompt_cache_miss_tokens: u
            .get("prompt_cache_miss_tokens")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32),
        reasoning_tokens: u
            .get("completion_tokens_details")
            .and_then(|details| details.get("reasoning_tokens"))
            .and_then(|v| v.as_u64())
            .map(|v| v as u32),
    });

    Ok(ProviderChatResponse {
        provider_id: provider_id.to_string(),
        model_id: model,
        content,
        tool_calls,
        finish_reason,
        usage,
        latency_ms,
    })
}

/// Helper to discover models via OpenAI-compatible /v1/models endpoint.
pub async fn openai_discover_models(
    ctx: &ProviderContext,
    provider_id: &str,
) -> Result<Vec<DiscoveredModel>, ProviderError> {
    // Ensure the base URL has a trailing slash so joining "models" appends it
    // as a sub-path rather than replacing the last segment.
    let url = with_trailing_slash(&ctx.base_url)
        .join("models")
        .map_err(|e| ProviderError::Other(e.to_string()))?;

    let config = ProviderConfig {
        id: provider_id.into(),
        api_key: ctx.api_key.clone(),
        ..Default::default()
    };

    let resp = ProviderHttp::get(&ctx.client, url, bearer_auth(&config)).await?;

    let status = resp.status();
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ProviderError::MalformedResponse(e.to_string()))?;

    if !status.is_success() {
        return Err(ProviderError::Other(format!("Discovery failed: {}", body)));
    }

    let models = body
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    let id = m.get("id")?.as_str()?.to_string();
                    Some(DiscoveredModel {
                        provider_id: provider_id.to_string(),
                        upstream_model_id: id.clone(),
                        display_name: Some(id.clone()),
                        endpoint_compatibility: vec!["chat".into()],
                        context_window: m
                            .get("max_context_length")
                            .and_then(|v| v.as_u64())
                            .or_else(|| m.get("context_length").and_then(|v| v.as_u64())),
                        free_tier: true,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(models)
}

/// Parse a single SSE data line and extract the JSON payload.
/// Returns None for non-data lines.
fn parse_sse_line(line: &str) -> Option<String> {
    let data = line.strip_prefix("data:")?;
    let trimmed = data.trim_start().trim_end();
    if trimmed == "[DONE]" {
        return Some(String::new());
    }
    Some(trimmed.to_string())
}

/// Stream OpenAI-compatible SSE chat completions through a channel.
/// Makes a POST to /chat/completions with stream: true, then parses SSE events
/// and sends them through the provided sender.
pub async fn openai_stream_completions(
    ctx: &ProviderContext,
    request: NormalizedChatRequest,
    provider_id: &str,
    tx: tokio::sync::mpsc::Sender<crate::api::openai::stream::StreamEvent>,
) -> Result<(), ProviderError> {
    let url = with_trailing_slash(&ctx.base_url)
        .join("chat/completions")
        .map_err(|e| ProviderError::Other(e.to_string()))?;

    let config = ProviderConfig {
        id: provider_id.into(),
        api_key: ctx.api_key.clone(),
        ..Default::default()
    };

    let body = openai_chat_body(&request, true);

    let mut resp = ctx
        .client
        .post(url.clone())
        .headers(bearer_auth(&config))
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                ProviderError::Timeout
            } else {
                ProviderError::Http(e.to_string())
            }
        })?;

    let status = resp.status();
    if !status.is_success() {
        let body_text = resp.text().await.unwrap_or_default();
        if should_retry_with_min_max_tokens(&request, &body_text) {
            tracing::info!(
                provider = %provider_id,
                model = %request.model,
                "Retrying OpenAI-compatible stream with max_tokens=1 after upstream rejected omitted token limit"
            );
            let retry_request = with_min_max_tokens(request.clone());
            let retry_body = openai_chat_body(&retry_request, true);
            resp = ctx
                .client
                .post(url)
                .headers(bearer_auth(&config))
                .json(&retry_body)
                .send()
                .await
                .map_err(|e| {
                    if e.is_timeout() {
                        ProviderError::Timeout
                    } else {
                        ProviderError::Http(e.to_string())
                    }
                })?;
            let retry_status = resp.status();
            if !retry_status.is_success() {
                let retry_body_text = resp.text().await.unwrap_or_default();
                return Err(classify_error(retry_status.as_u16(), &retry_body_text));
            }
        } else {
            return Err(classify_error(status.as_u16(), &body_text));
        }
    } else {
        // keep the successful response
    }

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        return Err(classify_error(status.as_u16(), &body_text));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let created = chrono::Utc::now().timestamp();
    let model = request.model.clone();

    // Read SSE stream line by line
    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();

    use futures::StreamExt;
    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                let text = String::from_utf8_lossy(&chunk);
                buffer.push_str(&text);

                // Process complete lines
                while let Some(newline_pos) = buffer.find('\n') {
                    let line = buffer[..newline_pos].trim().to_string();
                    buffer = buffer[newline_pos + 1..].to_string();

                    if line.is_empty() {
                        // Empty line = end of SSE event
                        continue;
                    }

                    if let Some(json_str) = parse_sse_line(&line) {
                        if json_str.is_empty() {
                            // [DONE] sentinel
                            let _ = tx.send(crate::api::openai::stream::StreamEvent::Done).await;
                            return Ok(());
                        }

                        // Parse the SSE data as streaming chunk
                        match serde_json::from_str::<serde_json::Value>(&json_str) {
                            Ok(data) => {
                                if let Some(choices) =
                                    data.get("choices").and_then(|c| c.as_array())
                                {
                                    for choice in choices {
                                        let delta = choice.get("delta");
                                        let content = delta
                                            .and_then(|d| d.get("content"))
                                            .and_then(|c| c.as_str())
                                            .map(|s| s.to_string());
                                        let role = delta
                                            .and_then(|d| d.get("role"))
                                            .and_then(|r| r.as_str())
                                            .map(|s| s.to_string());
                                        let finish = choice
                                            .get("finish_reason")
                                            .and_then(|f| f.as_str())
                                            .map(|s| s.to_string());

                                        if content.is_some() || finish.is_some() {
                                            let _ = tx
                                                .send(crate::api::openai::stream::StreamEvent::Chunk {
                                                    id: id.clone(),
                                                    created,
                                                    model: model.clone(),
                                                    delta: crate::api::openai::chat::StreamDelta {
                                                        role,
                                                        content,
                                                    },
                                                    finish_reason: finish,
                                                })
                                                .await;
                                        }

                                        if let Some(tool_calls) = delta
                                            .and_then(|d| d.get("tool_calls"))
                                            .and_then(|v| v.as_array())
                                        {
                                            for tool_call in tool_calls {
                                                let index = tool_call
                                                    .get("index")
                                                    .and_then(|v| v.as_u64())
                                                    .unwrap_or(0)
                                                    as u32;
                                                let tool_call_id = tool_call
                                                    .get("id")
                                                    .and_then(|v| v.as_str())
                                                    .map(|s| s.to_string());
                                                let function = tool_call.get("function");
                                                let function_name = function
                                                    .and_then(|f| f.get("name"))
                                                    .and_then(|v| v.as_str())
                                                    .map(|s| s.to_string());
                                                let function_arguments = function
                                                    .and_then(|f| f.get("arguments"))
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("")
                                                    .to_string();

                                                if tool_call_id.is_some()
                                                    || function_name.is_some()
                                                    || !function_arguments.is_empty()
                                                {
                                                    let _ = tx
                                                        .send(
                                                            crate::api::openai::stream::StreamEvent::ToolCallChunk {
                                                                id: id.clone(),
                                                                created,
                                                                model: model.clone(),
                                                                index,
                                                                tool_call_id,
                                                                function_name,
                                                                function_arguments,
                                                            },
                                                        )
                                                        .await;
                                                }
                                            }
                                        }
                                    }
                                }

                                // Check for usage metadata
                                if let Some(usage) = data.get("usage") {
                                    let _ = tx
                                        .send(crate::api::openai::stream::StreamEvent::Usage {
                                            id: id.clone(),
                                            created,
                                            model: model.clone(),
                                            prompt_tokens: usage
                                                .get("prompt_tokens")
                                                .and_then(|v| v.as_u64())
                                                .unwrap_or(0)
                                                as u32,
                                            completion_tokens: usage
                                                .get("completion_tokens")
                                                .and_then(|v| v.as_u64())
                                                .unwrap_or(0)
                                                as u32,
                                            total_tokens: usage
                                                .get("total_tokens")
                                                .and_then(|v| v.as_u64())
                                                .unwrap_or(0)
                                                as u32,
                                        })
                                        .await;
                                }
                            }
                            Err(error) => {
                                tracing::warn!(
                                    provider = %provider_id,
                                    %error,
                                    "Ignoring malformed upstream SSE data frame"
                                );
                            }
                        }
                    }
                }
            }
            Err(e) => {
                return Err(ProviderError::Http(format!("Stream error: {}", e)));
            }
        }
    }

    // Stream ended without [DONE]
    let _ = tx.send(crate::api::openai::stream::StreamEvent::Done).await;
    Ok(())
}

/// Classify HTTP status codes into provider errors.
pub fn classify_error(status: u16, message: &str) -> ProviderError {
    classify_error_with_rate_limits(status, message, &Default::default())
}

/// Classify HTTP status codes into provider errors with optional rate-limit hints.
pub fn classify_error_with_rate_limits(
    status: u16,
    message: &str,
    rate_limits: &crate::providers::normalization::RateLimitInfo,
) -> ProviderError {
    let message_lower = message.to_ascii_lowercase();
    match status {
        429 => ProviderError::RateLimited {
            retry_after: rate_limits.retry_after,
            details: message.to_string(),
        },
        413 if message_lower.contains("rate_limit_exceeded")
            || message_lower.contains("tokens per minute")
            || message_lower.contains("tpm") =>
        {
            ProviderError::RateLimited {
                retry_after: rate_limits.retry_after,
                details: message.to_string(),
            }
        }
        401 | 403 => ProviderError::Auth(message.to_string()),
        400 => ProviderError::Other(format!("Bad request: {}", message)),
        s if s >= 500 => ProviderError::Other(format!("Upstream {}: {}", s, message)),
        _ => ProviderError::Other(format!("HTTP {}: {}", status, message)),
    }
}

/// Build a macro to generate OpenAI-compatible stub adapters quickly.
#[macro_export]
macro_rules! openai_compat_adapter {
    ($name:ident, $id:expr, $display:expr, $base_url:expr $(, $extra_feature:ident)*) => {
        pub struct $name;

        #[async_trait]
        impl ProviderAdapter for $name {
            fn provider_id(&self) -> &'static str {
                $id
            }
            fn display_name(&self) -> &'static str {
                $display
            }
            fn supports_endpoint(&self, kind: &EndpointKind) -> bool {
                matches!(
                    kind,
                    EndpointKind::ChatCompletions | EndpointKind::ModelList
                )
            }
            fn auth_kind(&self) -> AuthKind {
                AuthKind::Bearer
            }
            fn capabilities(&self) -> ProviderCapabilities {
                ProviderCapabilities {
                    openai_compatible: true,
                    has_quirks: false,
                    quirks: vec![],
                    supports_streaming: true,
                    supports_tools: true,
                    supports_json_mode: true,
                    supports_vision: false,
                    docs_url: None,
                }
            }
            fn base_url(&self, config: &ProviderConfig) -> Url {
                $crate::providers::shared::provider_base_url($id, config, $base_url)
            }
            fn default_headers(&self, config: &ProviderConfig) -> HeaderMap {
                bearer_auth(config)
            }
            async fn discover_models(
                &self,
                ctx: &ProviderContext,
            ) -> Result<Vec<DiscoveredModel>, ProviderError> {
                $crate::providers::shared::openai_discover_models(ctx, $id).await
            }
            async fn chat_completions(
                &self,
                ctx: &ProviderContext,
                request: NormalizedChatRequest,
            ) -> Result<ProviderChatResponse, ProviderError> {
                $crate::providers::shared::openai_chat_completions(ctx, request, $id).await
            }
            async fn embeddings(
                &self,
                _ctx: &ProviderContext,
                _request: NormalizedEmbeddingsRequest,
            ) -> Result<ProviderEmbeddingsResponse, ProviderError> {
                Err(ProviderError::UnsupportedFeature(format!(
                    "{} does not support embeddings",
                    $id
                )))
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::{
        openai_chat_body, parse_sse_line, serialize_chat_messages,
        should_retry_with_min_max_tokens, with_min_max_tokens,
    };
    use crate::api::openai::chat::{ChatMessage, ToolCall, ToolCallFunction};

    #[test]
    fn serialize_chat_messages_preserves_tool_call_context() {
        let messages = vec![
            ChatMessage {
                role: "assistant".into(),
                content: None,
                name: None,
                tool_calls: Some(vec![ToolCall {
                    id: "call_1".into(),
                    call_type: "function".into(),
                    function: ToolCallFunction {
                        name: "pwd".into(),
                        arguments: "{}".into(),
                    },
                }]),
                tool_call_id: None,
            },
            ChatMessage {
                role: "tool".into(),
                content: Some(serde_json::Value::String(
                    "/Users/kabudu/repositories/token-scavenger".into(),
                )),
                name: None,
                tool_calls: None,
                tool_call_id: Some("call_1".into()),
            },
        ];

        let serialized = serialize_chat_messages(&messages);

        assert_eq!(serialized[0]["tool_calls"][0]["id"], "call_1");
        assert_eq!(serialized[0]["tool_calls"][0]["function"]["name"], "pwd");
        assert_eq!(serialized[1]["tool_call_id"], "call_1");
    }

    #[test]
    fn parse_sse_line_accepts_compact_and_spaced_data_prefixes() {
        assert_eq!(
            parse_sse_line("data:{\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}"),
            Some("{\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}".into())
        );
        assert_eq!(
            parse_sse_line("data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}"),
            Some("{\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}".into())
        );
        assert_eq!(parse_sse_line("data: [DONE]"), Some(String::new()));
        assert_eq!(parse_sse_line("event: message"), None);
    }

    #[test]
    fn openai_chat_body_omits_absent_max_tokens() {
        let request = crate::api::openai::chat::NormalizedChatRequest {
            model: "model-a".into(),
            messages: vec![crate::api::openai::chat::ChatMessage {
                role: "user".into(),
                content: Some(serde_json::json!("hi")),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            }],
            temperature: None,
            top_p: None,
            max_tokens: None,
            stream: true,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            response_format: None,
            tools: None,
            tool_choice: None,
        };

        let body = openai_chat_body(&request, true);
        assert!(body.get("max_tokens").is_none());
        assert_eq!(body["stream"], true);
        assert_eq!(body["stream_options"]["include_usage"], true);
    }

    #[test]
    fn retries_min_max_tokens_for_zero_limit_error() {
        let request = crate::api::openai::chat::NormalizedChatRequest {
            model: "openai/gpt-oss-120b".into(),
            messages: vec![crate::api::openai::chat::ChatMessage {
                role: "user".into(),
                content: Some(serde_json::json!("hi")),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            }],
            temperature: None,
            top_p: None,
            max_tokens: None,
            stream: true,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            response_format: None,
            tools: None,
            tool_choice: None,
        };

        assert!(should_retry_with_min_max_tokens(
            &request,
            r#"{"error":{"message":"max_tokens must be at least 1, got 0"}}"#
        ));
        let retry_request = with_min_max_tokens(request);
        let body = openai_chat_body(&retry_request, true);
        assert_eq!(body["max_tokens"], 1);
    }

    #[test]
    fn does_not_retry_negative_context_budget_error() {
        let request = crate::api::openai::chat::NormalizedChatRequest {
            model: "openai/gpt-oss-120b".into(),
            messages: vec![crate::api::openai::chat::ChatMessage {
                role: "user".into(),
                content: Some(serde_json::json!("hi")),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            }],
            temperature: None,
            top_p: None,
            max_tokens: None,
            stream: true,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            response_format: None,
            tools: None,
            tool_choice: None,
        };

        assert!(!should_retry_with_min_max_tokens(
            &request,
            r#"{"error":{"message":"max_tokens must be at least 1, got -504"}}"#
        ));
    }
}
