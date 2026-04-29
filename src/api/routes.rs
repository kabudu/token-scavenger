use crate::api::error::ApiError;
use crate::app::state::AppState;
use axum::response::sse::Event;
use axum::{
    extract::State,
    http::HeaderMap,
    response::{Html, IntoResponse, Json, Sse},
};
use futures::stream::Stream;
use std::convert::Infallible;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

/// GET /healthz — simple liveness check.
pub async fn healthz() -> &'static str {
    "ok"
}

/// GET /readyz — readiness check.
pub async fn readyz(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    let config = state.config();

    // Check that at least one provider is configured
    let providers_ready = !config.providers.is_empty();

    Ok(Json(serde_json::json!({
        "status": if providers_ready { "ready" } else { "not_ready" },
        "providers_configured": config.providers.len(),
        "uptime_secs": state.start_time.elapsed().as_secs(),
    })))
}

/// GET /metrics — Prometheus metrics endpoint.
pub async fn metrics() -> Result<String, ApiError> {
    // Delegate to the metrics subsystem
    Ok(crate::metrics::prometheus::render_metrics())
}

/// POST /v1/chat/completions — OpenAI-compatible chat completions.
pub async fn chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(req): axum::Json<crate::api::openai::chat::ChatRequest>,
) -> Result<axum::response::Response, ApiError> {
    let normalized = crate::api::openai::chat::NormalizedChatRequest::from_request(req);
    let request_id = request_id_from_headers(&headers);

    if normalized.stream {
        // Streaming path
        let stream = crate::api::openai::stream::create_chat_stream(state, normalized).await?;
        Ok(Sse::new(stream).into_response())
    } else {
        // Non-streaming path
        let response =
            crate::router::engine::route_chat_request(state, normalized, request_id).await?;
        Ok(Json(response).into_response())
    }
}

/// POST /v1/embeddings — OpenAI-compatible embeddings.
pub async fn embeddings(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(req): axum::Json<crate::api::openai::embeddings::EmbeddingsRequest>,
) -> Result<Json<crate::api::openai::embeddings::EmbeddingsResponse>, ApiError> {
    let normalized = crate::api::openai::embeddings::NormalizedEmbeddingsRequest::from_request(req);
    let request_id = request_id_from_headers(&headers);
    let response =
        crate::router::engine::route_embeddings_request(state, normalized, request_id).await?;
    Ok(Json(response))
}

fn request_id_from_headers(headers: &HeaderMap) -> String {
    headers
        .get("X-Request-Id")
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
}

/// GET /v1/models — OpenAI-compatible model listing.
pub async fn models(
    State(state): State<AppState>,
) -> Result<Json<crate::api::openai::models::ModelListResponse>, ApiError> {
    let response = crate::discovery::merge::build_model_list(&state).await;
    Ok(Json(response))
}

/// GET /ui — web UI index page.
pub async fn ui_index(State(state): State<AppState>) -> Result<Html<String>, ApiError> {
    let html = crate::ui::routes::render_dashboard(&state).await;
    Ok(Html(html))
}

/// GET /ui/*path — web UI static assets and views.
pub async fn ui_static(
    axum::extract::Path(path): axum::extract::Path<String>,
    State(state): State<AppState>,
) -> Result<axum::response::Response, ApiError> {
    let content = match path.as_str() {
        "" | "index.html" => crate::ui::routes::render_dashboard(&state).await,
        "providers" => crate::ui::routes::render_providers(&state).await,
        "models" => crate::ui::routes::render_models(&state).await,
        "routing" => crate::ui::routes::render_routing(&state).await,
        "usage" => crate::ui::routes::render_usage(&state).await,
        "health" => crate::ui::routes::render_health(&state).await,
        "logs" => crate::ui::routes::render_logs(&state).await,
        "config" => crate::ui::routes::render_config(&state).await,
        "audit" => crate::ui::routes::render_audit(&state).await,
        _ => {
            return Err(ApiError::InvalidRequest(format!(
                "Unknown UI path: {}",
                path
            )));
        }
    };

    Ok(Html(content).into_response())
}

// Admin endpoints.
pub async fn admin_providers(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let providers = crate::providers::registry::get_providers_state(&state).await;
    Ok(Json(providers))
}

pub async fn admin_config(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let config = state.config();
    let value = serde_json::to_value(&*config).unwrap_or_default();
    Ok(Json(crate::util::redact::redact_json_value(value)))
}

pub async fn admin_models(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let models = crate::discovery::merge::get_all_models(&state).await;
    Ok(Json(models))
}

pub async fn admin_usage_series(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let usage = crate::usage::aggregation::get_usage_series(&state).await;
    Ok(Json(usage))
}

pub async fn admin_logs_stream(
    State(state): State<AppState>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let rx = state.log_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(msg) => Some(Ok(Event::default().data(msg))),
        Err(_) => None,
    });
    Ok(Sse::new(stream))
}

pub async fn admin_health_events(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let events = crate::resilience::health::get_recent_events(&state).await;
    Ok(Json(events))
}

pub async fn admin_audit(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let entries = crate::db::models::get_audit_entries(&state).await;
    Ok(Json(entries))
}

/// POST /admin/providers/discovery/refresh — trigger manual model discovery
pub async fn admin_discovery_refresh(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    crate::discovery::refresh::refresh_all(&state).await;
    Ok(Json(
        serde_json::json!({"status": "ok", "message": "Discovery refresh triggered"}),
    ))
}

/// POST /admin/providers/:id/test — test a provider connection
pub async fn admin_provider_test(
    State(state): State<AppState>,
    axum::extract::Path(provider_id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let adapter = state.provider_registry.get(&provider_id).await;
    if adapter.is_none() {
        return Err(ApiError::InvalidRequest(format!(
            "Provider '{}' not found",
            provider_id
        )));
    }
    let adapter = adapter.unwrap();

    let config = state.config();
    let provider_cfg = config
        .providers
        .iter()
        .find(|p| p.id == provider_id)
        .ok_or_else(|| {
            ApiError::InvalidRequest(format!("Provider '{}' not configured", provider_id))
        })?;

    let ctx = crate::providers::traits::ProviderContext {
        base_url: adapter.base_url(provider_cfg),
        api_key: provider_cfg.api_key.clone(),
        config: std::sync::Arc::new(provider_cfg.clone()),
        client: state.http_client.clone(),
    };

    // Try model discovery as a connectivity test
    match adapter.discover_models(&ctx).await {
        Ok(models) => Ok(Json(serde_json::json!({
            "status": "ok",
            "message": format!("Provider {} is reachable", provider_id),
            "models_found": models.len()
        }))),
        Err(e) => Ok(Json(serde_json::json!({
            "status": "error",
            "message": format!("Provider {} is unreachable: {}", provider_id, e),
            "models_found": 0
        }))),
    }
}

/// PUT /admin/config — save operator config changes
pub async fn admin_config_save(
    State(state): State<AppState>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Validate and apply config changes
    // Currently supports toggling provider enabled/disabled
    if let Some(providers) = body.get("providers").and_then(|p| p.as_array()) {
        let mut config = (*state.runtime_config.load_full()).clone();
        for provider_update in providers {
            if let (Some(id), Some(enabled)) = (
                provider_update.get("id").and_then(|v| v.as_str()),
                provider_update.get("enabled").and_then(|v| v.as_bool()),
            ) {
                if let Some(provider) = config.providers.iter_mut().find(|p| p.id == id) {
                    provider.enabled = enabled;
                }
                // Persist to DB
                let _ = sqlx::query(
                    "INSERT OR REPLACE INTO providers (provider_id, display_name, enabled) VALUES (?, ?, ?)"
                )
                .bind(id)
                .bind(id)
                .bind(enabled)
                .execute(&state.db)
                .await;
            }
        }
        let config = std::sync::Arc::new(config);
        state.runtime_config.store(config.clone());
        let _ = state.config_watch_tx.send(config);
        state.provider_registry.init_from_config(&state).await;
    }

    if let Some(models) = body.get("models").and_then(|m| m.as_array()) {
        for model_update in models {
            if let (Some(provider_id), Some(model_id), Some(enabled)) = (
                model_update.get("provider_id").and_then(|v| v.as_str()),
                model_update.get("model_id").and_then(|v| v.as_str()),
                model_update.get("enabled").and_then(|v| v.as_bool()),
            ) {
                let _ = sqlx::query(
                    "INSERT INTO models (provider_id, upstream_model_id, public_model_id, enabled, updated_at)
                     VALUES (?, ?, ?, ?, datetime('now'))
                     ON CONFLICT(provider_id, upstream_model_id)
                     DO UPDATE SET enabled = excluded.enabled, updated_at = datetime('now')",
                )
                .bind(provider_id)
                .bind(model_id)
                .bind(model_id)
                .bind(enabled)
                .execute(&state.db)
                .await;
            }
        }
    }

    if let Some(aliases) = body.get("aliases").and_then(|a| a.as_array()) {
        for alias_update in aliases {
            if let (Some(alias), Some(target)) = (
                alias_update.get("alias").and_then(|v| v.as_str()),
                alias_update.get("target"),
            ) {
                let enabled = alias_update
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let _ = sqlx::query(
                    "INSERT INTO aliases (alias, target_json, enabled, updated_at)
                     VALUES (?, ?, ?, datetime('now'))
                     ON CONFLICT(alias)
                     DO UPDATE SET target_json = excluded.target_json, enabled = excluded.enabled, updated_at = datetime('now')",
                )
                .bind(alias)
                .bind(target.to_string())
                .bind(enabled)
                .execute(&state.db)
                .await;
            }
        }
    }

    // Record audit entry
    let _ =
        sqlx::query("INSERT INTO config_audit_log (actor, action, target_type) VALUES (?, ?, ?)")
            .bind("operator")
            .bind("config_update")
            .bind("config")
            .execute(&state.db)
            .await;

    Ok(Json(
        serde_json::json!({"status": "ok", "message": "Config saved"}),
    ))
}
