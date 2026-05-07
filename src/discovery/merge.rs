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
        models_map.insert(
            (m.provider_id.clone(), m.upstream_model_id.clone()),
            ModelEntry {
                id: m.upstream_model_id.clone(),
                object: "model".into(),
                created: 0,
                owned_by: m.provider_id.clone(),
                permission: vec![],
                root: None,
                provider_id: Some(m.provider_id.clone()),
                free_tier: Some(m.free_tier),
            },
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
        models_map.insert(
            (provider.to_string(), id.to_string()),
            ModelEntry {
                id: id.to_string(),
                object: "model".into(),
                created: 0,
                owned_by: provider.to_string(),
                permission: vec![],
                root: None,
                provider_id: Some(provider.to_string()),
                free_tier: Some(true),
            },
        );
    }

    let data: Vec<ModelEntry> = models_map.into_values().collect();

    ModelListResponse {
        object: "list".into(),
        data,
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
            }),
        );
    }

    let result = sqlx::query_as::<_, (String, String, String, bool, bool, i64)>(
        "SELECT provider_id, upstream_model_id, public_model_id, enabled, free_tier, priority FROM models ORDER BY provider_id, upstream_model_id",
    )
    .fetch_all(&state.db)
    .await;

    match result {
        Ok(rows) => {
            for (provider_id, upstream_model_id, public_model_id, enabled, free_tier, priority) in
                rows
            {
                models_map.insert(
                    (provider_id.clone(), upstream_model_id.clone()),
                    serde_json::json!({
                        "provider_id": provider_id,
                        "upstream_model_id": upstream_model_id,
                        "public_model_id": public_model_id,
                        "enabled": enabled,
                        "free_tier": free_tier,
                        "priority": priority,
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
    let result = sqlx::query_as::<_, (String, String, String, bool)>(
        "SELECT provider_id, upstream_model_id, public_model_id, enabled FROM models WHERE enabled = 1"
    )
    .fetch_all(&state.db)
    .await;

    match result {
        Ok(rows) => rows
            .into_iter()
            .map(|(p, u, pub_id, _e)| {
                serde_json::json!({
                    "provider_id": p,
                    "upstream_model_id": u,
                    "public_model_id": pub_id,
                })
            })
            .collect(),
        Err(_) => vec![],
    }
}
