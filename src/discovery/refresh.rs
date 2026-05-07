use crate::app::state::AppState;
use futures::future::join_all;
use tracing::{info, warn};

/// Refresh model discovery for all configured providers.
/// This is called on startup and on a scheduled interval.
pub async fn refresh_all(state: &AppState) {
    let config = state.config();
    let timeout = std::time::Duration::from_millis(config.server.request_timeout_ms);
    let providers: Vec<_> = config
        .providers
        .iter()
        .filter(|provider_cfg| provider_cfg.discover_models && provider_cfg.enabled)
        .cloned()
        .collect();
    let tasks = providers.into_iter().map(|provider_cfg| {
            let state = state.clone();
            async move {
                let provider_id = provider_cfg.id.clone();
                let result =
                    tokio::time::timeout(timeout, refresh_one(&state, provider_cfg)).await;
                if result.is_err() {
                    warn!(provider = %provider_id, "Discovery timed out");
                    let _ = sqlx::query(
                        "UPDATE providers SET discovery_state = 'timeout', last_error_at = datetime('now'), last_error_summary = ? WHERE provider_id = ?"
                    )
                    .bind("discovery timed out")
                    .bind(&provider_id)
                    .execute(&state.db)
                    .await;
                }
            }
        });

    join_all(tasks).await;
}

async fn refresh_one(state: &AppState, provider_cfg: crate::config::schema::ProviderConfig) {
    let adapter = match state.provider_registry.get(&provider_cfg.id).await {
        Some(adapter) => adapter,
        None => return,
    };

    upsert_provider_row(state, &provider_cfg).await;

    let ctx = crate::providers::traits::ProviderContext {
        base_url: adapter.base_url(&provider_cfg),
        api_key: provider_cfg.api_key.clone(),
        config: std::sync::Arc::new(provider_cfg.clone()),
        client: state.http_client.clone(),
    };

    info!(provider = %provider_cfg.id, "Starting model discovery");

    let _ =
        sqlx::query("INSERT INTO discovery_runs (provider_id, status) VALUES (?, 'in_progress')")
            .bind(&provider_cfg.id)
            .execute(&state.db)
            .await;

    match adapter.discover_models(&ctx).await {
        Ok(models) => {
            let upstream_count = models.len();
            let models_to_store = models
                .into_iter()
                .filter(|model| !provider_cfg.free_only || model.free_tier)
                .collect::<Vec<_>>();
            let mut stored_count = 0_i64;
            for m in &models_to_store {
                match sqlx::query(
                    "INSERT OR REPLACE INTO models (provider_id, upstream_model_id, public_model_id, enabled, free_tier, supports_chat, discovered_at, updated_at)
                     VALUES (?, ?, ?, 1, ?, 1, datetime('now'), datetime('now'))"
                )
                .bind(&m.provider_id)
                .bind(&m.upstream_model_id)
                .bind(m.display_name.as_deref().unwrap_or(&m.upstream_model_id))
                .bind(m.free_tier)
                .execute(&state.db)
                .await
                {
                    Ok(_) => stored_count += 1,
                    Err(error) => warn!(
                        provider = %m.provider_id,
                        model = %m.upstream_model_id,
                        %error,
                        "Failed to persist discovered model"
                    ),
                }
            }
            info!(
                provider = %provider_cfg.id,
                upstream_count,
                stored_count,
                free_only = provider_cfg.free_only,
                "Discovery succeeded"
            );

            let _ = sqlx::query(
                "UPDATE providers SET discovery_state = 'fresh', last_discovery_at = datetime('now'), last_success_at = datetime('now') WHERE provider_id = ?"
            )
            .bind(&provider_cfg.id)
            .execute(&state.db)
            .await;
            let _ = sqlx::query(
                "UPDATE discovery_runs SET finished_at = datetime('now'), status = 'success', models_found = ? WHERE provider_id = ? AND status = 'in_progress'"
            )
            .bind(stored_count)
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
            let _ = sqlx::query(
                "UPDATE discovery_runs SET finished_at = datetime('now'), status = 'error', error_summary = ? WHERE provider_id = ? AND status = 'in_progress'"
            )
            .bind(e.to_string())
            .bind(&provider_cfg.id)
            .execute(&state.db)
            .await;
        }
    }
}

async fn upsert_provider_row(
    state: &AppState,
    provider_cfg: &crate::config::schema::ProviderConfig,
) {
    if let Err(error) = sqlx::query(
        "INSERT INTO providers (provider_id, display_name, enabled, base_url, free_only)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(provider_id) DO UPDATE SET
             display_name = excluded.display_name,
             enabled = excluded.enabled,
             base_url = excluded.base_url,
             free_only = excluded.free_only",
    )
    .bind(&provider_cfg.id)
    .bind(&provider_cfg.id)
    .bind(provider_cfg.enabled)
    .bind(provider_cfg.base_url.as_deref())
    .bind(provider_cfg.free_only)
    .execute(&state.db)
    .await
    {
        warn!(
            provider = %provider_cfg.id,
            %error,
            "Failed to persist provider row before discovery"
        );
    }
}
