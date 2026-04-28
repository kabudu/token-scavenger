use serde::{Deserialize, Serialize};

/// A model discovered from a provider's model list endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredModel {
    pub provider_id: String,
    pub upstream_model_id: String,
    pub display_name: Option<String>,
    pub endpoint_compatibility: Vec<String>,
    pub context_window: Option<u64>,
    pub free_tier: bool,
}

/// Built-in curated model catalog shipped with the binary.
/// This is the baseline that discovery augments.
pub fn curated_catalog() -> Vec<DiscoveredModel> {
    vec![
        // Groq
        DiscoveredModel {
            provider_id: "groq".into(),
            upstream_model_id: "llama3-70b-8192".into(),
            display_name: Some("Llama 3 70B".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(8192),
            free_tier: true,
        },
        DiscoveredModel {
            provider_id: "groq".into(),
            upstream_model_id: "llama3-8b-8192".into(),
            display_name: Some("Llama 3 8B".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(8192),
            free_tier: true,
        },
        DiscoveredModel {
            provider_id: "groq".into(),
            upstream_model_id: "mixtral-8x7b-32768".into(),
            display_name: Some("Mixtral 8x7B".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(32768),
            free_tier: true,
        },
        // Google
        DiscoveredModel {
            provider_id: "google".into(),
            upstream_model_id: "gemini-2.0-flash".into(),
            display_name: Some("Gemini 2.0 Flash".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(1_048_576),
            free_tier: true,
        },
        DiscoveredModel {
            provider_id: "google".into(),
            upstream_model_id: "gemini-1.5-flash".into(),
            display_name: Some("Gemini 1.5 Flash".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(1_048_576),
            free_tier: true,
        },
        // OpenRouter free
        DiscoveredModel {
            provider_id: "openrouter".into(),
            upstream_model_id: "meta-llama/llama-3.3-70b-instruct:free".into(),
            display_name: Some("Llama 3.3 70B (Free)".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(8192),
            free_tier: true,
        },
        // Cloudflare
        DiscoveredModel {
            provider_id: "cloudflare".into(),
            upstream_model_id: "@cf/meta/llama-3.3-70b-instruct-fp8-fast".into(),
            display_name: Some("Llama 3.3 70B (Cloudflare)".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(8192),
            free_tier: true,
        },
        // Cerebras
        DiscoveredModel {
            provider_id: "cerebras".into(),
            upstream_model_id: "llama3.1-8b".into(),
            display_name: Some("Llama 3.1 8B".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(8192),
            free_tier: true,
        },
        // Mistral
        DiscoveredModel {
            provider_id: "mistral".into(),
            upstream_model_id: "mistral-large-latest".into(),
            display_name: Some("Mistral Large".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(128_000),
            free_tier: true,
        },
        DiscoveredModel {
            provider_id: "mistral".into(),
            upstream_model_id: "open-mistral-nemo".into(),
            display_name: Some("Mistral Nemo".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(128_000),
            free_tier: true,
        },
        // GitHub Models
        DiscoveredModel {
            provider_id: "github-models".into(),
            upstream_model_id: "gpt-4o-mini".into(),
            display_name: Some("GPT-4o Mini".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(128_000),
            free_tier: true,
        },
        // Z AI
        DiscoveredModel {
            provider_id: "zai".into(),
            upstream_model_id: "glm-4-flash".into(),
            display_name: Some("GLM-4 Flash".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(128_000),
            free_tier: true,
        },
        // SiliconFlow
        DiscoveredModel {
            provider_id: "siliconflow".into(),
            upstream_model_id: "deepseek-ai/DeepSeek-V3".into(),
            display_name: Some("DeepSeek V3".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(128_000),
            free_tier: true,
        },
        // NVIDIA
        DiscoveredModel {
            provider_id: "nvidia".into(),
            upstream_model_id: "meta/llama-3.3-70b-instruct".into(),
            display_name: Some("Llama 3.3 70B (NVIDIA)".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(128_000),
            free_tier: true,
        },
    ]
}
