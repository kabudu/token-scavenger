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

    // Build the OpenAI-compatible request body
    let mut body = serde_json::json!({
        "model": request.model,
        "messages": request.messages.iter().map(|m| {
            let mut msg = serde_json::json!({
                "role": m.role,
                "content": m.content,
            });
            if let Some(ref name) = m.name {
                msg["name"] = serde_json::Value::String(name.clone());
            }
            msg
        }).collect::<Vec<_>>(),
        "temperature": request.temperature,
        "top_p": request.top_p,
        "max_tokens": request.max_tokens,
        "stream": false,
        "stop": request.stop,
    });
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

    let start = std::time::Instant::now();
    let resp = ProviderHttp::post_json(&ctx.client, url, bearer_auth(&config), &body).await?;
    let latency_ms = start.elapsed().as_millis() as i64;

    let rate_limits = parse_rate_limit_headers(resp.headers());
    let status = resp.status();
    let response_body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ProviderError::MalformedResponse(e.to_string()))?;

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
/// Returns None for non-data lines or [DONE] sentinel.
fn parse_sse_line(line: &str) -> Option<String> {
    if let Some(data) = line.strip_prefix("data: ") {
        let trimmed = data.trim();
        if trimmed == "[DONE]" {
            // Done sentinel
            return Some(String::new());
        }
        return Some(trimmed.to_string());
    }
    None
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

    let mut body = serde_json::json!({
        "model": request.model,
        "messages": request.messages.iter().map(|m| {
            let mut msg = serde_json::json!({
                "role": m.role,
                "content": m.content,
            });
            if let Some(ref name) = m.name {
                msg["name"] = serde_json::Value::String(name.clone());
            }
            msg
        }).collect::<Vec<_>>(),
        "stream": true,
        "stream_options": { "include_usage": true },
        "temperature": request.temperature,
        "top_p": request.top_p,
        "max_tokens": request.max_tokens,
        "stop": request.stop,
    });
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

    let resp = ctx
        .client
        .post(url)
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
                        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&json_str) {
                            if let Some(choices) = data.get("choices").and_then(|c| c.as_array()) {
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
    match status {
        429 => ProviderError::RateLimited {
            retry_after: rate_limits.retry_after,
        },
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
