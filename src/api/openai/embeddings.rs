use crate::api::openai::chat::{ProviderUsage, UsageResponse};
use serde::{Deserialize, Serialize};

/// OpenAI-compatible embeddings request.
#[derive(Debug, Deserialize)]
pub struct EmbeddingsRequest {
    pub model: String,
    pub input: serde_json::Value,
    pub encoding_format: Option<String>,
    pub user: Option<String>,
}

/// OpenAI-compatible embeddings response.
#[derive(Debug, Serialize)]
pub struct EmbeddingsResponse {
    pub object: String,
    pub data: Vec<EmbeddingData>,
    pub model: String,
    pub usage: UsageResponse,
}

#[derive(Debug, Serialize)]
pub struct EmbeddingData {
    pub object: String,
    pub index: u32,
    pub embedding: Vec<f64>,
}

/// Normalized internal embeddings request.
#[derive(Debug, Clone)]
pub struct NormalizedEmbeddingsRequest {
    pub model: String,
    pub input: Vec<String>,
    pub encoding_format: Option<String>,
    pub user: Option<String>,
}

impl NormalizedEmbeddingsRequest {
    pub fn from_request(req: EmbeddingsRequest) -> Self {
        let input = match &req.input {
            serde_json::Value::String(s) => vec![s.clone()],
            serde_json::Value::Array(arr) => arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect(),
            _ => vec![],
        };

        Self {
            model: req.model,
            input,
            encoding_format: req.encoding_format,
            user: req.user,
        }
    }
}

/// Result from a provider adapter for embeddings.
#[derive(Debug)]
pub struct ProviderEmbeddingsResponse {
    pub provider_id: String,
    pub model_id: String,
    pub data: Vec<EmbeddingData>,
    pub usage: ProviderUsage,
    pub latency_ms: i64,
}
