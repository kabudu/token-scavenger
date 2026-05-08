use crate::app::state::AppState;

/// A model-group target can either be an upstream model ID that may be served
/// by any eligible provider, or a provider-qualified provider/model pair.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ModelTarget {
    pub provider_id: Option<String>,
    pub model_id: String,
}

impl ModelTarget {
    pub fn any_provider(model_id: impl Into<String>) -> Self {
        Self {
            provider_id: None,
            model_id: model_id.into(),
        }
    }

    pub fn label(&self) -> String {
        match &self.provider_id {
            Some(provider_id) => format!("{provider_id}/{}", self.model_id),
            None => self.model_id.clone(),
        }
    }
}

fn parse_model_target(value: &serde_json::Value) -> Option<ModelTarget> {
    if let Some(model_id) = value.as_str().map(str::trim).filter(|s| !s.is_empty()) {
        return Some(ModelTarget::any_provider(model_id));
    }

    let object = value.as_object()?;
    let model_id = object
        .get("model")
        .or_else(|| object.get("model_id"))
        .or_else(|| object.get("upstream_model_id"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    let provider_id = object
        .get("provider")
        .or_else(|| object.get("provider_id"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);

    Some(ModelTarget {
        provider_id,
        model_id: model_id.to_owned(),
    })
}

fn parse_model_targets(target_json: &str) -> Vec<ModelTarget> {
    match serde_json::from_str::<serde_json::Value>(target_json) {
        Ok(serde_json::Value::Array(values)) => {
            values.iter().filter_map(parse_model_target).collect()
        }
        Ok(value) => parse_model_target(&value).into_iter().collect(),
        Err(_) => {
            let model_id = target_json.trim();
            if model_id.is_empty() {
                Vec::new()
            } else {
                vec![ModelTarget::any_provider(model_id)]
            }
        }
    }
}

/// Resolve a model group to its normalized targets.
/// Returns `None` if no model group matches (use the model ID directly).
pub async fn resolve_model_group_targets(
    state: &AppState,
    model: &str,
) -> Option<Vec<ModelTarget>> {
    let result = sqlx::query_as::<_, (String,)>(
        "SELECT target_json FROM model_groups WHERE name = ? AND enabled = 1",
    )
    .bind(model)
    .fetch_optional(&state.db)
    .await
    .ok()??;

    let targets = parse_model_targets(&result.0);
    if targets.is_empty() {
        None
    } else {
        Some(targets)
    }
}

/// Resolve a model group to its target model IDs.
/// Returns `None` if no model group matches (use the model ID directly).
pub async fn resolve_model_group(state: &AppState, model: &str) -> Option<Vec<String>> {
    resolve_model_group_targets(state, model)
        .await
        .map(|targets| targets.into_iter().map(|target| target.model_id).collect())
}

/// Get all model groups from the database.
pub async fn get_all_model_groups(state: &AppState) -> Vec<serde_json::Value> {
    sqlx::query_as::<_, (String, String, bool)>(
        "SELECT name, target_json, enabled FROM model_groups ORDER BY name ASC",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|(name, target_json, enabled)| {
        let target: serde_json::Value =
            serde_json::from_str(&target_json).unwrap_or(serde_json::json!(target_json));
        serde_json::json!({
            "name": name,
            "target": target,
            "enabled": enabled
        })
    })
    .collect()
}
