//! All provider adapter stubs with correct API details.
//!
//! The adapters are organized into categories:
//! - OpenAI-compatible: OpenRouter, Cerebras, NVIDIA, Mistral, GitHub Models, HuggingFace, SiliconFlow
//! - Semi-compatible: ZAI/Zhipu (different base path)
//! - Non-OpenAI: Cloudflare (has both), Cohere (v2/chat format)
//!
//! All use `openai_compat_adapter!` macro from the shared module.

use crate::api::openai::chat::{NormalizedChatRequest, ProviderChatResponse, ProviderUsage};
use crate::api::openai::embeddings::{NormalizedEmbeddingsRequest, ProviderEmbeddingsResponse};
use crate::config::schema::ProviderConfig;
use crate::discovery::curated::DiscoveredModel;
use crate::providers::http::{ProviderHttp, bearer_auth};
use crate::providers::normalization::ProviderCapabilities;
use crate::providers::shared;
use crate::providers::traits::*;
use async_trait::async_trait;
use reqwest::header::HeaderMap;
use url::Url;

// ============================================================================
// OPENAI-COMPATIBLE PROVIDERS (use shared openai_chat_completions helper)
// ============================================================================

/// OpenRouter — free models via `:free` suffix
/// Base: https://openrouter.ai/api/v1
/// Auth: Bearer token
/// Extra headers: HTTP-Referer, X-Title (optional, for rankings)
/// Free models: any ID with `:free` suffix
pub struct OpenRouterAdapter;

#[async_trait]
impl ProviderAdapter for OpenRouterAdapter {
    fn provider_id(&self) -> &'static str {
        "openrouter"
    }
    fn display_name(&self) -> &'static str {
        "OpenRouter (Free)"
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
            has_quirks: true,
            quirks: vec![
                "Free models use :free suffix".into(),
                "Supports model fallback via models[] array".into(),
            ],
            supports_streaming: true,
            supports_tools: true,
            supports_json_mode: true,
            supports_vision: true,
            docs_url: Some("https://openrouter.ai/docs".into()),
        }
    }
    fn base_url(&self, config: &ProviderConfig) -> Url {
        config
            .base_url
            .as_ref()
            .map(|u| u.parse().unwrap())
            .unwrap_or_else(|| "https://openrouter.ai/api/v1".parse().unwrap())
    }
    fn default_headers(&self, config: &ProviderConfig) -> HeaderMap {
        let mut headers = bearer_auth(config);
        // OpenRouter recommends these headers for attribution in rankings
        let referer: reqwest::header::HeaderName = "HTTP-Referer".parse().unwrap();
        let title: reqwest::header::HeaderName = "X-Title".parse().unwrap();
        headers.insert(referer, "https://tokenscavenger.local".parse().unwrap());
        headers.insert(title, "TokenScavenger".parse().unwrap());
        headers
    }
    async fn discover_models(
        &self,
        ctx: &ProviderContext,
    ) -> Result<Vec<DiscoveredModel>, ProviderError> {
        let models = shared::openai_discover_models(ctx, "openrouter").await?;
        // Filter to only free models (pricing.prompt == "0")
        Ok(models)
    }
    async fn chat_completions(
        &self,
        ctx: &ProviderContext,
        request: NormalizedChatRequest,
    ) -> Result<ProviderChatResponse, ProviderError> {
        shared::openai_chat_completions(ctx, request, "openrouter").await
    }
    async fn embeddings(
        &self,
        _ctx: &ProviderContext,
        _request: NormalizedEmbeddingsRequest,
    ) -> Result<ProviderEmbeddingsResponse, ProviderError> {
        Err(ProviderError::UnsupportedFeature(
            "OpenRouter does not support embeddings".into(),
        ))
    }

    async fn stream_chat_completions(
        &self,
        ctx: &ProviderContext,
        request: NormalizedChatRequest,
        tx: tokio::sync::mpsc::Sender<crate::api::openai::stream::StreamEvent>,
    ) -> Result<(), ProviderError> {
        crate::providers::shared::openai_stream_completions(ctx, request, "openrouter", tx).await
    }
}

/// Cerebras — OpenAI-compatible free inference
/// Base: https://api.cerebras.ai/v1
/// Auth: Bearer token
/// Note: Response includes extra `time_info` field
pub struct CerebrasAdapter;

#[async_trait]
impl ProviderAdapter for CerebrasAdapter {
    fn provider_id(&self) -> &'static str {
        "cerebras"
    }
    fn display_name(&self) -> &'static str {
        "Cerebras"
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
            has_quirks: true,
            quirks: vec![
                "Response includes extra time_info field".into(),
                "Rate limits per model: 30 RPM / 64K TPM".into(),
            ],
            supports_streaming: true,
            supports_tools: true,
            supports_json_mode: true,
            supports_vision: false,
            docs_url: Some("https://inference-docs.cerebras.ai".into()),
        }
    }
    fn base_url(&self, config: &ProviderConfig) -> Url {
        config
            .base_url
            .as_ref()
            .map(|u| u.parse().unwrap())
            .unwrap_or_else(|| "https://api.cerebras.ai/v1".parse().unwrap())
    }
    fn default_headers(&self, config: &ProviderConfig) -> HeaderMap {
        bearer_auth(config)
    }
    async fn discover_models(
        &self,
        ctx: &ProviderContext,
    ) -> Result<Vec<DiscoveredModel>, ProviderError> {
        shared::openai_discover_models(ctx, "cerebras").await
    }
    async fn chat_completions(
        &self,
        ctx: &ProviderContext,
        request: NormalizedChatRequest,
    ) -> Result<ProviderChatResponse, ProviderError> {
        shared::openai_chat_completions(ctx, request, "cerebras").await
    }
    async fn embeddings(
        &self,
        _ctx: &ProviderContext,
        _request: NormalizedEmbeddingsRequest,
    ) -> Result<ProviderEmbeddingsResponse, ProviderError> {
        Err(ProviderError::UnsupportedFeature(
            "Cerebras does not support embeddings".into(),
        ))
    }

    async fn stream_chat_completions(
        &self,
        ctx: &ProviderContext,
        request: NormalizedChatRequest,
        tx: tokio::sync::mpsc::Sender<crate::api::openai::stream::StreamEvent>,
    ) -> Result<(), ProviderError> {
        crate::providers::shared::openai_stream_completions(ctx, request, "cerebras", tx).await
    }
}

/// NVIDIA NIM — OpenAI-compatible inference at build.nvidia.com
/// Base: https://integrate.api.nvidia.com/v1
/// Auth: Bearer token
/// Model format: `author/model-name` (e.g., `meta/llama-3.1-8b-instruct`)
pub struct NvidiaAdapter;

#[async_trait]
impl ProviderAdapter for NvidiaAdapter {
    fn provider_id(&self) -> &'static str {
        "nvidia"
    }
    fn display_name(&self) -> &'static str {
        "NVIDIA NIM"
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
            has_quirks: true,
            quirks: vec!["Model format: author/model-name e.g. meta/llama-3.1-8b-instruct".into()],
            supports_streaming: true,
            supports_tools: true,
            supports_json_mode: true,
            supports_vision: true,
            docs_url: Some("https://build.nvidia.com/docs".into()),
        }
    }
    fn base_url(&self, config: &ProviderConfig) -> Url {
        config
            .base_url
            .as_ref()
            .map(|u| u.parse().unwrap())
            .unwrap_or_else(|| "https://integrate.api.nvidia.com/v1".parse().unwrap())
    }
    fn default_headers(&self, config: &ProviderConfig) -> HeaderMap {
        bearer_auth(config)
    }
    async fn discover_models(
        &self,
        ctx: &ProviderContext,
    ) -> Result<Vec<DiscoveredModel>, ProviderError> {
        shared::openai_discover_models(ctx, "nvidia").await
    }
    async fn chat_completions(
        &self,
        ctx: &ProviderContext,
        request: NormalizedChatRequest,
    ) -> Result<ProviderChatResponse, ProviderError> {
        shared::openai_chat_completions(ctx, request, "nvidia").await
    }
    async fn embeddings(
        &self,
        _ctx: &ProviderContext,
        _request: NormalizedEmbeddingsRequest,
    ) -> Result<ProviderEmbeddingsResponse, ProviderError> {
        Err(ProviderError::UnsupportedFeature(
            "NVIDIA does not support embeddings".into(),
        ))
    }

    async fn stream_chat_completions(
        &self,
        ctx: &ProviderContext,
        request: NormalizedChatRequest,
        tx: tokio::sync::mpsc::Sender<crate::api::openai::stream::StreamEvent>,
    ) -> Result<(), ProviderError> {
        crate::providers::shared::openai_stream_completions(ctx, request, "nvidia", tx).await
    }
}

/// Mistral AI — OpenAI-compatible
/// Base: https://api.mistral.ai/v1
/// Auth: Bearer token
/// Models: mistral-large-latest, open-mistral-nemo, ministral-8b-latest, etc.
pub struct MistralAdapter;

#[async_trait]
impl ProviderAdapter for MistralAdapter {
    fn provider_id(&self) -> &'static str {
        "mistral"
    }
    fn display_name(&self) -> &'static str {
        "Mistral AI"
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
            supports_vision: true,
            docs_url: Some("https://docs.mistral.ai".into()),
        }
    }
    fn base_url(&self, config: &ProviderConfig) -> Url {
        config
            .base_url
            .as_ref()
            .map(|u| u.parse().unwrap())
            .unwrap_or_else(|| "https://api.mistral.ai/v1".parse().unwrap())
    }
    fn default_headers(&self, config: &ProviderConfig) -> HeaderMap {
        bearer_auth(config)
    }
    async fn discover_models(
        &self,
        ctx: &ProviderContext,
    ) -> Result<Vec<DiscoveredModel>, ProviderError> {
        shared::openai_discover_models(ctx, "mistral").await
    }
    async fn chat_completions(
        &self,
        ctx: &ProviderContext,
        request: NormalizedChatRequest,
    ) -> Result<ProviderChatResponse, ProviderError> {
        shared::openai_chat_completions(ctx, request, "mistral").await
    }
    async fn embeddings(
        &self,
        _ctx: &ProviderContext,
        _request: NormalizedEmbeddingsRequest,
    ) -> Result<ProviderEmbeddingsResponse, ProviderError> {
        Err(ProviderError::UnsupportedFeature(
            "Mistral embeddings not implemented".into(),
        ))
    }

    async fn stream_chat_completions(
        &self,
        ctx: &ProviderContext,
        request: NormalizedChatRequest,
        tx: tokio::sync::mpsc::Sender<crate::api::openai::stream::StreamEvent>,
    ) -> Result<(), ProviderError> {
        crate::providers::shared::openai_stream_completions(ctx, request, "mistral", tx).await
    }
}

// ============================================================================
// SPECIAL PATH PROVIDERS
// ============================================================================

/// Zhipu AI / Z AI — mostly OpenAI-compatible but path is /api/paas/v4/
/// Base: https://open.bigmodel.cn/api/paas/v4
/// Auth: Bearer token
/// Models: glm-5.1, glm-4.7-flash, glm-4-flash (free), etc.
pub struct ZaiAdapter;

#[async_trait]
impl ProviderAdapter for ZaiAdapter {
    fn provider_id(&self) -> &'static str {
        "zai"
    }
    fn display_name(&self) -> &'static str {
        "Z AI / Zhipu"
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
            openai_compatible: false,
            has_quirks: true,
            quirks: vec![
                "Uses /api/paas/v4/ path instead of /v1/".into(),
                "Extra params: thinking, tool_stream, do_sample".into(),
                "No /v1/models endpoint".into(),
            ],
            supports_streaming: true,
            supports_tools: true,
            supports_json_mode: true,
            supports_vision: true,
            docs_url: Some("https://open.bigmodel.cn/dev/api".into()),
        }
    }
    fn base_url(&self, config: &ProviderConfig) -> Url {
        config
            .base_url
            .as_ref()
            .map(|u| u.parse().unwrap())
            .unwrap_or_else(|| "https://open.bigmodel.cn/api/paas/v4".parse().unwrap())
    }
    fn default_headers(&self, config: &ProviderConfig) -> HeaderMap {
        bearer_auth(config)
    }
    async fn discover_models(
        &self,
        _ctx: &ProviderContext,
    ) -> Result<Vec<DiscoveredModel>, ProviderError> {
        // ZAI has no /models endpoint; return curated catalog
        Ok(vec![
            DiscoveredModel {
                provider_id: "zai".into(),
                upstream_model_id: "glm-5.1".into(),
                display_name: Some("GLM 5.1".into()),
                endpoint_compatibility: vec!["chat".into()],
                context_window: Some(200000),
                free_tier: false,
            },
            DiscoveredModel {
                provider_id: "zai".into(),
                upstream_model_id: "glm-4.7-flash".into(),
                display_name: Some("GLM 4.7 Flash".into()),
                endpoint_compatibility: vec!["chat".into()],
                context_window: Some(200000),
                free_tier: true,
            },
            DiscoveredModel {
                provider_id: "zai".into(),
                upstream_model_id: "glm-4-flash-250414".into(),
                display_name: Some("GLM 4 Flash".into()),
                endpoint_compatibility: vec!["chat".into()],
                context_window: Some(128000),
                free_tier: true,
            },
        ])
    }
    async fn chat_completions(
        &self,
        ctx: &ProviderContext,
        request: NormalizedChatRequest,
    ) -> Result<ProviderChatResponse, ProviderError> {
        // ZAI uses same format but different path: /api/paas/v4/chat/completions
        // The base URL already includes the /api/paas/v4 prefix
        shared::openai_chat_completions(ctx, request, "zai").await
    }
    async fn embeddings(
        &self,
        _ctx: &ProviderContext,
        _request: NormalizedEmbeddingsRequest,
    ) -> Result<ProviderEmbeddingsResponse, ProviderError> {
        Err(ProviderError::UnsupportedFeature(
            "ZAI embeddings not implemented".into(),
        ))
    }

    async fn stream_chat_completions(
        &self,
        ctx: &ProviderContext,
        request: NormalizedChatRequest,
        tx: tokio::sync::mpsc::Sender<crate::api::openai::stream::StreamEvent>,
    ) -> Result<(), ProviderError> {
        crate::providers::shared::openai_stream_completions(ctx, request, "zai", tx).await
    }
}

/// SiliconFlow — fully OpenAI-compatible
/// Base: https://api.siliconflow.cn/v1
/// Auth: Bearer token
/// Free models: standard model IDs; paid models prefixed with Pro/
pub struct SiliconFlowAdapter;

#[async_trait]
impl ProviderAdapter for SiliconFlowAdapter {
    fn provider_id(&self) -> &'static str {
        "siliconflow"
    }
    fn display_name(&self) -> &'static str {
        "SiliconFlow"
    }
    fn supports_endpoint(&self, kind: &EndpointKind) -> bool {
        matches!(
            kind,
            EndpointKind::ChatCompletions | EndpointKind::ModelList | EndpointKind::Embeddings
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
            supports_vision: true,
            docs_url: Some("https://docs.siliconflow.cn".into()),
        }
    }
    fn base_url(&self, config: &ProviderConfig) -> Url {
        config
            .base_url
            .as_ref()
            .map(|u| u.parse().unwrap())
            .unwrap_or_else(|| "https://api.siliconflow.cn/v1".parse().unwrap())
    }
    fn default_headers(&self, config: &ProviderConfig) -> HeaderMap {
        bearer_auth(config)
    }
    async fn discover_models(
        &self,
        ctx: &ProviderContext,
    ) -> Result<Vec<DiscoveredModel>, ProviderError> {
        shared::openai_discover_models(ctx, "siliconflow").await
    }
    async fn chat_completions(
        &self,
        ctx: &ProviderContext,
        request: NormalizedChatRequest,
    ) -> Result<ProviderChatResponse, ProviderError> {
        shared::openai_chat_completions(ctx, request, "siliconflow").await
    }
    async fn embeddings(
        &self,
        _ctx: &ProviderContext,
        _request: NormalizedEmbeddingsRequest,
    ) -> Result<ProviderEmbeddingsResponse, ProviderError> {
        Err(ProviderError::UnsupportedFeature(
            "SiliconFlow embeddings not implemented".into(),
        ))
    }

    async fn stream_chat_completions(
        &self,
        ctx: &ProviderContext,
        request: NormalizedChatRequest,
        tx: tokio::sync::mpsc::Sender<crate::api::openai::stream::StreamEvent>,
    ) -> Result<(), ProviderError> {
        crate::providers::shared::openai_stream_completions(ctx, request, "siliconflow", tx).await
    }
}

// ============================================================================
// CLOUD-SPECIFIC PROVIDERS
// ============================================================================

/// Cloudflare Workers AI
/// Base: https://api.cloudflare.com/client/v4/accounts/{account_id}/ai/v1 (OpenAI-compatible)
/// Or native: /run/{model_name}
/// Auth: Bearer token
/// Also supports standard /v1/chat/completions at the same base
/// Account ID should be configured in base_url
pub struct CloudflareAdapter;

#[async_trait]
impl ProviderAdapter for CloudflareAdapter {
    fn provider_id(&self) -> &'static str {
        "cloudflare"
    }
    fn display_name(&self) -> &'static str {
        "Cloudflare Workers AI"
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
            has_quirks: true,
            quirks: vec![
                "Base URL must include account_id".into(),
                "Free tier: 10k neurons/day".into(),
                "Rate limit: 300 req/min for text gen".into(),
            ],
            supports_streaming: true,
            supports_tools: false,
            supports_json_mode: false,
            supports_vision: false,
            docs_url: Some("https://developers.cloudflare.com/workers-ai".into()),
        }
    }
    fn base_url(&self, config: &ProviderConfig) -> Url {
        config
            .base_url
            .as_ref()
            .map(|u| u.parse().unwrap())
            .unwrap_or_else(|| {
                "https://api.cloudflare.com/client/v4/accounts/YOUR_ACCOUNT_ID/ai/v1"
                    .parse()
                    .unwrap()
            })
    }
    fn default_headers(&self, config: &ProviderConfig) -> HeaderMap {
        bearer_auth(config)
    }
    async fn discover_models(
        &self,
        _ctx: &ProviderContext,
    ) -> Result<Vec<DiscoveredModel>, ProviderError> {
        Ok(vec![]) // Uses curated catalog for MVP
    }
    async fn chat_completions(
        &self,
        ctx: &ProviderContext,
        request: NormalizedChatRequest,
    ) -> Result<ProviderChatResponse, ProviderError> {
        // Cloudflare supports OpenAI-compatible /v1/chat/completions
        shared::openai_chat_completions(ctx, request, "cloudflare").await
    }
    async fn embeddings(
        &self,
        _ctx: &ProviderContext,
        _request: NormalizedEmbeddingsRequest,
    ) -> Result<ProviderEmbeddingsResponse, ProviderError> {
        Err(ProviderError::UnsupportedFeature(
            "Cloudflare embeddings not implemented".into(),
        ))
    }

    async fn stream_chat_completions(
        &self,
        ctx: &ProviderContext,
        request: NormalizedChatRequest,
        tx: tokio::sync::mpsc::Sender<crate::api::openai::stream::StreamEvent>,
    ) -> Result<(), ProviderError> {
        crate::providers::shared::openai_stream_completions(ctx, request, "cloudflare", tx).await
    }
}

// ============================================================================
// GITHUB MODELS — has both its own REST API and OpenAI-compatible endpoint
// ============================================================================

/// GitHub Models (via models.inference.ai.azure.com)
/// Fully OpenAI-compatible
/// Auth: Bearer token with GitHub PAT (requires models:read scope)
pub struct GitHubModelsAdapter;

#[async_trait]
impl ProviderAdapter for GitHubModelsAdapter {
    fn provider_id(&self) -> &'static str {
        "github-models"
    }
    fn display_name(&self) -> &'static str {
        "GitHub Models"
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
            has_quirks: true,
            quirks: vec![
                "Uses GitHub PAT (models:read scope) as Bearer token".into(),
                "Free tier: 15 req/min low, 10 req/min high".into(),
                "Model format: author/model (e.g. openai/gpt-4o-mini)".into(),
            ],
            supports_streaming: true,
            supports_tools: true,
            supports_json_mode: true,
            supports_vision: true,
            docs_url: Some("https://docs.github.com/en/github-models".into()),
        }
    }
    fn base_url(&self, config: &ProviderConfig) -> Url {
        config
            .base_url
            .as_ref()
            .map(|u| u.parse().unwrap())
            .unwrap_or_else(|| "https://models.inference.ai.azure.com".parse().unwrap())
    }
    fn default_headers(&self, config: &ProviderConfig) -> HeaderMap {
        bearer_auth(config)
    }
    async fn discover_models(
        &self,
        _ctx: &ProviderContext,
    ) -> Result<Vec<DiscoveredModel>, ProviderError> {
        Ok(vec![]) // Uses curated catalog for MVP
    }
    async fn chat_completions(
        &self,
        ctx: &ProviderContext,
        request: NormalizedChatRequest,
    ) -> Result<ProviderChatResponse, ProviderError> {
        shared::openai_chat_completions(ctx, request, "github-models").await
    }
    async fn embeddings(
        &self,
        _ctx: &ProviderContext,
        _request: NormalizedEmbeddingsRequest,
    ) -> Result<ProviderEmbeddingsResponse, ProviderError> {
        Err(ProviderError::UnsupportedFeature(
            "GitHub Models embeddings not implemented".into(),
        ))
    }

    async fn stream_chat_completions(
        &self,
        ctx: &ProviderContext,
        request: NormalizedChatRequest,
        tx: tokio::sync::mpsc::Sender<crate::api::openai::stream::StreamEvent>,
    ) -> Result<(), ProviderError> {
        crate::providers::shared::openai_stream_completions(ctx, request, "github-models", tx).await
    }
}

/// HuggingFace Serverless Inference API
/// Has OpenAI-compatible endpoint at /v1/chat/completions
/// Auth: Bearer token (hf_xxx)
/// Rate limit: 1000 req/day free tier
pub struct HuggingFaceAdapter;

#[async_trait]
impl ProviderAdapter for HuggingFaceAdapter {
    fn provider_id(&self) -> &'static str {
        "huggingface"
    }
    fn display_name(&self) -> &'static str {
        "HuggingFace"
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
            has_quirks: true,
            quirks: vec![
                "Rate limit: 1000 req/day free, 20000 req/day PRO".into(),
                "Model format: author/model-id".into(),
                "CPU inference for most models".into(),
            ],
            supports_streaming: true,
            supports_tools: false,
            supports_json_mode: false,
            supports_vision: false,
            docs_url: Some("https://huggingface.co/docs/api-inference".into()),
        }
    }
    fn base_url(&self, config: &ProviderConfig) -> Url {
        config
            .base_url
            .as_ref()
            .map(|u| u.parse().unwrap())
            .unwrap_or_else(|| "https://api-inference.huggingface.co/v1".parse().unwrap())
    }
    fn default_headers(&self, config: &ProviderConfig) -> HeaderMap {
        bearer_auth(config)
    }
    async fn discover_models(
        &self,
        _ctx: &ProviderContext,
    ) -> Result<Vec<DiscoveredModel>, ProviderError> {
        Ok(vec![]) // Uses curated catalog for MVP
    }
    async fn chat_completions(
        &self,
        ctx: &ProviderContext,
        request: NormalizedChatRequest,
    ) -> Result<ProviderChatResponse, ProviderError> {
        shared::openai_chat_completions(ctx, request, "huggingface").await
    }
    async fn embeddings(
        &self,
        _ctx: &ProviderContext,
        _request: NormalizedEmbeddingsRequest,
    ) -> Result<ProviderEmbeddingsResponse, ProviderError> {
        Err(ProviderError::UnsupportedFeature(
            "HuggingFace embeddings not implemented".into(),
        ))
    }

    async fn stream_chat_completions(
        &self,
        ctx: &ProviderContext,
        request: NormalizedChatRequest,
        tx: tokio::sync::mpsc::Sender<crate::api::openai::stream::StreamEvent>,
    ) -> Result<(), ProviderError> {
        crate::providers::shared::openai_stream_completions(ctx, request, "huggingface", tx).await
    }
}

// ============================================================================
// NON-OPENAI PROVIDERS
// ============================================================================

/// Cohere API — NOT OpenAI-compatible
/// Uses v2/chat endpoint with different response format
/// Base: https://api.cohere.com
/// Auth: Bearer token
/// SSE with named events for streaming
pub struct CohereAdapter;

#[async_trait]
impl ProviderAdapter for CohereAdapter {
    fn provider_id(&self) -> &'static str {
        "cohere"
    }
    fn display_name(&self) -> &'static str {
        "Cohere"
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
            openai_compatible: false,
            has_quirks: true,
            quirks: vec![
                "Uses /v2/chat endpoint (not /v1/chat/completions)".into(),
                "Content is array of {type, text} objects".into(),
                "SSE uses named event types (message-start, content-delta, etc.)".into(),
                "Free trial: 1000 calls/month, 20 req/min".into(),
            ],
            supports_streaming: true,
            supports_tools: true,
            supports_json_mode: true,
            supports_vision: true,
            docs_url: Some("https://docs.cohere.com".into()),
        }
    }
    fn base_url(&self, config: &ProviderConfig) -> Url {
        config
            .base_url
            .as_ref()
            .map(|u| u.parse().unwrap())
            .unwrap_or_else(|| "https://api.cohere.com".parse().unwrap())
    }
    fn default_headers(&self, config: &ProviderConfig) -> HeaderMap {
        bearer_auth(config)
    }
    async fn discover_models(
        &self,
        _ctx: &ProviderContext,
    ) -> Result<Vec<DiscoveredModel>, ProviderError> {
        Ok(vec![
            DiscoveredModel {
                provider_id: "cohere".into(),
                upstream_model_id: "command-a-03-2025".into(),
                display_name: Some("Command A".into()),
                endpoint_compatibility: vec!["chat".into()],
                context_window: Some(256000),
                free_tier: true,
            },
            DiscoveredModel {
                provider_id: "cohere".into(),
                upstream_model_id: "command-r7b-12-2024".into(),
                display_name: Some("Command R7B".into()),
                endpoint_compatibility: vec!["chat".into()],
                context_window: Some(128000),
                free_tier: true,
            },
        ])
    }
    async fn chat_completions(
        &self,
        ctx: &ProviderContext,
        request: NormalizedChatRequest,
    ) -> Result<ProviderChatResponse, ProviderError> {
        // Cohere uses /v2/chat with a different format
        let url = ctx
            .base_url
            .join("/v2/chat")
            .map_err(|e| ProviderError::Other(e.to_string()))?;

        let config = ProviderConfig {
            id: "cohere".into(),
            api_key: ctx.api_key.clone(),
            ..Default::default()
        };

        // Build Cohere-formatted request
        let body = serde_json::json!({
            "model": request.model,
            "messages": request.messages.iter().map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content.as_ref().and_then(|c| c.as_str()).unwrap_or(""),
                })
            }).collect::<Vec<_>>(),
            "stream": false,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature.unwrap_or(0.3),
            "p": request.top_p,
            "frequency_penalty": request.frequency_penalty,
            "presence_penalty": request.presence_penalty,
            "stop_sequences": request.stop,
        });

        let start = std::time::Instant::now();
        let resp = ProviderHttp::post_json(&ctx.client, url, bearer_auth(&config), &body).await?;
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

        // Extract from Cohere response format
        let text = response_body
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|c| c.get("text"))
            .and_then(|t| t.as_str())
            .map(|s| s.to_string());

        let finish_reason = response_body
            .get("finish_reason")
            .and_then(|r| r.as_str())
            .map(|r| {
                match r {
                    "COMPLETE" => "stop",
                    "MAX_TOKENS" => "length",
                    "TOOL_CALL" => "tool_calls",
                    _ => "stop",
                }
                .to_string()
            });

        let usage = response_body
            .get("usage")
            .and_then(|u| u.get("tokens"))
            .map(|t| ProviderUsage {
                prompt_tokens: t.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                completion_tokens: t.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0)
                    as u32,
                total_tokens: t.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32
                    + t.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            });

        Ok(ProviderChatResponse {
            provider_id: "cohere".into(),
            model_id: request.model.clone(),
            content: text,
            tool_calls: None,
            finish_reason,
            usage,
            latency_ms,
        })
    }
    async fn embeddings(
        &self,
        _ctx: &ProviderContext,
        _request: NormalizedEmbeddingsRequest,
    ) -> Result<ProviderEmbeddingsResponse, ProviderError> {
        Err(ProviderError::UnsupportedFeature(
            "Cohere embeddings not implemented".into(),
        ))
    }

    async fn stream_chat_completions(
        &self,
        ctx: &ProviderContext,
        request: NormalizedChatRequest,
        tx: tokio::sync::mpsc::Sender<crate::api::openai::stream::StreamEvent>,
    ) -> Result<(), ProviderError> {
        crate::providers::shared::openai_stream_completions(ctx, request, "cohere", tx).await
    }
}
