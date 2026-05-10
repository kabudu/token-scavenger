use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;

/// OpenAI-compatible chat completion request.
#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_positive_max_tokens")]
    pub max_tokens: Option<u32>,
    pub stream: Option<bool>,
    pub stop: Option<serde_json::Value>,
    pub presence_penalty: Option<f64>,
    pub frequency_penalty: Option<f64>,
    pub user: Option<String>,
    pub response_format: Option<ResponseFormat>,
    pub tools: Option<Vec<ToolDefinition>>,
    pub tool_choice: Option<serde_json::Value>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: Option<serde_json::Value>,
    pub name: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: ToolFunction,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ToolFunction {
    pub name: String,
    pub description: Option<String>,
    pub parameters: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ToolCallFunction,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
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
    pub usage: Option<UsageResponse>,
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
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UsageResponse {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_hit_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_miss_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u32>,
}

/// SSE streaming delta payload for chat completions.
#[derive(Debug, Serialize, Clone)]
pub struct StreamDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
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
            Some(serde_json::Value::Array(arr)) => Some(
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect(),
            ),
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

    pub fn prompt_size_hint(&self) -> usize {
        let messages = serde_json::to_vec(&self.messages)
            .map(|value| value.len())
            .unwrap_or(0);
        let tools = self
            .tools
            .as_ref()
            .and_then(|tools| serde_json::to_vec(tools).ok())
            .map(|value| value.len())
            .unwrap_or(0);
        messages + tools
    }
}

fn deserialize_positive_max_tokens<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    let Some(value) = value else {
        return Ok(None);
    };

    let number = if let Some(number) = value.as_i64() {
        number
    } else if let Some(number) = value.as_u64() {
        number.min(u32::MAX as u64) as i64
    } else {
        return Err(serde::de::Error::custom("max_tokens must be a number"));
    };

    Ok(Some(number.clamp(1, u32::MAX as i64) as u32))
}

/// Usage struct used by provider adapters.
#[derive(Debug, Clone)]
pub struct ProviderUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    pub prompt_cache_hit_tokens: Option<u32>,
    pub prompt_cache_miss_tokens: Option<u32>,
    pub reasoning_tokens: Option<u32>,
}

/// Result from a provider adapter for chat completions.
#[derive(Debug)]
pub struct ProviderChatResponse {
    pub provider_id: String,
    pub model_id: String,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub finish_reason: Option<String>,
    pub usage: Option<ProviderUsage>,
    pub latency_ms: i64,
}

#[cfg(test)]
mod tests {
    use super::ChatRequest;

    #[test]
    fn chat_request_clamps_non_positive_max_tokens() {
        let request: ChatRequest = serde_json::from_value(serde_json::json!({
            "model": "agentic",
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": -78
        }))
        .unwrap();
        assert_eq!(request.max_tokens, Some(1));

        let request: ChatRequest = serde_json::from_value(serde_json::json!({
            "model": "agentic",
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 0
        }))
        .unwrap();
        assert_eq!(request.max_tokens, Some(1));
    }
}
