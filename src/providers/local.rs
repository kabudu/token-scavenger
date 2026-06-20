use crate::api::openai::chat::{NormalizedChatRequest, ProviderChatResponse};
use crate::api::openai::embeddings::{NormalizedEmbeddingsRequest, ProviderEmbeddingsResponse};
use crate::config::schema::ProviderConfig;
use crate::config::schema::ProviderEmbeddingSupport;
use crate::discovery::curated::DiscoveredModel;
use crate::providers::http::bearer_auth;
use crate::providers::normalization::ProviderCapabilities;
use crate::providers::shared;
use crate::providers::traits::{
    AuthKind, EndpointKind, ProviderAdapter, ProviderContext, ProviderError,
};
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::header::HeaderMap;
use std::collections::HashSet;
use std::time::Duration;
use url::Url;

const LOCAL_EMBEDDING_PROBE_CONCURRENCY: usize = 4;
const LOCAL_EMBEDDING_PROBE_TIMEOUT: Duration = Duration::from_millis(1500);

fn local_capabilities(docs_url: &'static str, quirks: Vec<String>) -> ProviderCapabilities {
    ProviderCapabilities {
        openai_compatible: true,
        has_quirks: true,
        quirks,
        supports_streaming: true,
        supports_tools: true,
        supports_json_mode: true,
        supports_vision: false,
        docs_url: Some(docs_url.into()),
    }
}

async fn probe_local_embedding_models(
    ctx: &ProviderContext,
    provider_id: &'static str,
    model_ids: Vec<String>,
) -> HashSet<String> {
    futures::stream::iter(model_ids)
        .map(|model_id| async move {
            let result = tokio::time::timeout(
                LOCAL_EMBEDDING_PROBE_TIMEOUT,
                shared::probe_openai_embeddings(ctx, &model_id, provider_id),
            )
            .await;
            match result {
                Ok(Ok(())) => Some(model_id),
                Ok(Err(error)) => {
                    tracing::debug!(
                        provider = provider_id,
                        model = %model_id,
                        error = %error,
                        "Local embeddings probe did not succeed"
                    );
                    None
                }
                Err(_) => {
                    tracing::debug!(
                        provider = provider_id,
                        model = %model_id,
                        timeout_ms = LOCAL_EMBEDDING_PROBE_TIMEOUT.as_millis(),
                        "Local embeddings probe timed out"
                    );
                    None
                }
            }
        })
        .buffer_unordered(LOCAL_EMBEDDING_PROBE_CONCURRENCY)
        .filter_map(|model_id| async move { model_id })
        .collect()
        .await
}

macro_rules! local_openai_adapter {
    ($name:ident, $id:expr, $display:expr, $base_url:expr, $docs_url:expr, [$($quirk:expr),+ $(,)?]) => {
        pub struct $name;

        #[async_trait]
        impl ProviderAdapter for $name {
            fn provider_id(&self) -> &'static str {
                $id
            }

            fn display_name(&self) -> &'static str {
                $display
            }

            fn supports_endpoint(&self, kind: &EndpointKind) -> bool {
                matches!(
                    kind,
                    EndpointKind::ChatCompletions | EndpointKind::Embeddings | EndpointKind::ModelList
                )
            }

            fn auth_kind(&self) -> AuthKind {
                AuthKind::Bearer
            }

            fn capabilities(&self) -> ProviderCapabilities {
                local_capabilities($docs_url, vec![$($quirk.into()),+])
            }

            fn base_url(&self, config: &ProviderConfig) -> Url {
                shared::provider_base_url($id, config, $base_url)
            }

            fn default_headers(&self, config: &ProviderConfig) -> HeaderMap {
                bearer_auth(config)
            }

            async fn discover_models(
                &self,
                ctx: &ProviderContext,
            ) -> Result<Vec<DiscoveredModel>, ProviderError> {
                let mut models = shared::openai_discover_models(ctx, $id).await?;
                let probed_embedding_models = match ctx.config.embedding_support {
                    ProviderEmbeddingSupport::Auto => {
                        let model_ids = models
                            .iter()
                            .map(|model| model.upstream_model_id.clone())
                            .collect::<Vec<_>>();
                        Some(probe_local_embedding_models(ctx, $id, model_ids).await)
                    }
                    _ => None,
                };
                for model in &mut models {
                    let supports_embeddings = match ctx.config.embedding_support {
                        ProviderEmbeddingSupport::Enabled => true,
                        ProviderEmbeddingSupport::Disabled => false,
                        ProviderEmbeddingSupport::Auto => probed_embedding_models
                            .as_ref()
                            .is_some_and(|model_ids| model_ids.contains(&model.upstream_model_id)),
                    };
                    if supports_embeddings
                        && !model.endpoint_compatibility.iter().any(|kind| kind == "embeddings")
                    {
                        model.endpoint_compatibility.push("embeddings".into());
                    }
                }
                Ok(models)
            }

            async fn chat_completions(
                &self,
                ctx: &ProviderContext,
                request: NormalizedChatRequest,
            ) -> Result<ProviderChatResponse, ProviderError> {
                shared::openai_chat_completions(ctx, request, $id).await
            }

            async fn embeddings(
                &self,
                ctx: &ProviderContext,
                request: NormalizedEmbeddingsRequest,
            ) -> Result<ProviderEmbeddingsResponse, ProviderError> {
                shared::openai_embeddings(ctx, request, $id).await
            }

            async fn stream_chat_completions(
                &self,
                ctx: &ProviderContext,
                request: NormalizedChatRequest,
                tx: tokio::sync::mpsc::Sender<crate::api::openai::stream::StreamEvent>,
            ) -> Result<(), ProviderError> {
                shared::openai_stream_completions(ctx, request, $id, tx).await
            }
        }
    };
}

local_openai_adapter!(
    LocalOpenAiAdapter,
    "local",
    "Local OpenAI-Compatible",
    "http://127.0.0.1:1234/v1",
    "https://platform.openai.com/docs/api-reference",
    [
        "Generic local OpenAI-compatible upstream; override base_url for your server",
        "Actual tool, JSON mode, vision, and embeddings support depends on the local server and loaded model"
    ]
);

local_openai_adapter!(
    OllamaAdapter,
    "ollama",
    "Ollama",
    "http://127.0.0.1:11434/v1",
    "https://github.com/ollama/ollama/blob/main/docs/openai.md",
    [
        "Uses Ollama's OpenAI-compatible /v1 endpoints",
        "Model availability depends on locally pulled Ollama models"
    ]
);

local_openai_adapter!(
    LlamaCppAdapter,
    "llama-cpp",
    "llama.cpp Server",
    "http://127.0.0.1:8080/v1",
    "https://github.com/ggml-org/llama.cpp/tree/master/tools/server",
    [
        "Uses the llama.cpp server OpenAI-compatible API",
        "Capabilities depend on server flags and the loaded model"
    ]
);

local_openai_adapter!(
    LmStudioAdapter,
    "lmstudio",
    "LM Studio",
    "http://127.0.0.1:1234/v1",
    "https://lmstudio.ai/docs/app/api/endpoints/openai",
    [
        "Uses LM Studio's OpenAI-compatible local server",
        "Capabilities depend on the selected local model"
    ]
);
