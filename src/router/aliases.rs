use crate::app::state::AppState;

/// Resolve a model alias to its target model ID.
/// Returns `None` if no alias matches (use the model ID directly).
pub async fn resolve_alias(state: &AppState, model: &str) -> Option<String> {
    // Check DB aliases
    let result = sqlx::query_as::<_, (String,)>(
        "SELECT target_json FROM aliases WHERE alias = ? AND enabled = 1",
    )
    .bind(model)
    .fetch_optional(&state.db)
    .await
    .ok()??;

    let target_json: String = result.0;
    // If target_json is a simple string, return it
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&target_json) {
        if let Some(s) = v.as_str() {
            return Some(s.to_string());
        }
        // If it's an array, return the first
        if let Some(arr) = v.as_array() {
            if let Some(first) = arr.first().and_then(|v| v.as_str()) {
                return Some(first.to_string());
            }
        }
    }

    // Fallback: return the target as-is
    Some(target_json)
}
