use crate::app::state::AppState;

/// Resolve a model alias to its target model IDs.
/// Returns `None` if no alias matches (use the model ID directly).
pub async fn resolve_alias(state: &AppState, model: &str) -> Option<Vec<String>> {
    // Check DB aliases
    let result = sqlx::query_as::<_, (String,)>(
        "SELECT target_json FROM aliases WHERE alias = ? AND enabled = 1",
    )
    .bind(model)
    .fetch_optional(&state.db)
    .await
    .ok()??;

    let target_json: String = result.0;

    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&target_json) {
        if let Some(s) = v.as_str() {
            return Some(vec![s.to_string()]);
        }
        if let Some(arr) = v.as_array() {
            let models: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            if !models.is_empty() {
                return Some(models);
            }
        }
    }

    // Fallback: return the target as a single-element list if it's not JSON
    Some(vec![target_json])
}

/// Get all aliases from the database.
pub async fn get_all_aliases(state: &AppState) -> Vec<serde_json::Value> {
    sqlx::query_as::<_, (String, String, bool)>(
        "SELECT alias, target_json, enabled FROM aliases ORDER BY alias ASC",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|(alias, target_json, enabled)| {
        let target: serde_json::Value =
            serde_json::from_str(&target_json).unwrap_or(serde_json::json!(target_json));
        serde_json::json!({
            "alias": alias,
            "target": target,
            "enabled": enabled
        })
    })
    .collect()
}
