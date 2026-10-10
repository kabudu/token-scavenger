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
///
/// IDs are refreshed against upstream free-tier offerings (October 2026).
/// Retired IDs (Groq `llama3-*-8192`/`mixtral-*`, Gemini 1.5/2.0,
/// OpenRouter `stealth/*` previews) were removed; live provider discovery
/// remains the source of truth and corrects any drift here.
pub fn curated_catalog() -> Vec<DiscoveredModel> {
    vec![
        // Groq: Llama 3.x versatile/instant IDs were retired in August 2026.
        // Current free-tier lineup is GPT-OSS plus Compound.
        DiscoveredModel {
            provider_id: "groq".into(),
            upstream_model_id: "openai/gpt-oss-120b".into(),
            display_name: Some("GPT-OSS 120B (Groq)".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(131_072),
            free_tier: true,
        },
        DiscoveredModel {
            provider_id: "groq".into(),
            upstream_model_id: "openai/gpt-oss-20b".into(),
            display_name: Some("GPT-OSS 20B (Groq)".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(131_072),
            free_tier: true,
        },
        DiscoveredModel {
            provider_id: "groq".into(),
            upstream_model_id: "groq/compound-mini".into(),
            display_name: Some("Compound Mini (Groq)".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(128_000),
            free_tier: true,
        },
        // Google: Gemini 1.5 family is retired and 2.0 Flash is deprecated
        // for new keys. 2.5 Flash-class models are the current free tier.
        DiscoveredModel {
            provider_id: "google".into(),
            upstream_model_id: "gemini-2.5-flash".into(),
            display_name: Some("Gemini 2.5 Flash".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(1_048_576),
            free_tier: true,
        },
        DiscoveredModel {
            provider_id: "google".into(),
            upstream_model_id: "gemini-2.5-flash-lite".into(),
            display_name: Some("Gemini 2.5 Flash-Lite".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(1_048_576),
            free_tier: true,
        },
        // OpenRouter free
        DiscoveredModel {
            provider_id: "openrouter".into(),
            upstream_model_id: "openrouter/free".into(),
            display_name: Some("OpenRouter Free Models Router".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(200_000),
            free_tier: true,
        },
        DiscoveredModel {
            provider_id: "openrouter".into(),
            upstream_model_id: "meta-llama/llama-3.3-70b-instruct:free".into(),
            display_name: Some("Llama 3.3 70B (Free)".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(131_072),
            free_tier: true,
        },
        DiscoveredModel {
            provider_id: "openrouter".into(),
            upstream_model_id: "cohere/north-mini-code:free".into(),
            display_name: Some("North Mini Code (Free)".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(256_000),
            free_tier: true,
        },
        DiscoveredModel {
            provider_id: "openrouter".into(),
            upstream_model_id: "openai/gpt-oss-20b:free".into(),
            display_name: Some("GPT-OSS 20B (Free)".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(131_072),
            free_tier: true,
        },
        DiscoveredModel {
            provider_id: "openrouter".into(),
            upstream_model_id: "google/gemma-3-27b-it:free".into(),
            display_name: Some("Gemma 3 27B (Free)".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(131_072),
            free_tier: true,
        },
        // Cloudflare
        DiscoveredModel {
            provider_id: "cloudflare".into(),
            upstream_model_id: "@cf/meta/llama-3.3-70b-instruct-fp8-fast".into(),
            display_name: Some("Llama 3.3 70B (Cloudflare)".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(128_000),
            free_tier: true,
        },
        // Cerebras
        DiscoveredModel {
            provider_id: "cerebras".into(),
            upstream_model_id: "llama3.1-8b".into(),
            display_name: Some("Llama 3.1 8B".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(128_000),
            free_tier: true,
        },
        DiscoveredModel {
            provider_id: "cerebras".into(),
            upstream_model_id: "llama-3.3-70b".into(),
            display_name: Some("Llama 3.3 70B (Cerebras)".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(131_072),
            free_tier: true,
        },
        DiscoveredModel {
            provider_id: "cerebras".into(),
            upstream_model_id: "qwen-3-32b".into(),
            display_name: Some("Qwen3 32B (Cerebras)".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(131_072),
            free_tier: true,
        },
        DiscoveredModel {
            provider_id: "cerebras".into(),
            upstream_model_id: "gpt-oss-120b".into(),
            display_name: Some("GPT-OSS 120B (Cerebras)".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(131_072),
            free_tier: true,
        },
        // Google embeddings: seeded so /v1/embeddings is routable on a
        // fresh database; live discovery also retains embedContent models.
        DiscoveredModel {
            provider_id: "google".into(),
            upstream_model_id: "gemini-embedding-001".into(),
            display_name: Some("Gemini Embedding 001".into()),
            endpoint_compatibility: vec!["embeddings".into()],
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
        // Z AI: glm-4-flash remains permanently free; GLM 4.5 Flash is the
        // current free flagship.
        DiscoveredModel {
            provider_id: "zai".into(),
            upstream_model_id: "glm-4-flash".into(),
            display_name: Some("GLM-4 Flash".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(128_000),
            free_tier: true,
        },
        DiscoveredModel {
            provider_id: "zai".into(),
            upstream_model_id: "glm-4.5-flash".into(),
            display_name: Some("GLM-4.5 Flash".into()),
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
        // DeepSeek paid fallback: deepseek-chat/deepseek-reasoner aliases
        // were retired in July 2026 in favor of the v4 family.
        DiscoveredModel {
            provider_id: "deepseek".into(),
            upstream_model_id: "deepseek-v4-flash".into(),
            display_name: Some("DeepSeek V4 Flash".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: None,
            free_tier: false,
        },
        DiscoveredModel {
            provider_id: "deepseek".into(),
            upstream_model_id: "deepseek-v4-pro".into(),
            display_name: Some("DeepSeek V4 Pro".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: None,
            free_tier: false,
        },
        // xAI paid fallback
        DiscoveredModel {
            provider_id: "xai".into(),
            upstream_model_id: "grok-4.20".into(),
            display_name: Some("Grok 4.20".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(2_000_000),
            free_tier: false,
        },
        DiscoveredModel {
            provider_id: "xai".into(),
            upstream_model_id: "grok-4.20-reasoning".into(),
            display_name: Some("Grok 4.20 Reasoning".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(2_000_000),
            free_tier: false,
        },
        DiscoveredModel {
            provider_id: "xai".into(),
            upstream_model_id: "grok-code-fast-1".into(),
            display_name: Some("Grok Code Fast 1".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: Some(256_000),
            free_tier: false,
        },
        // Local OpenAI-compatible providers. These are conservative examples;
        // dynamic discovery and operator-configured model rows remain the source
        // of truth for what is actually loaded on the workstation.
        DiscoveredModel {
            provider_id: "ollama".into(),
            upstream_model_id: "llama3.2".into(),
            display_name: Some("Llama 3.2 (Ollama)".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: None,
            free_tier: true,
        },
        DiscoveredModel {
            provider_id: "ollama".into(),
            upstream_model_id: "qwen2.5-coder:7b".into(),
            display_name: Some("Qwen2.5 Coder 7B (Ollama)".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: None,
            free_tier: true,
        },
        DiscoveredModel {
            provider_id: "local".into(),
            upstream_model_id: "local-model".into(),
            display_name: Some("Local Model".into()),
            endpoint_compatibility: vec!["chat".into()],
            context_window: None,
            free_tier: true,
        },
    ]
}

/// Retired upstream model IDs that must not appear in the curated catalog.
/// Covered by a maintained test so future refreshes cannot reintroduce them.
pub const RETIRED_UPSTREAM_IDS: &[&str] = &[
    "llama3-70b-8192",
    "llama3-8b-8192",
    "mixtral-8x7b-32768",
    "gemini-1.5-flash",
    "gemini-1.5-pro",
    "gemini-2.0-flash",
    "stealth/ox-alpha",
    "stealth/union-alpha",
    "deepseek-chat",
    "deepseek-reasoner",
    "llama-3.1-8b-instant",
    "llama-3.3-70b-versatile",
    "qwen/qwen3-32b",
    // Advertised as free by OpenRouter discovery but live calls return
    // 404 "unavailable for free" (verified October 2026).
    "qwen/qwen3-coder:free",
];

/// Insert curated model catalog rows into the models table so that
/// `filter_by_model_enabled` can find them. Uses INSERT OR IGNORE to
/// avoid overwriting rows already populated by provider discovery.
pub async fn seed_curated_models(db: &sqlx::SqlitePool) {
    let catalog = curated_catalog();
    for model in &catalog {
        let supports_chat = model
            .endpoint_compatibility
            .iter()
            .any(|kind| kind == "chat");
        let supports_embeddings = model
            .endpoint_compatibility
            .iter()
            .any(|kind| kind == "embeddings");
        let intelligence_metadata = crate::discovery::model_intelligence::intelligence_metadata(
            &model.provider_id,
            &model.upstream_model_id,
            model.context_window,
            supports_embeddings,
        );
        let supports_vision = intelligence_metadata
            .get("supports_vision")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let _ = sqlx::query(
            "INSERT OR IGNORE INTO models \
             (provider_id, upstream_model_id, public_model_id, enabled, free_tier, supports_chat, supports_embeddings, \
              supports_vision, metadata_json, discovered_at, updated_at) \
             VALUES (?, ?, ?, 1, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))",
        )
        .bind(&model.provider_id)
        .bind(&model.upstream_model_id)
        .bind(
            model
                .display_name
                .as_deref()
                .unwrap_or(&model.upstream_model_id),
        )
        .bind(model.free_tier)
        .bind(supports_chat)
        .bind(supports_embeddings)
        .bind(supports_vision)
        .bind(intelligence_metadata.to_string())
        .execute(db)
        .await;
    }
}
