use axum::{
    extract::State,
    response::{Html, IntoResponse, Json, Sse},
    routing::get,
    Router,
};
use axum::response::sse::Event;
use futures::stream::Stream;
use std::convert::Infallible;
use std::sync::Arc;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tracing::{info, warn};
use crate::api::error::ApiError;
use crate::app::state::AppState;

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
    axum::Json(req): axum::Json<crate::api::openai::chat::ChatRequest>,
) -> Result<axum::response::Response, ApiError> {
    let normalized = crate::api::openai::chat::NormalizedChatRequest::from_request(req);

    if normalized.stream {
        // Streaming path
        let stream = crate::api::openai::stream::create_chat_stream(state, normalized).await?;
        Ok(Sse::new(stream).into_response())
    } else {
        // Non-streaming path
        let response = crate::router::engine::route_chat_request(state, normalized).await?;
        Ok(Json(response).into_response())
    }
}

/// POST /v1/embeddings — OpenAI-compatible embeddings.
pub async fn embeddings(
    State(state): State<AppState>,
    axum::Json(req): axum::Json<crate::api::openai::embeddings::EmbeddingsRequest>,
) -> Result<Json<crate::api::openai::embeddings::EmbeddingsResponse>, ApiError> {
    let normalized = crate::api::openai::embeddings::NormalizedEmbeddingsRequest::from_request(req);
    let response = crate::router::engine::route_embeddings_request(state, normalized).await?;
    Ok(Json(response))
}

/// GET /v1/models — OpenAI-compatible model listing.
pub async fn models(
    State(state): State<AppState>,
) -> Result<Json<crate::api::openai::models::ModelListResponse>, ApiError> {
    let response = crate::discovery::merge::build_model_list(&state).await;
    Ok(Json(response))
}

/// GET /ui — web UI index page.
pub async fn ui_index(
    State(state): State<AppState>,
) -> Result<Html<String>, ApiError> {
    let html = crate::ui::routes::render_dashboard(&state).await;
    Ok(Html(html))
}

/// GET /ui/*path — web UI static assets and views.
pub async fn ui_static(
    axum::extract::Path(path): axum::extract::Path<String>,
    State(state): State<AppState>,
) -> Result<axum::response::Response, ApiError> {
    use axum::http::StatusCode;
    use axum::response::Response;

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
        _ => return Err(ApiError::InvalidRequest(format!("Unknown UI path: {}", path))),
    };

    Ok(Html(content).into_response())
}

/// Admin endpoints

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
    Ok(Json(serde_json::to_value(&*config).unwrap_or_default()))
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
    let stream = BroadcastStream::new(rx).filter_map(|result| {
        match result {
            Ok(msg) => Some(Ok(Event::default().data(msg))),
            Err(_) => None,
        }
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
