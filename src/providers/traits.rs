use crate::api::openai::chat::{NormalizedChatRequest, ProviderChatResponse};
use crate::api::openai::embeddings::{NormalizedEmbeddingsRequest, ProviderEmbeddingsResponse};
use crate::providers::normalization::ProviderCapabilities;
use async_trait::async_trait;
use reqwest::header::HeaderMap;
use url::Url;

/// Kinds of endpoints a provider can support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EndpointKind {
    ChatCompletions,
    Embeddings,
    ModelList,
}

/// Authentication schemes supported by providers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthKind {
    /// Bearer token in Authorization header.
    Bearer,
    /// Custom header-based auth (e.g., x-api-key).
    Header(String),
    /// No authentication required.
    None,
}

/// Context passed to provider adapter methods.
#[derive(Debug, Clone)]
pub struct ProviderContext {
    pub base_url: Url,
    pub api_key: Option<String>,
    pub config: std::sync::Arc<crate::config::schema::ProviderConfig>,
    pub client: reqwest::Client,
}

/// Unified provider error type.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("Timeout")]
    Timeout,
    #[error("Rate limited: retry after {retry_after:?}")]
    RateLimited { retry_after: Option<u64> },
    #[error("Quota exhausted: {details}")]
    QuotaExhausted {
        details: String,
        reset_at: Option<i64>,
    },
    #[error("Authentication failed: {0}")]
    Auth(String),
    #[error("Unsupported feature: {0}")]
    UnsupportedFeature(String),
    #[error("Malformed response: {0}")]
    MalformedResponse(String),
    #[error("Provider error: {0}")]
    Other(String),
    #[error("Unknown model: {0}")]
    UnknownModel(String),
}

/// The core trait every provider adapter must implement.
#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    /// Unique provider identifier (e.g., "groq", "google").
    fn provider_id(&self) -> &'static str;

    /// Human-readable display name.
    fn display_name(&self) -> &'static str;

    /// Which endpoints this provider supports.
    fn supports_endpoint(&self, kind: &EndpointKind) -> bool;

    /// Authentication kind.
    fn auth_kind(&self) -> AuthKind;

    /// Provider capabilities for catalog rendering.
    fn capabilities(&self) -> ProviderCapabilities;

    /// The base URL for API calls.
    fn base_url(&self, config: &crate::config::schema::ProviderConfig) -> Url;

    /// Default headers to include in every request (auth, content-type, etc.).
    fn default_headers(&self, config: &crate::config::schema::ProviderConfig) -> HeaderMap;

    /// Discover available models from the provider's model list endpoint.
    async fn discover_models(
        &self,
        ctx: &ProviderContext,
    ) -> Result<Vec<crate::discovery::curated::DiscoveredModel>, ProviderError>;

    /// Send a chat completion request and return the normalized response.
    async fn chat_completions(
        &self,
        ctx: &ProviderContext,
        request: NormalizedChatRequest,
    ) -> Result<ProviderChatResponse, ProviderError>;

    /// Send an embeddings request and return the normalized response.
    async fn embeddings(
        &self,
        ctx: &ProviderContext,
        request: NormalizedEmbeddingsRequest,
    ) -> Result<ProviderEmbeddingsResponse, ProviderError>;

    /// Stream a chat completion response by sending events into the provided channel.
    /// The adapter should send one or more StreamEvent::Chunk/Usage, then StreamEvent::Done.
    /// Default implementation calls chat_completions and sends a single Chunk + Done.
    async fn stream_chat_completions(
        &self,
        ctx: &ProviderContext,
        request: NormalizedChatRequest,
        tx: tokio::sync::mpsc::Sender<crate::api::openai::stream::StreamEvent>,
    ) -> Result<(), ProviderError> {
        let response = self.chat_completions(ctx, request).await?;
        let id = uuid::Uuid::new_v4().to_string();
        let created = chrono::Utc::now().timestamp();

        let _ = tx
            .send(crate::api::openai::stream::StreamEvent::Chunk {
                id: id.clone(),
                created,
                model: response.model_id.clone(),
                delta: crate::api::openai::chat::StreamDelta {
                    role: Some("assistant".into()),
                    content: response.content.clone(),
                },
                finish_reason: response.finish_reason.clone(),
            })
            .await;

        if let Some(usage) = response.usage {
            let _ = tx
                .send(crate::api::openai::stream::StreamEvent::Usage {
                    id,
                    created,
                    model: response.model_id,
                    prompt_tokens: usage.prompt_tokens,
                    completion_tokens: usage.completion_tokens,
                    total_tokens: usage.total_tokens,
                })
                .await;
        }

        let _ = tx.send(crate::api::openai::stream::StreamEvent::Done).await;
        Ok(())
    }
}
