use crate::api::openai::models::{ModelEntry, ModelListResponse};
use crate::app::state::AppState;

/// Build the merged model list from curated catalog and discovered models.
pub async fn build_model_list(state: &AppState) -> ModelListResponse {
    let curated = crate::discovery::curated::curated_catalog();

    // Get discovered models from DB
    let discovered = get_discovered_from_db(state).await;

    // Merge: discovered overrides curated, manual overrides always win
    let mut models_map: std::collections::HashMap<(String, String), ModelEntry> =
        std::collections::HashMap::new();

    // Insert curated entries first
    for m in &curated {
        let metadata = crate::discovery::model_intelligence::intelligence_metadata(
            &m.provider_id,
            &m.upstream_model_id,
            m.context_window,
            false,
        )
        .to_string();
        models_map.insert(
            (m.provider_id.clone(), m.upstream_model_id.clone()),
            model_entry(
                &m.provider_id,
                &m.upstream_model_id,
                Some(m.free_tier),
                Some(&metadata),
                None,
                None,
                None,
            ),
        );
    }

    // Override with discovered entries
    for m in &discovered {
        let id = m
            .get("upstream_model_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let provider = m
            .get("provider_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let metadata_json = m.get("metadata_json").and_then(|v| v.as_str());
        let free_tier = m.get("free_tier").and_then(|v| v.as_bool()).or(Some(true));
        models_map.insert(
            (provider.to_string(), id.to_string()),
            model_entry(
                provider,
                id,
                free_tier,
                metadata_json,
                m.get("supports_embeddings").and_then(|v| v.as_bool()),
                m.get("discovery_state").and_then(|v| v.as_str()),
                m.get("updated_at").and_then(|v| v.as_str()),
            ),
        );
    }

    let data: Vec<ModelEntry> = models_map.into_values().collect();

    ModelListResponse {
        object: "list".into(),
        data,
    }
}

fn model_entry(
    provider: &str,
    id: &str,
    free_tier: Option<bool>,
    metadata_json: Option<&str>,
    supports_embeddings: Option<bool>,
    discovery_state: Option<&str>,
    updated_at: Option<&str>,
) -> ModelEntry {
    let intelligence = crate::discovery::model_intelligence::infer_model_intelligence(
        provider,
        id,
        metadata_json,
        supports_embeddings.unwrap_or(false),
        discovery_state,
        None,
        updated_at,
    );
    ModelEntry {
        id: id.to_string(),
        object: "model".into(),
        created: 0,
        owned_by: provider.to_string(),
        permission: vec![],
        root: None,
        provider_id: Some(provider.to_string()),
        free_tier,
        context_window: intelligence.context_window,
        task_tags: Some(intelligence.task_tags),
        modalities: Some(intelligence.modalities),
        freshness: Some(crate::discovery::model_intelligence::freshness_label(
            &intelligence.freshness,
        )),
    }
}

/// Get all models as JSON for the admin API.
pub async fn get_all_models(state: &AppState) -> serde_json::Value {
    let mut models_map: std::collections::BTreeMap<(String, String), serde_json::Value> =
        std::collections::BTreeMap::new();

    for model in crate::discovery::curated::curated_catalog() {
        models_map.insert(
            (model.provider_id.clone(), model.upstream_model_id.clone()),
            serde_json::json!({
                "provider_id": model.provider_id,
                "upstream_model_id": model.upstream_model_id,
                "public_model_id": model.display_name.unwrap_or_default(),
                "enabled": true,
                "free_tier": model.free_tier,
                "priority": 100,
                "source": "curated",
                "intelligence": crate::discovery::model_intelligence::infer_model_intelligence(
                    &model.provider_id,
                    &model.upstream_model_id,
                    Some(&crate::discovery::model_intelligence::intelligence_metadata(
                        &model.provider_id,
                        &model.upstream_model_id,
                        model.context_window,
                        false,
                    ).to_string()),
                    false,
                    None,
                    None,
                    None,
                ),
            }),
        );
    }

    let result = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            bool,
            bool,
            bool,
            bool,
            bool,
            bool,
            bool,
            i64,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    >(
        "SELECT m.provider_id, m.upstream_model_id, m.public_model_id, m.enabled, m.free_tier,
                m.supports_chat, m.supports_embeddings, m.supports_tools, m.supports_json_mode,
                m.supports_vision, m.priority, m.metadata_json, p.discovery_state,
                m.discovered_at, m.updated_at
         FROM models m
         LEFT JOIN providers p ON p.provider_id = m.provider_id
         ORDER BY m.provider_id, m.upstream_model_id",
    )
    .fetch_all(&state.db)
    .await;

    match result {
        Ok(rows) => {
            for (
                provider_id,
                upstream_model_id,
                public_model_id,
                enabled,
                free_tier,
                supports_chat,
                supports_embeddings,
                supports_tools,
                supports_json_mode,
                supports_vision,
                priority,
                metadata_json,
                discovery_state,
                discovered_at,
                updated_at,
            ) in rows
            {
                let intelligence = crate::discovery::model_intelligence::infer_model_intelligence(
                    &provider_id,
                    &upstream_model_id,
                    metadata_json.as_deref(),
                    supports_embeddings,
                    discovery_state.as_deref(),
                    discovered_at.as_deref(),
                    updated_at.as_deref(),
                );
                models_map.insert(
                    (provider_id.clone(), upstream_model_id.clone()),
                    serde_json::json!({
                        "provider_id": provider_id,
                        "upstream_model_id": upstream_model_id,
                        "public_model_id": public_model_id,
                        "enabled": enabled,
                        "free_tier": free_tier,
                        "supports_chat": supports_chat,
                        "supports_embeddings": supports_embeddings,
                        "supports_tools": supports_tools,
                        "supports_json_mode": supports_json_mode,
                        "supports_vision": supports_vision || intelligence.supports_vision,
                        "priority": priority,
                        "discovery_state": discovery_state,
                        "freshness": crate::discovery::model_intelligence::freshness_label(&intelligence.freshness),
                        "freshness_score": intelligence.freshness_score,
                        "intelligence": intelligence,
                        "source": "database",
                    }),
                );
            }
            serde_json::json!({"models": models_map.into_values().collect::<Vec<_>>()})
        }
        Err(error) => {
            tracing::warn!(%error, "Failed to load DB model catalog; returning curated catalog");
            serde_json::json!({"models": models_map.into_values().collect::<Vec<_>>()})
        }
    }
}

async fn get_discovered_from_db(state: &AppState) -> Vec<serde_json::Value> {
    let result = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            bool,
            bool,
            bool,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    >(
        "SELECT m.provider_id, m.upstream_model_id, m.public_model_id, m.enabled, m.free_tier,
                m.supports_embeddings, m.metadata_json, m.updated_at, p.discovery_state
         FROM models m
         LEFT JOIN providers p ON p.provider_id = m.provider_id
         WHERE m.enabled = 1",
    )
    .fetch_all(&state.db)
    .await;

    match result {
        Ok(rows) => rows
            .into_iter()
            .map(
                |(
                    p,
                    u,
                    pub_id,
                    _e,
                    free_tier,
                    supports_embeddings,
                    metadata_json,
                    updated_at,
                    discovery_state,
                )| {
                    serde_json::json!({
                        "provider_id": p,
                        "upstream_model_id": u,
                        "public_model_id": pub_id,
                        "free_tier": free_tier,
                        "supports_embeddings": supports_embeddings,
                        "metadata_json": metadata_json,
                        "updated_at": updated_at,
                        "discovery_state": discovery_state,
                    })
                },
            )
            .collect(),
        Err(_) => vec![],
    }
}
