use crate::app::state::AppState;
use tracing::{info, warn};

/// Refresh model discovery for all configured providers.
/// This is called on startup and on a scheduled interval.
pub async fn refresh_all(state: &AppState) {
    let config = state.config();
    let providers = config.providers.clone();

    for provider_cfg in &providers {
        if !provider_cfg.discover_models {
            continue;
        }

        let adapter = state.provider_registry.get(&provider_cfg.id).await;
        if adapter.is_none() {
            continue;
        }
        let adapter = adapter.unwrap();

        let ctx = crate::providers::traits::ProviderContext {
            base_url: adapter.base_url(provider_cfg),
            api_key: provider_cfg.api_key.clone(),
            config: std::sync::Arc::new(provider_cfg.clone()),
            client: state.http_client.clone(),
        };

        info!(provider = %provider_cfg.id, "Starting model discovery");

        match adapter.discover_models(&ctx).await {
            Ok(models) => {
                info!(provider = %provider_cfg.id, count = models.len(), "Discovery succeeded");
                // Persist to DB
                for m in &models {
                    let _ = sqlx::query(
                        "INSERT OR REPLACE INTO models (provider_id, upstream_model_id, public_model_id, enabled, free_tier, supports_chat, discovered_at, updated_at)
                         VALUES (?, ?, ?, 1, ?, 1, datetime('now'), datetime('now'))"
                    )
                    .bind(&m.provider_id)
                    .bind(&m.upstream_model_id)
                    .bind(m.display_name.as_deref().unwrap_or(&m.upstream_model_id))
                    .bind(m.free_tier)
                    .execute(&state.db)
                    .await;
                }

                // Update provider discovery state
                let _ = sqlx::query(
                    "UPDATE providers SET discovery_state = 'fresh', last_discovery_at = datetime('now'), last_success_at = datetime('now') WHERE provider_id = ?"
                )
                .bind(&provider_cfg.id)
                .execute(&state.db)
                .await;
            }
            Err(e) => {
                warn!(provider = %provider_cfg.id, error = %e, "Discovery failed");
                let _ = sqlx::query(
                    "UPDATE providers SET discovery_state = 'error_last_attempt', last_error_at = datetime('now'), last_error_summary = ? WHERE provider_id = ?"
                )
                .bind(e.to_string())
                .bind(&provider_cfg.id)
                .execute(&state.db)
                .await;
            }
        }
    }
}
