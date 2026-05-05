use crate::app::state::AppState;
use crate::providers::traits::{EndpointKind, ProviderAdapter};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ProviderCatalogEntry {
    pub id: &'static str,
    pub display_name: &'static str,
    pub default_base_url: &'static str,
    pub free_only_default: bool,
}

pub const SUPPORTED_PROVIDERS: &[ProviderCatalogEntry] = &[
    ProviderCatalogEntry {
        id: "groq",
        display_name: "Groq",
        default_base_url: "",
        free_only_default: true,
    },
    ProviderCatalogEntry {
        id: "google",
        display_name: "Google AI Studio / Gemini",
        default_base_url: "",
        free_only_default: true,
    },
    ProviderCatalogEntry {
        id: "openrouter",
        display_name: "OpenRouter",
        default_base_url: "",
        free_only_default: true,
    },
    ProviderCatalogEntry {
        id: "cloudflare",
        display_name: "Cloudflare Workers AI",
        default_base_url: "",
        free_only_default: true,
    },
    ProviderCatalogEntry {
        id: "cerebras",
        display_name: "Cerebras",
        default_base_url: "",
        free_only_default: true,
    },
    ProviderCatalogEntry {
        id: "nvidia",
        display_name: "NVIDIA NIM",
        default_base_url: "",
        free_only_default: true,
    },
    ProviderCatalogEntry {
        id: "cohere",
        display_name: "Cohere",
        default_base_url: "",
        free_only_default: true,
    },
    ProviderCatalogEntry {
        id: "mistral",
        display_name: "Mistral AI",
        default_base_url: "",
        free_only_default: true,
    },
    ProviderCatalogEntry {
        id: "github-models",
        display_name: "GitHub Models",
        default_base_url: "",
        free_only_default: true,
    },
    ProviderCatalogEntry {
        id: "huggingface",
        display_name: "Hugging Face Serverless Inference",
        default_base_url: "",
        free_only_default: true,
    },
    ProviderCatalogEntry {
        id: "zai",
        display_name: "Z AI / Zhipu AI",
        default_base_url: "",
        free_only_default: true,
    },
    ProviderCatalogEntry {
        id: "siliconflow",
        display_name: "SiliconFlow",
        default_base_url: "",
        free_only_default: true,
    },
    ProviderCatalogEntry {
        id: "deepseek",
        display_name: "DeepSeek",
        default_base_url: "https://api.deepseek.com",
        free_only_default: false,
    },
    ProviderCatalogEntry {
        id: "xai",
        display_name: "xAI / Grok",
        default_base_url: "https://api.x.ai/v1",
        free_only_default: false,
    },
];

/// Registry of all available provider adapters.
pub struct ProviderRegistry {
    providers: tokio::sync::RwLock<HashMap<String, Arc<dyn ProviderAdapter>>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: tokio::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Initialize providers from the application config.
    pub async fn init_from_config(&self, state: &AppState) {
        let config = state.config();
        let mut map = self.providers.write().await;
        map.clear();

        for provider_cfg in &config.providers {
            if !provider_cfg.enabled {
                continue;
            }
            if let Some(adapter) = create_adapter(&provider_cfg.id) {
                map.insert(provider_cfg.id.clone(), adapter);
            }
        }
    }

    /// Register a single provider adapter.
    pub async fn register(&self, adapter: Arc<dyn ProviderAdapter>) {
        let mut map = self.providers.write().await;
        map.insert(adapter.provider_id().to_string(), adapter);
    }

    /// Get a provider adapter by ID.
    pub async fn get(&self, id: &str) -> Option<Arc<dyn ProviderAdapter>> {
        let map = self.providers.read().await;
        map.get(id).cloned()
    }

    /// List all registered provider IDs.
    pub async fn list_ids(&self) -> Vec<String> {
        let map = self.providers.read().await;
        map.keys().cloned().collect()
    }

    /// List all registered provider adapters.
    pub async fn list_all(&self) -> Vec<Arc<dyn ProviderAdapter>> {
        let map = self.providers.read().await;
        map.values().cloned().collect()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a provider adapter instance by ID.
fn create_adapter(id: &str) -> Option<Arc<dyn ProviderAdapter>> {
    match id {
        "groq" => Some(Arc::new(crate::providers::groq::GroqAdapter)),
        "google" => Some(Arc::new(crate::providers::google::GoogleAdapter)),
        "openrouter" | "openrouter-free" => {
            Some(Arc::new(crate::providers::stub_adapters::OpenRouterAdapter))
        }
        "cloudflare" => Some(Arc::new(crate::providers::stub_adapters::CloudflareAdapter)),
        "cerebras" => Some(Arc::new(crate::providers::stub_adapters::CerebrasAdapter)),
        "nvidia" => Some(Arc::new(crate::providers::stub_adapters::NvidiaAdapter)),
        "cohere" => Some(Arc::new(crate::providers::stub_adapters::CohereAdapter)),
        "mistral" => Some(Arc::new(crate::providers::stub_adapters::MistralAdapter)),
        "github-models" => Some(Arc::new(
            crate::providers::stub_adapters::GitHubModelsAdapter,
        )),
        "huggingface" => Some(Arc::new(
            crate::providers::stub_adapters::HuggingFaceAdapter,
        )),
        "zai" | "zhipu" => Some(Arc::new(crate::providers::stub_adapters::ZaiAdapter)),
        "siliconflow" => Some(Arc::new(
            crate::providers::stub_adapters::SiliconFlowAdapter,
        )),
        "deepseek" => Some(Arc::new(crate::providers::stub_adapters::DeepSeekAdapter)),
        "xai" | "grok" => Some(Arc::new(crate::providers::stub_adapters::XaiAdapter)),
        _ => None,
    }
}

/// Get the provider state as JSON for the admin API.
pub async fn get_providers_state(state: &AppState) -> serde_json::Value {
    let registry = &state.provider_registry;
    let adapters = registry.list_all().await;
    let config = state.config();

    let mut providers = Vec::new();
    for adapter in &adapters {
        let pid = adapter.provider_id();
        let cfg = config.providers.iter().find(|p| p.id == pid);

        let health = state.health_states.get(pid);
        let breaker = state.breaker_states.get(pid);

        providers.push(serde_json::json!({
            "provider_id": pid,
            "display_name": adapter.display_name(),
            "enabled": cfg.map(|c| c.enabled).unwrap_or(false),
            "health_state": health.as_ref().map(|h| format!("{:?}", h.value())).unwrap_or_default(),
            "breaker_state": breaker.as_ref().map(|b| format!("{:?}", b.state())).unwrap_or_default(),
            "supports_chat": adapter.supports_endpoint(&EndpointKind::ChatCompletions),
            "supports_embeddings": adapter.supports_endpoint(&EndpointKind::Embeddings),
        }));
    }

    serde_json::json!({"providers": providers})
}
