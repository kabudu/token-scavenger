use crate::api::error::ApiError;
use crate::app::state::AppState;
use axum::response::sse::Event;
use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, header},
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
        "chat" => crate::ui::routes::render_chat(&state).await,
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

/// GET /ui/logo.png — serves the project logo.
pub async fn ui_logo() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "image/png")],
        include_bytes!("../../resources/TokenScavengerLogo.png"),
    )
}

/// GET /favicon.ico — serves the project logo as favicon.
pub async fn favicon() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "image/png")],
        include_bytes!("../../resources/TokenScavengerLogo.png"),
    )
}

// Admin endpoints.
pub async fn admin_providers(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let providers = crate::providers::registry::get_providers_state(&state).await;
    Ok(Json(providers))
}

pub async fn admin_session(
    State(state): State<AppState>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Result<axum::response::Response, ApiError> {
    let config = state.config();
    if config.server.master_api_key.is_empty() || !config.server.ui_session_auth {
        return Err(ApiError::InvalidRequest(
            "UI session auth is not enabled".into(),
        ));
    }
    let key = body
        .get("api_key")
        .and_then(|v| v.as_str())
        .ok_or(ApiError::AuthError)?;
    if key != config.server.master_api_key {
        return Err(ApiError::AuthError);
    }

    let token = uuid::Uuid::new_v4().to_string();
    state
        .ui_sessions
        .insert(token.clone(), chrono::Utc::now().timestamp());
    let cookie = format!("tokenscavenger_session={token}; HttpOnly; SameSite=Lax; Path=/");
    let mut response = Json(serde_json::json!({"status": "ok"})).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie)
            .map_err(|e| ApiError::InternalError(format!("invalid session cookie: {e}")))?,
    );
    Ok(response)
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
    let usage = crate::usage::aggregation::get_usage_series(&state, "24h").await;
    Ok(Json(usage))
}

pub async fn admin_logs_stream(
    State(state): State<AppState>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    // Subscribe BEFORE emitting any trace so events aren't missed.
    let rx = {
        let guard = state.log_tx.lock().unwrap();
        guard
            .as_ref()
            .expect("log_tx sender taken during shutdown")
            .subscribe()
    };

    // Notify via tracing (will reach all subscribers including this new one).
    tracing::info!("SSE client connected to system stream");

    let log_stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(msg) => Some(Ok(Event::default().data(msg))),
        Err(_) => None,
    });

    // Periodic keepalive: send an SSE comment every 15 s to prevent browser timeout.
    let keepalive = tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(
        std::time::Duration::from_secs(15),
    ))
    .map(|_| Ok(Event::default().comment("keepalive")));

    let initial = futures::stream::iter(std::iter::once(Ok(
        Event::default().data("Connected to system stream")
    )));

    let mut shutdown_rx = state.shutdown_rx.clone();

    Ok(Sse::new(futures::StreamExt::take_until(
        initial.chain(log_stream.merge(keepalive)),
        async move {
            let _ = shutdown_rx.changed().await;
        },
    )))
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

pub async fn admin_route_plan(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let model = params
        .get("model")
        .cloned()
        .unwrap_or_else(|| "default".to_string());
    let endpoint = params.get("endpoint").map(String::as_str).unwrap_or("chat");
    let endpoint_kind = if endpoint == "embeddings" {
        crate::providers::traits::EndpointKind::Embeddings
    } else {
        crate::providers::traits::EndpointKind::ChatCompletions
    };

    let config = state.config();
    let resolved_models = crate::router::model_groups::resolve_model_group(&state, &model)
        .await
        .unwrap_or_else(|| vec![model.clone()]);
    let policy = crate::router::policy::RoutePolicy::from_config(&config);

    let mut raw_plan = Vec::new();
    for resolved in &resolved_models {
        let plan = crate::router::selection::build_attempt_plan(
            &policy,
            &state.provider_registry,
            resolved,
            endpoint_kind,
        )
        .await;
        raw_plan.extend(plan);
    }

    let mut attempts = Vec::new();
    for attempt in raw_plan {
        let health = state
            .health_states
            .get(&attempt.provider_id)
            .map(|h| format!("{:?}", h.value().state))
            .unwrap_or_else(|| "Unknown".to_string());
        let breaker = state
            .breaker_states
            .get(&attempt.provider_id)
            .map(|b| format!("{:?}", b.state()))
            .unwrap_or_else(|| "Closed".to_string());
        let model_enabled = sqlx::query_as::<_, (bool,)>(
            "SELECT enabled FROM models WHERE provider_id = ? AND upstream_model_id = ?",
        )
        .bind(&attempt.provider_id)
        .bind(&attempt.model_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .map(|row| row.0)
        .unwrap_or(true);
        let free_only = config
            .providers
            .iter()
            .find(|provider| provider.id == attempt.provider_id)
            .map(|provider| provider.free_only)
            .unwrap_or(true);
        let paid_allowed = free_only || config.routing.allow_paid_fallback;
        let included = model_enabled
            && paid_allowed
            && health != "Unhealthy"
            && breaker != "Open"
            && breaker != "HalfOpen";
        let reason = if included {
            "eligible"
        } else if !paid_allowed {
            "filtered by paid fallback policy"
        } else {
            "filtered by health, breaker, or model enablement"
        };
        attempts.push(serde_json::json!({
            "provider_id": attempt.provider_id,
            "model_id": attempt.model_id,
            "priority": attempt.priority,
            "health": health,
            "breaker_state": breaker,
            "model_enabled": model_enabled,
            "free_only": free_only,
            "included": included,
            "reason": reason
        }));
    }

    Ok(Json(serde_json::json!({
        "requested_model": model,
        "resolved_model": resolved_models.join(", "),
        "endpoint": endpoint,
        "free_first": config.routing.free_first,
        "allow_paid_fallback": config.routing.allow_paid_fallback,
        "attempts": attempts
    })))
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

/// PUT /admin/config — save operator config changes (hot-reloads without restart)
pub async fn admin_config_save(
    State(state): State<AppState>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut config = (*state.runtime_config.load_full()).clone();
    let before_config = config.clone();
    let mut changed = false;
    let config_path = &state.boot_config_file;

    // --- Server settings ---
    if let Some(server) = body.get("server") {
        if let Some(bind) = server.get("bind").and_then(|v| v.as_str()) {
            config.server.bind = bind.to_string();
            changed = true;
        }
        if let Some(key) = server.get("master_api_key").and_then(|v| v.as_str()) {
            config.server.master_api_key = key.to_string();
            changed = true;
        }
        if let Some(cors) = server
            .get("allowed_cors_origins")
            .and_then(|v| v.as_array())
        {
            config.server.allowed_cors_origins = cors
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            changed = true;
        }
        if let Some(ui) = server.get("ui_enabled").and_then(|v| v.as_bool()) {
            config.server.ui_enabled = ui;
            changed = true;
        }
    }

    // --- Routing settings ---
    if let Some(routing) = body.get("routing") {
        if let Some(free) = routing.get("free_first").and_then(|v| v.as_bool()) {
            config.routing.free_first = free;
            changed = true;
        }
        if let Some(paid) = routing.get("allow_paid_fallback").and_then(|v| v.as_bool()) {
            config.routing.allow_paid_fallback = paid;
            changed = true;
        }
        if let Some(order) = routing.get("provider_order").and_then(|v| v.as_array()) {
            config.routing.provider_order = order
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            changed = true;
        }
    }

    // --- Resilience settings ---
    if let Some(resilience) = body.get("resilience") {
        if let Some(v) = resilience
            .get("max_retries_per_provider")
            .and_then(|v| v.as_u64())
        {
            config.resilience.max_retries_per_provider = v as u32;
            changed = true;
        }
        if let Some(v) = resilience
            .get("breaker_failure_threshold")
            .and_then(|v| v.as_u64())
        {
            config.resilience.breaker_failure_threshold = v as u32;
            changed = true;
        }
        if let Some(v) = resilience
            .get("breaker_cooldown_secs")
            .and_then(|v| v.as_u64())
        {
            config.resilience.breaker_cooldown_secs = v;
            changed = true;
        }
        if let Some(v) = resilience
            .get("health_probe_interval_secs")
            .and_then(|v| v.as_u64())
        {
            config.resilience.health_probe_interval_secs = v;
            changed = true;
        }
    }

    // --- Providers (toggle, add, update) ---
    if let Some(providers) = body.get("providers").and_then(|p| p.as_array()) {
        for provider_update in providers {
            if let Some(id) = provider_update.get("id").and_then(|v| v.as_str()) {
                let enabled = provider_update.get("enabled");
                let api_key = provider_update
                    .get("api_key")
                    .and_then(|v| v.as_str())
                    .and_then(|s| if s.is_empty() { None } else { Some(s) });
                let base_url = provider_update
                    .get("base_url")
                    .and_then(|v| v.as_str())
                    .and_then(|s| if s.is_empty() { None } else { Some(s) });
                let free_only = provider_update.get("free_only").and_then(|v| v.as_bool());
                let is_removal = provider_update
                    .get("remove")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                if is_removal {
                    // Remove provider entirely
                    config.providers.retain(|p| p.id != id);
                    let _ = sqlx::query("DELETE FROM providers WHERE provider_id = ?")
                        .bind(id)
                        .execute(&state.db)
                        .await;
                    changed = true;
                } else if let Some(provider) = config.providers.iter_mut().find(|p| p.id == id) {
                    // Update existing
                    if let Some(e) = enabled.and_then(|v| v.as_bool()) {
                        provider.enabled = e;
                    }
                    if let Some(k) = api_key {
                        provider.api_key = Some(k.to_string());
                    }
                    if let Some(u) = base_url {
                        provider.base_url = Some(u.to_string());
                    }
                    if let Some(f) = free_only {
                        provider.free_only = f;
                    }
                    // Persist provider state to DB
                    let display_name = &provider.id;
                    let _ = sqlx::query(
                        "INSERT OR REPLACE INTO providers (provider_id, display_name, enabled) VALUES (?, ?, ?)"
                    )
                    .bind(id)
                    .bind(display_name)
                    .bind(provider.enabled)
                    .execute(&state.db)
                    .await;
                    changed = true;
                } else if let Some(e) = enabled.and_then(|v| v.as_bool()).or(Some(true)) {
                    // New provider (add)
                    let new_provider = crate::config::schema::ProviderConfig {
                        id: id.to_string(),
                        enabled: e,
                        base_url: base_url.map(String::from),
                        api_key: api_key.map(String::from),
                        free_only: free_only.unwrap_or(true),
                        discover_models: true,
                    };
                    config.providers.push(new_provider);
                    let _ = sqlx::query(
                        "INSERT OR REPLACE INTO providers (provider_id, display_name, enabled) VALUES (?, ?, ?)"
                    )
                    .bind(id)
                    .bind(id)
                    .bind(e)
                    .execute(&state.db)
                    .await;
                    changed = true;
                }
            }
        }
    }

    // --- Models (existing logic, unchanged) ---
    if let Some(models) = body.get("models").and_then(|m| m.as_array()) {
        for model_update in models {
            if let (Some(provider_id), Some(model_id)) = (
                model_update.get("provider_id").and_then(|v| v.as_str()),
                model_update.get("model_id").and_then(|v| v.as_str()),
            ) {
                let enabled = model_update.get("enabled").and_then(|v| v.as_bool());
                let priority = model_update.get("priority").and_then(|v| v.as_i64());

                if enabled.is_some() || priority.is_some() {
                    let mut query = String::from(
                        "INSERT INTO models (provider_id, upstream_model_id, public_model_id, updated_at",
                    );
                    let mut values = String::from(") VALUES (?, ?, ?, datetime('now')");
                    let mut update = String::from(
                        " ON CONFLICT(provider_id, upstream_model_id) DO UPDATE SET updated_at = datetime('now')",
                    );

                    if enabled.is_some() {
                        query.push_str(", enabled");
                        values.push_str(", ?");
                        update.push_str(", enabled = excluded.enabled");
                    }
                    if priority.is_some() {
                        query.push_str(", priority");
                        values.push_str(", ?");
                        update.push_str(", priority = excluded.priority");
                    }

                    let full_query = format!("{} {}) {}", query, values, update);
                    let mut sql = sqlx::query(&full_query)
                        .bind(provider_id)
                        .bind(model_id)
                        .bind(model_id);

                    if let Some(e) = enabled {
                        sql = sql.bind(e);
                    }
                    if let Some(p) = priority {
                        sql = sql.bind(p);
                    }

                    if let Err(e) = sql.execute(&state.db).await {
                        tracing::error!("Failed to update model priority/enabled: {:?}", e);
                    } else {
                        changed = true;
                    }
                }
            }
        }
    }

    // --- Model groups ---
    if let Some(model_groups) = body.get("model_groups").and_then(|a| a.as_array()) {
        for model_group_update in model_groups {
            if let (Some(name), Some(target)) = (
                model_group_update.get("name").and_then(|v| v.as_str()),
                model_group_update.get("target"),
            ) {
                let enabled = model_group_update
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let _ = sqlx::query(
                    "INSERT INTO model_groups (name, target_json, enabled, updated_at)
                     VALUES (?, ?, ?, datetime('now'))
                     ON CONFLICT(name)
                     DO UPDATE SET target_json = excluded.target_json, enabled = excluded.enabled, updated_at = datetime('now')",
                )
                .bind(name)
                .bind(target.to_string())
                .bind(enabled)
                .execute(&state.db)
                .await;
                changed = true;
            }
        }
    }

    if changed {
        let validation = crate::config::validation::validate_config(&config);
        if !validation.errors.is_empty() {
            let _ = sqlx::query(
                "INSERT INTO config_audit_log (actor, action, target_type, before_json, after_json) VALUES (?, ?, ?, ?, ?)",
            )
            .bind("operator")
            .bind("config_update_rejected")
            .bind("config")
            .bind(serde_json::to_string(&before_config).unwrap_or_default())
            .bind(serde_json::json!({"errors": validation.errors}).to_string())
            .execute(&state.db)
            .await;
            return Err(ApiError::InvalidRequest(
                "Config validation failed; changes were not applied".into(),
            ));
        }

        let snapshot_json = serde_json::to_string(&before_config).unwrap_or_default();
        let _ = sqlx::query(
            "INSERT INTO config_snapshots (version, created_by, source, config_json) VALUES (?, ?, ?, ?)",
        )
        .bind(&before_config.version)
        .bind("operator")
        .bind("admin_config_save")
        .bind(snapshot_json)
        .execute(&state.db)
        .await;

        // Apply the new config to runtime
        let config = std::sync::Arc::new(config);
        state.runtime_config.store(config.clone());
        let _ = state.config_watch_tx.send(config.clone());

        // Re-initialize the provider registry with the effective config
        state.provider_registry.init_from_config(&state).await;

        // Update the route engine
        if let Ok(mut engine) = state.route_engine.write() {
            engine.update_config(config.clone());
        }

        // Persist runtime overrides to disk
        let _ = crate::config::overrides::save_runtime_overrides(config_path, &config);

        // Record audit entry
        let _ = sqlx::query(
            "INSERT INTO config_audit_log (actor, action, target_type) VALUES (?, ?, ?)",
        )
        .bind("operator")
        .bind("config_update")
        .bind("config")
        .execute(&state.db)
        .await;
    }

    Ok(Json(
        serde_json::json!({"status": "ok", "message": "Config saved and applied without restart"}),
    ))
}

pub async fn admin_config_rollback(
    State(state): State<AppState>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let snapshot_id = body
        .get("snapshot_id")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ApiError::InvalidRequest("snapshot_id is required".into()))?;

    let row =
        sqlx::query_as::<_, (String,)>("SELECT config_json FROM config_snapshots WHERE id = ?")
            .bind(snapshot_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| ApiError::InternalError(e.to_string()))?
            .ok_or_else(|| ApiError::InvalidRequest(format!("snapshot {snapshot_id} not found")))?;

    let config: crate::config::schema::Config = serde_json::from_str(&row.0)
        .map_err(|e| ApiError::InvalidRequest(format!("snapshot is invalid: {e}")))?;
    let validation = crate::config::validation::validate_config(&config);
    if !validation.errors.is_empty() {
        return Err(ApiError::InvalidRequest(
            "Snapshot config failed validation".into(),
        ));
    }

    let config = std::sync::Arc::new(config);
    state.runtime_config.store(config.clone());
    let _ = state.config_watch_tx.send(config.clone());
    state.provider_registry.init_from_config(&state).await;
    if let Ok(mut engine) = state.route_engine.write() {
        engine.update_config(config.clone());
    }
    let _ = crate::config::overrides::save_runtime_overrides(&state.boot_config_file, &config);
    let _ = sqlx::query(
        "INSERT INTO config_audit_log (actor, action, target_type, target_id) VALUES (?, ?, ?, ?)",
    )
    .bind("operator")
    .bind("config_rollback")
    .bind("config_snapshot")
    .bind(snapshot_id.to_string())
    .execute(&state.db)
    .await;

    Ok(Json(serde_json::json!({
        "status": "ok",
        "message": "Config rolled back and applied without restart",
        "snapshot_id": snapshot_id
    })))
}
pub async fn admin_model_groups_list(
    State(state): State<AppState>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    Ok(Json(
        crate::router::model_groups::get_all_model_groups(&state).await,
    ))
}

pub async fn admin_delete_model_group(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    sqlx::query("DELETE FROM model_groups WHERE name = ?")
        .bind(&name)
        .execute(&state.db)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string()))?;

    Ok(Json(serde_json::json!({
        "status": "ok",
        "message": format!("Model group '{}' deleted", name)
    })))
}

#[derive(serde::Deserialize)]
pub struct SaveModelGroupRequest {
    pub name: String,
    pub target: serde_json::Value, // Can be a string or array of strings
    pub enabled: bool,
}

pub async fn admin_save_model_group(
    State(state): State<AppState>,
    Json(payload): Json<SaveModelGroupRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let target_json = serde_json::to_string(&payload.target)
        .map_err(|e| ApiError::InvalidRequest(format!("Invalid target JSON: {e}")))?;

    sqlx::query(
        "INSERT INTO model_groups (name, target_json, enabled, updated_at) VALUES (?, ?, ?, datetime('now'))
         ON CONFLICT(name) DO UPDATE SET target_json = excluded.target_json, enabled = excluded.enabled, updated_at = excluded.updated_at"
    )
    .bind(&payload.name)
    .bind(target_json)
    .bind(payload.enabled)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::InternalError(e.to_string()))?;

    Ok(Json(serde_json::json!({
        "status": "ok",
        "message": format!("Model group '{}' saved", payload.name)
    })))
}

#[derive(serde::Deserialize)]
pub struct AnalyticsQuery {
    pub period: Option<String>,
}

pub async fn admin_analytics_traffic(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<AnalyticsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let period = query.period.unwrap_or_else(|| "24h".to_string());
    Ok(Json(
        crate::usage::aggregation::get_hourly_traffic(&state, &period).await,
    ))
}

pub async fn admin_analytics_distribution(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<AnalyticsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let period = query.period.unwrap_or_else(|| "24h".to_string());
    Ok(Json(
        crate::usage::aggregation::get_provider_distribution(&state, &period).await,
    ))
}

pub async fn admin_analytics_summary(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<AnalyticsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let period = query.period.unwrap_or_else(|| "24h".to_string());
    Ok(Json(
        crate::usage::aggregation::get_usage_series(&state, &period).await,
    ))
}

pub async fn admin_analytics_metrics(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<AnalyticsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let period = query.period.unwrap_or_else(|| "24h".to_string());
    Ok(Json(
        crate::usage::aggregation::get_period_summary(&state, &period).await,
    ))
}
