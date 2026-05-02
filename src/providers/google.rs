use crate::api::openai::chat::{NormalizedChatRequest, ProviderChatResponse, ProviderUsage};
use crate::api::openai::embeddings::{
    EmbeddingData, NormalizedEmbeddingsRequest, ProviderEmbeddingsResponse,
};
use crate::config::schema::ProviderConfig;
use crate::discovery::curated::DiscoveredModel;
use crate::providers::http::ProviderHttp;
use crate::providers::normalization::ProviderCapabilities;
use crate::providers::shared;
use crate::providers::traits::*;
use async_trait::async_trait;
use reqwest::header::HeaderMap;
use url::Url;

/// Google AI Studio / Gemini API
///
/// Uses a completely different API format from OpenAI:
/// - Auth: x-goog-api-key header (NOT Bearer)
/// - Chat: POST /v1beta/models/{model}:generateContent
/// - Messages: contents[{role, parts[{text}]}] instead of messages[{role, content}]
/// - Model in URL path, not request body
/// - Streaming: :streamGenerateContent endpoint
/// - Response: candidates[].content.parts[].text instead of choices[].message.content
pub struct GoogleAdapter;

fn google_api_key_auth(config: &ProviderConfig) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Some(ref key) = config.api_key {
        let header_name: reqwest::header::HeaderName = "x-goog-api-key".parse().unwrap();
        headers.insert(header_name, key.parse().unwrap());
    }
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        "application/json".parse().unwrap(),
    );
    headers
}

fn google_generate_content_body(request: &NormalizedChatRequest) -> serde_json::Value {
    let contents: Vec<serde_json::Value> = request
        .messages
        .iter()
        .filter(|m| m.role != "system")
        .map(|m| {
            let text = m
                .content
                .as_ref()
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string();
            serde_json::json!({
                "role": if m.role == "assistant" { "model" } else { m.role.as_str() },
                "parts": [{"text": text}]
            })
        })
        .collect();

    let mut generation_config = serde_json::json!({
        "temperature": request.temperature,
        "topP": request.top_p,
        "maxOutputTokens": request.max_tokens,
        "stopSequences": request.stop,
    });

    if request.response_format.is_some() {
        generation_config["responseMimeType"] = serde_json::json!("application/json");
    }

    let mut body = serde_json::json!({
        "contents": contents,
        "generationConfig": generation_config
    });

    if let Some(system_msg) = request.messages.iter().find(|m| m.role == "system") {
        if let Some(text) = system_msg.content.as_ref().and_then(|c| c.as_str()) {
            body["systemInstruction"] = serde_json::json!({
                "parts": [{"text": text}]
            });
        }
    }

    if let Some(tools) = &request.tools {
        body["tools"] = serde_json::to_value(tools).unwrap_or_else(|_| serde_json::json!([]));
    }

    if let Some(tool_choice) = &request.tool_choice {
        body["toolConfig"] = serde_json::json!({"functionCallingConfig": tool_choice});
    }

    body
}

#[async_trait]
impl ProviderAdapter for GoogleAdapter {
    fn provider_id(&self) -> &'static str {
        "google"
    }
    fn display_name(&self) -> &'static str {
        "Google AI Studio"
    }
    fn supports_endpoint(&self, kind: &EndpointKind) -> bool {
        matches!(
            kind,
            EndpointKind::ChatCompletions | EndpointKind::ModelList | EndpointKind::Embeddings
        )
    }
    fn auth_kind(&self) -> AuthKind {
        AuthKind::Header("x-goog-api-key".into())
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            openai_compatible: false,
            has_quirks: true,
            quirks: vec![
                "Google Gemini uses a fundamentally different API format from OpenAI".into(),
                "Auth is via x-goog-api-key header, not Bearer token".into(),
                "Messages use contents[{role, parts[{text}]}] format".into(),
                "Model is in URL path, not request body".into(),
            ],
            supports_streaming: true,
            supports_tools: true,
            supports_json_mode: true,
            supports_vision: true,
            docs_url: Some("https://ai.google.dev/gemini-api/docs".into()),
        }
    }
    fn base_url(&self, config: &ProviderConfig) -> Url {
        shared::provider_base_url(
            "google",
            config,
            "https://generativelanguage.googleapis.com/v1beta",
        )
    }
    fn default_headers(&self, config: &ProviderConfig) -> HeaderMap {
        google_api_key_auth(config)
    }

    async fn discover_models(
        &self,
        ctx: &ProviderContext,
    ) -> Result<Vec<DiscoveredModel>, ProviderError> {
        let url = crate::providers::shared::with_trailing_slash(&ctx.base_url)
            .join("models")
            .map_err(|e| ProviderError::Other(e.to_string()))?;

        let config = ProviderConfig {
            id: "google".into(),
            api_key: ctx.api_key.clone(),
            ..Default::default()
        };

        let resp = ProviderHttp::get(&ctx.client, url, google_api_key_auth(&config)).await?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ProviderError::MalformedResponse(e.to_string()))?;

        if !status.is_success() {
            return Err(ProviderError::Other(format!("Discovery failed: {}", body)));
        }

        let models = body
            .get("models")
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| {
                        let name = m.get("name")?.as_str()?;
                        // Strip "models/" prefix
                        let model_id = name.strip_prefix("models/").unwrap_or(name).to_string();
                        let display = m
                            .get("displayName")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&model_id);
                        let supports_chat = m
                            .get("supportedGenerationMethods")
                            .and_then(|v| v.as_array())
                            .map(|arr| arr.iter().any(|m| m.as_str() == Some("generateContent")))
                            .unwrap_or(false);
                        if !supports_chat {
                            return None;
                        }
                        Some(DiscoveredModel {
                            provider_id: "google".into(),
                            upstream_model_id: model_id.clone(),
                            display_name: Some(display.to_string()),
                            endpoint_compatibility: vec!["chat".into()],
                            context_window: m.get("inputTokenLimit").and_then(|v| v.as_u64()),
                            free_tier: true,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        Ok(models)
    }

    async fn chat_completions(
        &self,
        ctx: &ProviderContext,
        request: NormalizedChatRequest,
    ) -> Result<ProviderChatResponse, ProviderError> {
        // Gemini uses POST /v1beta/models/{model}:generateContent
        let endpoint_path = format!("/models/{}:generateContent", request.model);
        let url = ctx
            .base_url
            .join(&endpoint_path)
            .map_err(|e| ProviderError::Other(e.to_string()))?;

        let config = ProviderConfig {
            id: "google".into(),
            api_key: ctx.api_key.clone(),
            ..Default::default()
        };

        let body = google_generate_content_body(&request);

        let start = std::time::Instant::now();
        let resp =
            ProviderHttp::post_json(&ctx.client, url, google_api_key_auth(&config), &body).await?;
        let latency_ms = start.elapsed().as_millis() as i64;

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
            return Err(shared::classify_error(status.as_u16(), msg));
        }

        // Extract text from Gemini response
        let text = response_body
            .get("candidates")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|c| c.get("content"))
            .and_then(|c| c.get("parts"))
            .and_then(|p| p.as_array())
            .and_then(|arr| arr.first())
            .and_then(|p| p.get("text"))
            .and_then(|t| t.as_str())
            .map(|s| s.to_string());

        let finish_reason = response_body
            .get("candidates")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|c| c.get("finishReason"))
            .and_then(|r| r.as_str())
            .map(|s| s.to_string());

        let usage = response_body.get("usageMetadata").map(|u| ProviderUsage {
            prompt_tokens: u
                .get("promptTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
            completion_tokens: u
                .get("candidatesTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
            total_tokens: u
                .get("totalTokenCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
        });

        let model = response_body
            .get("modelVersion")
            .and_then(|v| v.as_str())
            .unwrap_or(&request.model)
            .to_string();

        Ok(ProviderChatResponse {
            provider_id: "google".into(),
            model_id: model,
            content: text,
            tool_calls: None,
            finish_reason: finish_reason.map(|r| {
                match r.as_str() {
                    "STOP" => "stop",
                    "MAX_TOKENS" => "length",
                    "SAFETY" => "content_filter",
                    _ => "stop",
                }
                .to_string()
            }),
            usage,
            latency_ms,
        })
    }

    async fn embeddings(
        &self,
        ctx: &ProviderContext,
        request: NormalizedEmbeddingsRequest,
    ) -> Result<ProviderEmbeddingsResponse, ProviderError> {
        let endpoint_path = format!("/models/{}:embedContent", request.model);
        let url = ctx
            .base_url
            .join(&endpoint_path)
            .map_err(|e| ProviderError::Other(e.to_string()))?;

        let config = ProviderConfig {
            id: "google".into(),
            api_key: ctx.api_key.clone(),
            ..Default::default()
        };

        let mut all_embeddings = Vec::new();
        for (i, input) in request.input.iter().enumerate() {
            let body = serde_json::json!({
                "content": {
                    "parts": [{"text": input}]
                }
            });

            let start = std::time::Instant::now();
            let resp = ProviderHttp::post_json(
                &ctx.client,
                url.clone(),
                google_api_key_auth(&config),
                &body,
            )
            .await?;
            let _latency_ms = start.elapsed().as_millis() as i64;

            let response_body: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| ProviderError::MalformedResponse(e.to_string()))?;

            let values = response_body
                .get("embedding")
                .and_then(|e| e.get("values"))
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect::<Vec<f64>>())
                .unwrap_or_default();

            all_embeddings.push(EmbeddingData {
                object: "embedding".into(),
                index: i as u32,
                embedding: values,
            });
        }

        Ok(ProviderEmbeddingsResponse {
            provider_id: "google".into(),
            model_id: request.model.clone(),
            data: all_embeddings,
            usage: ProviderUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            },
            latency_ms: 0,
        })
    }

    async fn stream_chat_completions(
        &self,
        ctx: &ProviderContext,
        request: NormalizedChatRequest,
        tx: tokio::sync::mpsc::Sender<crate::api::openai::stream::StreamEvent>,
    ) -> Result<(), ProviderError> {
        // Gemini uses :streamGenerateContent instead of :generateContent
        let endpoint_path = format!("/models/{}:streamGenerateContent", request.model);
        let url = ctx
            .base_url
            .join(&endpoint_path)
            .map_err(|e| ProviderError::Other(e.to_string()))?;

        let config = ProviderConfig {
            id: "google".into(),
            api_key: ctx.api_key.clone(),
            ..Default::default()
        };

        let body = google_generate_content_body(&request);

        let resp = ctx
            .client
            .post(url)
            .headers(google_api_key_auth(&config))
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
            return Err(shared::classify_error(status.as_u16(), &body_text));
        }

        let id = uuid::Uuid::new_v4().to_string();
        let created = chrono::Utc::now().timestamp();
        let model = request.model.clone();

        // Read SSE stream
        use futures::StreamExt;
        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk =
                chunk_result.map_err(|e| ProviderError::Http(format!("Stream error: {}", e)))?;
            let text = String::from_utf8_lossy(&chunk);
            buffer.push_str(&text);

            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim().to_string();
                buffer = buffer[newline_pos + 1..].to_string();

                if line.is_empty() || !line.starts_with("data: ") {
                    continue;
                }

                let json_str = line.strip_prefix("data: ").unwrap_or("").trim().to_string();
                if json_str.is_empty() || json_str == "[DONE]" {
                    let _ = tx.send(crate::api::openai::stream::StreamEvent::Done).await;
                    return Ok(());
                }

                if let Ok(data) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    let text = data
                        .get("candidates")
                        .and_then(|c| c.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|c| c.get("content"))
                        .and_then(|c| c.get("parts"))
                        .and_then(|p| p.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|p| p.get("text"))
                        .and_then(|t| t.as_str())
                        .map(|s| s.to_string());

                    let finish_reason = data
                        .get("candidates")
                        .and_then(|c| c.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|c| c.get("finishReason"))
                        .and_then(|r| r.as_str())
                        .map(|s| s.to_string());

                    if text.is_some() || finish_reason.is_some() {
                        let _ = tx
                            .send(crate::api::openai::stream::StreamEvent::Chunk {
                                id: id.clone(),
                                created,
                                model: model.clone(),
                                delta: crate::api::openai::chat::StreamDelta {
                                    role: Some("assistant".into()),
                                    content: text,
                                },
                                finish_reason: finish_reason.map(|r| {
                                    match r.as_str() {
                                        "STOP" => "stop",
                                        "MAX_TOKENS" => "length",
                                        _ => &r,
                                    }
                                    .to_string()
                                }),
                            })
                            .await;
                    }

                    if let Some(usage) = data.get("usageMetadata") {
                        let _ = tx
                            .send(crate::api::openai::stream::StreamEvent::Usage {
                                id: id.clone(),
                                created,
                                model: model.clone(),
                                prompt_tokens: usage
                                    .get("promptTokenCount")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0)
                                    as u32,
                                completion_tokens: usage
                                    .get("candidatesTokenCount")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0)
                                    as u32,
                                total_tokens: usage
                                    .get("totalTokenCount")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0)
                                    as u32,
                            })
                            .await;
                    }
                }
            }
        }

        let _ = tx.send(crate::api::openai::stream::StreamEvent::Done).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::openai::chat::{ChatMessage, NormalizedChatRequest};

    #[test]
    fn google_translation_preserves_optional_openai_fields() {
        let req = NormalizedChatRequest {
            model: "gemini-test".into(),
            messages: vec![
                ChatMessage {
                    role: "system".into(),
                    content: Some(serde_json::json!("Be terse")),
                    name: None,
                    tool_calls: None,
                    tool_call_id: None,
                },
                ChatMessage {
                    role: "user".into(),
                    content: Some(serde_json::json!("Hi")),
                    name: None,
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            stream: false,
            temperature: Some(0.2),
            top_p: Some(0.9),
            max_tokens: Some(64),
            stop: Some(vec!["END".into()]),
            tools: Some(vec![crate::api::openai::chat::ToolDefinition {
                tool_type: "function".into(),
                function: crate::api::openai::chat::ToolFunction {
                    name: "lookup".into(),
                    description: Some("Lookup".into()),
                    parameters: None,
                },
            }]),
            tool_choice: Some(serde_json::json!({"mode": "AUTO"})),
            response_format: Some(crate::api::openai::chat::ResponseFormat {
                format_type: "json_object".into(),
                json_schema: None,
            }),
            frequency_penalty: Some(0.1),
            presence_penalty: Some(0.1),
            user: Some("caller".into()),
        };

        let body = google_generate_content_body(&req);
        assert_eq!(body["systemInstruction"]["parts"][0]["text"], "Be terse");
        assert_eq!(body["contents"][0]["role"], "user");
        assert_eq!(
            body["generationConfig"]["responseMimeType"],
            "application/json"
        );
        assert!(body.get("tools").is_some());
        assert!(body.get("toolConfig").is_some());
    }
}
