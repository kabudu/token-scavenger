use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// OpenAI-compatible chat completion request.
#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub max_tokens: Option<u32>,
    pub stream: Option<bool>,
    pub stop: Option<serde_json::Value>,
    pub presence_penalty: Option<f64>,
    pub frequency_penalty: Option<f64>,
    pub user: Option<String>,
    pub response_format: Option<ResponseFormat>,
    pub tools: Option<Vec<ToolDefinition>>,
    pub tool_choice: Option<serde_json::Value>,
    /// Extra fields not explicitly supported, preserved for passthrough.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: Option<serde_json::Value>,
    pub name: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: ToolFunction,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ToolFunction {
    pub name: String,
    pub description: Option<String>,
    pub parameters: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ToolCallFunction,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ResponseFormat {
    #[serde(rename = "type")]
    pub format_type: String,
    pub json_schema: Option<serde_json::Value>,
}

/// OpenAI-compatible chat completion response (non-streaming).
#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChatChoice>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Serialize)]
pub struct ChatChoice {
    pub index: u32,
    pub message: ChatResponseMessage,
    pub finish_reason: Option<String>,
    pub logprobs: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct ChatResponseMessage {
    pub role: String,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// OpenAI-compatible usage metadata.
#[derive(Debug, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Normalized internal chat request used by the routing engine and provider adapters.
#[derive(Debug, Clone)]
pub struct NormalizedChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub max_tokens: Option<u32>,
    pub stream: bool,
    pub stop: Option<Vec<String>>,
    pub presence_penalty: Option<f64>,
    pub frequency_penalty: Option<f64>,
    pub user: Option<String>,
    pub response_format: Option<ResponseFormat>,
    pub tools: Option<Vec<ToolDefinition>>,
    pub tool_choice: Option<serde_json::Value>,
}

impl NormalizedChatRequest {
    pub fn from_request(req: ChatRequest) -> Self {
        let stop = match req.stop {
            Some(serde_json::Value::String(s)) => Some(vec![s]),
            Some(serde_json::Value::Array(arr)) => {
                let v: Vec<String> = arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                Some(v)
            }
            _ => None,
        };

        Self {
            model: req.model,
            messages: req.messages,
            temperature: req.temperature,
            top_p: req.top_p,
            max_tokens: req.max_tokens,
            stream: req.stream.unwrap_or(false),
            stop,
            presence_penalty: req.presence_penalty,
            frequency_penalty: req.frequency_penalty,
            user: req.user,
            response_format: req.response_format,
            tools: req.tools,
            tool_choice: req.tool_choice,
        }
    }
}

/// Result from a provider adapter for chat completions.
#[derive(Debug)]
pub struct ProviderChatResponse {
    pub provider_id: String,
    pub model_id: String,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub finish_reason: Option<String>,
    pub usage: Option<Usage>,
    pub latency_ms: i64,
}
