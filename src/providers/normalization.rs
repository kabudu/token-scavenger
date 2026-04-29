use serde::{Deserialize, Serialize};

/// Provider capabilities metadata for catalog rendering.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderCapabilities {
    /// Whether the provider uses standard OpenAI-compatible endpoints.
    pub openai_compatible: bool,
    /// Whether the provider has known quirks in its implementation.
    pub has_quirks: bool,
    /// List of known quirks if any.
    pub quirks: Vec<String>,
    /// Whether the provider supports streaming.
    pub supports_streaming: bool,
    /// Whether the provider supports tool calls.
    pub supports_tools: bool,
    /// Whether the provider supports JSON mode.
    pub supports_json_mode: bool,
    /// Whether the provider supports vision.
    pub supports_vision: bool,
    /// Documentation URL.
    pub docs_url: Option<String>,
}

/// Provider-specific adapter configuration parsed from headers and rate-limit responses.
#[derive(Debug, Clone, Default)]
pub struct RateLimitInfo {
    pub remaining: Option<u64>,
    pub limit: Option<u64>,
    pub reset_at: Option<i64>,
    pub retry_after: Option<u64>,
}

/// Extract common rate-limit header patterns from a response.
pub fn parse_rate_limit_headers(headers: &reqwest::header::HeaderMap) -> RateLimitInfo {
    let remaining = headers
        .get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());

    let limit = headers
        .get("x-ratelimit-limit")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());

    let reset_at = headers
        .get("x-ratelimit-reset")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok());

    let retry_after = headers
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());

    RateLimitInfo {
        remaining,
        limit,
        reset_at,
        retry_after,
    }
}
