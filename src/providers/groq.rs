use crate::api::openai::chat::{NormalizedChatRequest, ProviderChatResponse};
use crate::api::openai::embeddings::{NormalizedEmbeddingsRequest, ProviderEmbeddingsResponse};
use crate::config::schema::ProviderConfig;
use crate::discovery::curated::DiscoveredModel;
use crate::providers::http::bearer_auth;
use crate::providers::normalization::ProviderCapabilities;
use crate::providers::shared;
use crate::providers::traits::*;
use async_trait::async_trait;
use reqwest::header::HeaderMap;
use url::Url;

/// Groq (https://console.groq.com) — native OpenAI-compatible, free tier available.
pub struct GroqAdapter;

#[async_trait]
impl ProviderAdapter for GroqAdapter {
    fn provider_id(&self) -> &'static str {
        "groq"
    }
    fn display_name(&self) -> &'static str {
        "Groq"
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
            docs_url: Some("https://console.groq.com/docs".into()),
        }
    }
    fn base_url(&self, config: &ProviderConfig) -> Url {
        config
            .base_url
            .as_ref()
            .map(|u| u.parse().unwrap())
            .unwrap_or_else(|| "https://api.groq.com/openai/v1".parse().unwrap())
    }
    fn default_headers(&self, config: &ProviderConfig) -> HeaderMap {
        bearer_auth(config)
    }

    async fn discover_models(
        &self,
        ctx: &ProviderContext,
    ) -> Result<Vec<DiscoveredModel>, ProviderError> {
        shared::openai_discover_models(ctx, "groq").await
    }

    async fn chat_completions(
        &self,
        ctx: &ProviderContext,
        request: NormalizedChatRequest,
    ) -> Result<ProviderChatResponse, ProviderError> {
        shared::openai_chat_completions(ctx, request, "groq").await
    }

    async fn embeddings(
        &self,
        _ctx: &ProviderContext,
        _request: NormalizedEmbeddingsRequest,
    ) -> Result<ProviderEmbeddingsResponse, ProviderError> {
        Err(ProviderError::UnsupportedFeature(
            "Groq does not support embeddings".into(),
        ))
    }

    async fn stream_chat_completions(
        &self,
        ctx: &ProviderContext,
        request: NormalizedChatRequest,
        tx: tokio::sync::mpsc::Sender<crate::api::openai::stream::StreamEvent>,
    ) -> Result<(), ProviderError> {
        shared::openai_stream_completions(ctx, request, "groq", tx).await
    }
}
