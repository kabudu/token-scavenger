use crate::api::openai::chat::NormalizedChatRequest;
use crate::app::state::AppState;
use crate::providers::traits::EndpointKind;
use crate::router::selection::RouteAttempt;
use serde::{Deserialize, Serialize};

const STALE_AFTER_HOURS: i64 = 24 * 7;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CatalogFreshness {
    Fresh,
    Stale,
    ErrorLastAttempt,
    NeverDiscovered,
    Disabled,
    Curated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelIntelligence {
    pub family: String,
    pub task_tags: Vec<String>,
    pub modalities: Vec<String>,
    pub context_window: Option<u64>,
    pub supports_tools: bool,
    pub supports_json_mode: bool,
    pub supports_reasoning: bool,
    pub supports_vision: bool,
    pub supports_embeddings: bool,
    pub freshness: CatalogFreshness,
    pub freshness_score: f64,
}

pub fn freshness_label(freshness: &CatalogFreshness) -> String {
    serde_json::to_value(freshness)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}

#[derive(Debug, Clone, Copy)]
pub struct ModelRequestRequirements {
    pub requires_tools: bool,
    pub requires_json_mode: bool,
    pub requires_vision: bool,
    pub required_context_tokens: Option<u64>,
}

impl ModelRequestRequirements {
    pub fn for_chat(request: &NormalizedChatRequest) -> Self {
        let prompt_tokens = request
            .prompt_size_hint()
            .div_ceil(4)
            .min(u64::MAX as usize) as u64;
        Self {
            requires_tools: request
                .tools
                .as_ref()
                .is_some_and(|tools| !tools.is_empty()),
            requires_json_mode: request.response_format.is_some(),
            requires_vision: request_contains_image_input(request),
            required_context_tokens: Some(
                prompt_tokens + u64::from(request.max_tokens.unwrap_or(1024)),
            ),
        }
    }

    pub fn endpoint_only(_endpoint_kind: EndpointKind) -> Self {
        Self {
            requires_tools: false,
            requires_json_mode: false,
            requires_vision: false,
            required_context_tokens: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompatibilityDecision {
    pub compatible: bool,
    pub reasons: Vec<String>,
}

pub fn infer_model_intelligence(
    provider_id: &str,
    model_id: &str,
    metadata_json: Option<&str>,
    supports_embeddings: bool,
    provider_discovery_state: Option<&str>,
    discovered_at: Option<&str>,
    updated_at: Option<&str>,
) -> ModelIntelligence {
    let metadata = metadata_json
        .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let lower = model_id.to_ascii_lowercase();
    let provider_lower = provider_id.to_ascii_lowercase();

    let context_window = metadata
        .get("context_window")
        .and_then(|value| value.as_u64())
        .or_else(|| infer_context_window(&lower));
    let supports_vision = metadata
        .get("supports_vision")
        .and_then(|value| value.as_bool())
        .unwrap_or_else(|| infer_vision(&provider_lower, &lower));
    let supports_reasoning = metadata
        .get("supports_reasoning")
        .and_then(|value| value.as_bool())
        .unwrap_or_else(|| infer_reasoning(&lower));
    let family = metadata
        .get("family")
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| infer_family(&provider_lower, &lower));
    let task_tags = metadata
        .get("task_tags")
        .and_then(|value| value.as_array())
        .map(|tags| {
            tags.iter()
                .filter_map(|tag| tag.as_str().map(ToOwned::to_owned))
                .collect::<Vec<_>>()
        })
        .filter(|tags| !tags.is_empty())
        .unwrap_or_else(|| infer_task_tags(&lower, supports_reasoning, supports_embeddings));
    let mut modalities = vec!["text".to_string()];
    if supports_vision {
        modalities.push("vision".to_string());
    }
    if supports_embeddings {
        modalities.push("embeddings".to_string());
    }

    let metadata_source = metadata.get("source").and_then(|value| value.as_str());
    let freshness = freshness(
        metadata_source,
        provider_discovery_state,
        discovered_at,
        updated_at,
    );
    let freshness_score = freshness_score(&freshness);

    ModelIntelligence {
        family,
        task_tags,
        modalities,
        context_window,
        supports_tools: metadata
            .get("supports_tools")
            .and_then(|value| value.as_bool())
            .unwrap_or(true),
        supports_json_mode: metadata
            .get("supports_json_mode")
            .and_then(|value| value.as_bool())
            .unwrap_or(true),
        supports_reasoning,
        supports_vision,
        supports_embeddings,
        freshness,
        freshness_score,
    }
}

pub async fn model_compatibility(
    state: &AppState,
    attempt: &RouteAttempt,
    requirements: ModelRequestRequirements,
) -> CompatibilityDecision {
    let row = sqlx::query_as::<
        _,
        (
            bool,
            bool,
            bool,
            bool,
            bool,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    >(
        "SELECT m.supports_chat, m.supports_embeddings, m.supports_tools, m.supports_json_mode,
                m.supports_vision, m.metadata_json, p.discovery_state, m.discovered_at, m.updated_at
         FROM models m
         LEFT JOIN providers p ON p.provider_id = m.provider_id
         WHERE m.provider_id = ? AND m.upstream_model_id = ?",
    )
    .bind(&attempt.provider_id)
    .bind(&attempt.model_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    let Some((
        supports_chat,
        supports_embeddings,
        supports_tools,
        supports_json_mode,
        supports_vision,
        metadata_json,
        provider_discovery_state,
        discovered_at,
        updated_at,
    )) = row
    else {
        return CompatibilityDecision {
            compatible: true,
            reasons: vec!["model intelligence unavailable; using provider capability".to_string()],
        };
    };

    let intelligence = infer_model_intelligence(
        &attempt.provider_id,
        &attempt.model_id,
        metadata_json.as_deref(),
        supports_embeddings,
        provider_discovery_state.as_deref(),
        discovered_at.as_deref(),
        updated_at.as_deref(),
    );

    let mut reasons = Vec::new();
    if !supports_chat {
        reasons.push("filtered by model chat capability".to_string());
    }
    if requirements.requires_tools && !(supports_tools && intelligence.supports_tools) {
        reasons.push("filtered by model tool-call capability".to_string());
    }
    if requirements.requires_json_mode && !(supports_json_mode && intelligence.supports_json_mode) {
        reasons.push("filtered by model JSON-mode capability".to_string());
    }
    if requirements.requires_vision && !(supports_vision || intelligence.supports_vision) {
        reasons.push("filtered by model vision capability".to_string());
    }
    if let (Some(required), Some(context_window)) = (
        requirements.required_context_tokens,
        intelligence.context_window,
    ) {
        if required > context_window {
            reasons.push(format!(
                "filtered by context window: required {required} > available {context_window}"
            ));
        }
    }

    CompatibilityDecision {
        compatible: reasons.is_empty(),
        reasons,
    }
}

pub async fn filter_by_model_intelligence(
    plan: Vec<RouteAttempt>,
    state: &AppState,
    requirements: ModelRequestRequirements,
) -> Vec<RouteAttempt> {
    let mut filtered = Vec::with_capacity(plan.len());
    for attempt in plan {
        let decision = model_compatibility(state, &attempt, requirements).await;
        if decision.compatible {
            filtered.push(attempt);
        } else {
            tracing::info!(
                provider = %attempt.provider_id,
                model = %attempt.model_id,
                reasons = ?decision.reasons,
                "Filtered by model intelligence"
            );
        }
    }
    filtered
}

pub fn intelligence_metadata(
    provider_id: &str,
    model_id: &str,
    context_window: Option<u64>,
    supports_embeddings: bool,
) -> serde_json::Value {
    let inferred = infer_model_intelligence(
        provider_id,
        model_id,
        Some(&serde_json::json!({"context_window": context_window}).to_string()),
        supports_embeddings,
        None,
        None,
        None,
    );
    serde_json::json!({
        "context_window": context_window,
        "family": inferred.family,
        "task_tags": inferred.task_tags,
        "modalities": inferred.modalities,
        "supports_reasoning": inferred.supports_reasoning,
        "supports_vision": inferred.supports_vision,
        "supports_tools": inferred.supports_tools,
        "supports_json_mode": inferred.supports_json_mode,
        "source": "curated"
    })
}

pub async fn seed_smart_model_groups(db: &sqlx::SqlitePool) {
    for group in smart_model_groups() {
        let _ = sqlx::query(
            "INSERT OR IGNORE INTO model_groups (name, target_json, enabled, updated_at)
             VALUES (?, ?, 1, datetime('now'))",
        )
        .bind(group.name)
        .bind(serde_json::Value::Array(group.targets).to_string())
        .execute(db)
        .await;
    }
}

struct SmartModelGroup {
    name: &'static str,
    targets: Vec<serde_json::Value>,
}

fn smart_model_groups() -> Vec<SmartModelGroup> {
    vec![
        SmartModelGroup {
            name: "fast:chat",
            targets: vec![
                target("groq", "llama3-8b-8192"),
                target("cerebras", "llama3.1-8b"),
                target("google", "gemini-2.0-flash"),
                target("ollama", "llama3.2"),
            ],
        },
        SmartModelGroup {
            name: "cheap:code",
            targets: vec![
                target("ollama", "qwen2.5-coder:7b"),
                target("mistral", "open-mistral-nemo"),
                target("siliconflow", "deepseek-ai/DeepSeek-V3"),
            ],
        },
        SmartModelGroup {
            name: "reasoning:deep",
            targets: vec![
                target("xai", "grok-4.20-reasoning"),
                target("deepseek", "deepseek-v4-pro"),
                target("siliconflow", "deepseek-ai/DeepSeek-V3"),
            ],
        },
        SmartModelGroup {
            name: "vision:balanced",
            targets: vec![
                target("google", "gemini-2.0-flash"),
                target("github-models", "gpt-4o-mini"),
            ],
        },
    ]
}

fn target(provider: &str, model: &str) -> serde_json::Value {
    serde_json::json!({"provider": provider, "model": model})
}

fn infer_family(provider_id: &str, model_id: &str) -> String {
    if model_id.contains("llama") {
        "llama".to_string()
    } else if model_id.contains("gemini") {
        "gemini".to_string()
    } else if model_id.contains("mistral") || model_id.contains("mixtral") {
        "mistral".to_string()
    } else if model_id.contains("deepseek") {
        "deepseek".to_string()
    } else if model_id.contains("qwen") {
        "qwen".to_string()
    } else if model_id.contains("grok") {
        "grok".to_string()
    } else if provider_id == "local" || provider_id == "ollama" {
        "local".to_string()
    } else {
        "general".to_string()
    }
}

fn infer_task_tags(
    model_id: &str,
    supports_reasoning: bool,
    supports_embeddings: bool,
) -> Vec<String> {
    let mut tags = vec!["chat".to_string()];
    if model_id.contains("code") || model_id.contains("coder") {
        tags.push("code".to_string());
    }
    if supports_reasoning {
        tags.push("reasoning".to_string());
    }
    if supports_embeddings {
        tags.push("embeddings".to_string());
    }
    tags
}

fn infer_context_window(model_id: &str) -> Option<u64> {
    if model_id.contains("32768") {
        Some(32_768)
    } else if model_id.contains("8192") || model_id.contains("8k") {
        Some(8_192)
    } else if model_id.contains("128k") || model_id.contains("128_000") {
        Some(128_000)
    } else if model_id.contains("gemini") {
        Some(1_048_576)
    } else if model_id.contains("grok-4") {
        Some(2_000_000)
    } else {
        None
    }
}

fn infer_vision(provider_id: &str, model_id: &str) -> bool {
    model_id.contains("vision")
        || model_id.contains("gpt-4o")
        || model_id.contains("gemini")
        || (provider_id == "google" && model_id.contains("flash"))
}

fn infer_reasoning(model_id: &str) -> bool {
    model_id.contains("reason")
        || model_id.contains("deepseek")
        || model_id.contains("grok-4")
        || model_id.contains("r1")
}

fn freshness(
    metadata_source: Option<&str>,
    provider_discovery_state: Option<&str>,
    discovered_at: Option<&str>,
    updated_at: Option<&str>,
) -> CatalogFreshness {
    if metadata_source == Some("curated") {
        return CatalogFreshness::Curated;
    }
    match provider_discovery_state {
        Some("disabled") => return CatalogFreshness::Disabled,
        Some("error_last_attempt") => return CatalogFreshness::ErrorLastAttempt,
        Some("never_discovered") => return CatalogFreshness::NeverDiscovered,
        _ => {}
    }
    if discovered_at.is_none() && updated_at.is_none() {
        return CatalogFreshness::Curated;
    }
    let Some(timestamp) = discovered_at.or(updated_at) else {
        return CatalogFreshness::Curated;
    };
    if timestamp_is_stale(timestamp) {
        CatalogFreshness::Stale
    } else {
        CatalogFreshness::Fresh
    }
}

fn freshness_score(freshness: &CatalogFreshness) -> f64 {
    match freshness {
        CatalogFreshness::Fresh => 1.0,
        CatalogFreshness::Curated => 0.75,
        CatalogFreshness::Stale => 0.45,
        CatalogFreshness::ErrorLastAttempt => 0.30,
        CatalogFreshness::NeverDiscovered => 0.20,
        CatalogFreshness::Disabled => 0.0,
    }
}

fn timestamp_is_stale(timestamp: &str) -> bool {
    let Ok(parsed) = chrono::NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S") else {
        return false;
    };
    let updated_at = parsed.and_utc();
    chrono::Utc::now()
        .signed_duration_since(updated_at)
        .num_hours()
        > STALE_AFTER_HOURS
}

fn request_contains_image_input(request: &NormalizedChatRequest) -> bool {
    request.messages.iter().any(|message| {
        let Some(content) = &message.content else {
            return false;
        };
        match content {
            serde_json::Value::Array(parts) => parts.iter().any(|part| {
                part.get("type").and_then(|value| value.as_str()) == Some("image_url")
                    || part.get("image_url").is_some()
            }),
            serde_json::Value::Object(object) => object.contains_key("image_url"),
            _ => false,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::openai::chat::{ChatMessage, NormalizedChatRequest};

    #[test]
    fn infers_intelligence_for_reasoning_and_vision_models() {
        let reasoning =
            infer_model_intelligence("xai", "grok-4.20-reasoning", None, false, None, None, None);
        assert!(reasoning.supports_reasoning);
        assert!(reasoning.task_tags.contains(&"reasoning".to_string()));

        let vision =
            infer_model_intelligence("google", "gemini-2.0-flash", None, false, None, None, None);
        assert!(vision.supports_vision);
        assert!(vision.modalities.contains(&"vision".to_string()));
    }

    #[test]
    fn detects_vision_request_requirements() {
        let request = NormalizedChatRequest {
            model: "vision:balanced".into(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: Some(serde_json::json!([
                    {"type": "text", "text": "what is this?"},
                    {"type": "image_url", "image_url": {"url": "data:image/png;base64,abc"}}
                ])),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            }],
            temperature: None,
            top_p: None,
            max_tokens: Some(256),
            stream: false,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            response_format: None,
            tools: None,
            tool_choice: None,
        };
        assert!(ModelRequestRequirements::for_chat(&request).requires_vision);
    }
}
