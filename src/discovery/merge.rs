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
    let result = sqlx::query_as::<_, (String, String, String, bool)>(
        "SELECT provider_id, upstream_model_id, public_model_id, enabled FROM models ORDER BY provider_id",
    )
    .fetch_all(&state.db)
    .await;

    match result {
        Ok(rows) => {
            let models: Vec<serde_json::Value> = rows
                .into_iter()
                .map(|(p, m, public_id, e)| {
                    serde_json::json!({
                        "provider_id": p,
                        "upstream_model_id": m,
                        "public_model_id": public_id,
                        "enabled": e,
                    })
                })
                .collect();
            serde_json::json!({"models": models})
        }
        Err(_) => serde_json::json!({"models": []}),
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
