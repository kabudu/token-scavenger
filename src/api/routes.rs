use crate::api::error::ApiError;
use crate::app::state::AppState;
use axum::response::sse::Event;
use axum::{
    Extension,
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
        "version": env!("CARGO_PKG_VERSION"),
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
    auth: Option<Extension<crate::api::auth::AuthContext>>,
    headers: HeaderMap,
    axum::Json(req): axum::Json<crate::api::openai::chat::ChatRequest>,
) -> Result<axum::response::Response, ApiError> {
    let normalized = crate::api::openai::chat::NormalizedChatRequest::from_request(req);
    let request_id = request_id_from_headers(&headers);
    register_project_for_request(&state, &request_id, auth.as_ref().map(|Extension(ctx)| ctx));

    if normalized.stream {
        // Streaming path
        let stream =
            crate::api::openai::stream::create_chat_stream(state, normalized, request_id).await?;
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
    auth: Option<Extension<crate::api::auth::AuthContext>>,
    headers: HeaderMap,
    axum::Json(req): axum::Json<crate::api::openai::embeddings::EmbeddingsRequest>,
) -> Result<Json<crate::api::openai::embeddings::EmbeddingsResponse>, ApiError> {
    let normalized = crate::api::openai::embeddings::NormalizedEmbeddingsRequest::from_request(req);
    let request_id = request_id_from_headers(&headers);
    register_project_for_request(&state, &request_id, auth.as_ref().map(|Extension(ctx)| ctx));
    let response =
        crate::router::engine::route_embeddings_request(state, normalized, request_id).await?;
    Ok(Json(response))
}

fn register_project_for_request(
    state: &AppState,
    request_id: &str,
    auth: Option<&crate::api::auth::AuthContext>,
) {
    let project = auth
        .and_then(|context| context.project.clone())
        .unwrap_or_else(crate::projects::ClientProjectContext::master_default);
    crate::projects::register_request_project(state, request_id, project);
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

/// GET /ui/login — browser login page for UI session auth.
pub async fn ui_login(State(state): State<AppState>) -> Result<Html<String>, ApiError> {
    let html = crate::ui::routes::render_login(&state).await;
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
        "projects" => crate::ui::routes::render_projects(&state).await,
        "health" => crate::ui::routes::render_health(&state).await,
        "observability" => crate::ui::routes::render_observability(&state).await,
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

pub async fn admin_whoami(
    auth: Option<Extension<crate::api::auth::AuthContext>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Some(Extension(context)) = auth else {
        return Ok(Json(serde_json::json!({
            "authenticated": true,
            "source": "disabled",
            "role": "admin"
        })));
    };

    Ok(Json(serde_json::json!({
        "authenticated": true,
        "source": match context.source {
            crate::api::auth::AuthSource::MasterKey => "master_key",
            crate::api::auth::AuthSource::UiSession => "ui_session",
            crate::api::auth::AuthSource::ExternalIdentity => "external_identity",
            crate::api::auth::AuthSource::ProjectKey => "project_key",
        },
        "subject": context.subject,
        "email": context.email,
        "display_name": context.display_name,
        "role": context.role.as_str(),
        "can_manage_credentials": context.role.can_manage_credentials(),
        "project": context.project.as_ref().map(|project| serde_json::json!({
            "project_id": project.project_id,
            "display_name": project.display_name,
            "api_key_prefix": project.api_key_prefix,
        }))
    })))
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
    let resolved_targets = crate::router::model_groups::resolve_model_group_targets(&state, &model)
        .await
        .unwrap_or_else(|| {
            vec![crate::router::model_groups::ModelTarget::any_provider(
                model.clone(),
            )]
        });
    let resolved_models = resolved_targets
        .iter()
        .map(|target| target.label())
        .collect::<Vec<_>>();
    let policy = crate::router::policy::RoutePolicy::from_config(&config);

    let mut raw_plan = Vec::new();
    for resolved in &resolved_targets {
        let plan = crate::router::selection::build_attempt_plan_for_target(
            &policy,
            &state.provider_registry,
            resolved,
            endpoint_kind,
        )
        .await;
        raw_plan.extend(plan);
    }
    crate::router::selection::assign_attempt_priorities(&mut raw_plan);

    let mut attempts = Vec::new();
    let token_estimate = crate::router::selection::TokenEstimate {
        input_tokens: params
            .get("input_tokens")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(256),
        output_tokens: params
            .get("output_tokens")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(1024),
    };
    let route_requirements = crate::discovery::model_intelligence::ModelRequestRequirements {
        requires_tools: params
            .get("tools")
            .and_then(|value| value.parse::<bool>().ok())
            .unwrap_or(false),
        requires_json_mode: params
            .get("json")
            .or_else(|| params.get("json_mode"))
            .and_then(|value| value.parse::<bool>().ok())
            .unwrap_or(false),
        requires_vision: params
            .get("vision")
            .and_then(|value| value.parse::<bool>().ok())
            .unwrap_or(false),
        required_context_tokens: Some(
            u64::from(token_estimate.input_tokens) + u64::from(token_estimate.output_tokens),
        ),
    };
    let project_policy = match params.get("project_id") {
        Some(project_id) => crate::projects::load_project_policy(&state.db, project_id).await?,
        None => None,
    };
    let explanations = crate::router::selection::explain_policy_plan(
        raw_plan,
        &state,
        &policy,
        &model,
        endpoint_kind,
        token_estimate,
    )
    .await;
    for explanation in explanations {
        let attempt = explanation.attempt;
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
        let model_state = sqlx::query_as::<_, (bool, bool, bool)>(
            "SELECT enabled, supports_chat, supports_embeddings FROM models WHERE provider_id = ? AND upstream_model_id = ?",
        )
        .bind(&attempt.provider_id)
        .bind(&attempt.model_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .unwrap_or((true, true, true));
        let model_enabled = model_state.0;
        let endpoint_supported = match endpoint_kind {
            crate::providers::traits::EndpointKind::ChatCompletions => model_state.1,
            crate::providers::traits::EndpointKind::Embeddings => model_state.2,
            crate::providers::traits::EndpointKind::ModelList => true,
        };
        let compatibility = crate::discovery::model_intelligence::model_compatibility(
            &state,
            &attempt,
            route_requirements,
        )
        .await;
        let free_only = config
            .providers
            .iter()
            .find(|provider| provider.id == attempt.provider_id)
            .map(|provider| provider.free_only)
            .unwrap_or(true);
        let paid_allowed = free_only || config.routing.allow_paid_fallback;
        let base_included = model_enabled
            && endpoint_supported
            && compatibility.compatible
            && paid_allowed
            && health != "Unhealthy"
            && breaker != "Open"
            && breaker != "HalfOpen";
        let mut project_reasons = Vec::new();
        if let Some(project_policy) = project_policy.as_ref() {
            project_reasons = crate::projects::project_policy_skip_reasons(
                &state,
                project_policy,
                None,
                &model,
                &attempt,
                token_estimate,
            )
            .await?;
        }
        let included = base_included && explanation.included && project_reasons.is_empty();
        let base_reason = if base_included {
            None
        } else if !paid_allowed {
            Some("filtered by paid fallback policy")
        } else if !endpoint_supported {
            Some("filtered by model endpoint capability")
        } else if !compatibility.compatible {
            Some("filtered by model intelligence compatibility")
        } else {
            Some("filtered by health, breaker, or model enablement")
        };
        let mut reasons = explanation.reasons;
        reasons.extend(compatibility.reasons);
        if !project_reasons.is_empty() {
            reasons.retain(|reason| reason != "eligible");
            reasons.extend(project_reasons);
        }
        if let Some(base_reason) = base_reason {
            if base_reason == "filtered by model intelligence compatibility" {
                reasons.retain(|reason| reason != "eligible");
            }
            reasons.insert(0, base_reason.to_string());
        }
        let reason = reasons.join("; ");
        attempts.push(serde_json::json!({
            "provider_id": attempt.provider_id,
            "model_id": attempt.model_id,
            "priority": attempt.priority,
            "objective": format!("{:?}", explanation.objective),
            "score": explanation.score.total,
            "score_components": {
                "cost": explanation.score.cost_score,
                "latency": explanation.score.latency_score,
                "reliability": explanation.score.reliability_score,
                "quality": explanation.score.quality_score,
                "context": explanation.score.context_score,
                "operator": explanation.score.operator_score
            },
            "estimated_cost_usd": explanation.score.estimated_cost_usd,
            "cost_confidence": explanation.score.cost_confidence,
            "observed_latency_ms": explanation.score.observed_latency_ms,
            "recent_failure_rate": explanation.score.recent_failure_rate,
            "health": health,
            "breaker_state": breaker,
            "model_enabled": model_enabled,
            "model_intelligence_compatible": compatibility.compatible,
            "free_only": free_only,
            "included": included,
            "reason": reason,
            "reasons": reasons
        }));
    }

    Ok(Json(serde_json::json!({
        "requested_model": model,
        "resolved_model": resolved_models.join(", "),
        "endpoint": endpoint,
        "free_first": config.routing.free_first,
        "allow_paid_fallback": config.routing.allow_paid_fallback,
        "objective": format!("{:?}", policy.objective_for_model_group(&model)),
        "project_id": project_policy.as_ref().map(|policy| policy.project_id.clone()),
        "token_estimate": {
            "input_tokens": token_estimate.input_tokens,
            "output_tokens": token_estimate.output_tokens
        },
        "requirements": {
            "tools": route_requirements.requires_tools,
            "json_mode": route_requirements.requires_json_mode,
            "vision": route_requirements.requires_vision,
            "context_tokens": route_requirements.required_context_tokens
        },
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
    auth: Option<Extension<crate::api::auth::AuthContext>>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let auth_context = auth.map(|Extension(context)| context);
    if config_update_changes_credentials(&body)
        && !auth_context
            .as_ref()
            .map(|context| context.role.can_manage_credentials())
            .unwrap_or(true)
    {
        return Err(ApiError::Forbidden);
    }

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
        if let Some(ui_session_auth) = server.get("ui_session_auth").and_then(|v| v.as_bool()) {
            config.server.ui_session_auth = ui_session_auth;
            changed = true;
        }
        if let Some(external_identity) = server.get("external_identity") {
            config.server.external_identity = serde_json::from_value(external_identity.clone())
                .map_err(|error| {
                    ApiError::InvalidRequest(format!("Invalid server.external_identity: {error}"))
                })?;
            changed = true;
        }
        if let Some(allow_query_api_keys) =
            server.get("allow_query_api_keys").and_then(|v| v.as_bool())
        {
            config.server.allow_query_api_keys = allow_query_api_keys;
            changed = true;
        }
        if let Some(ui_path) = server.get("ui_path").and_then(|v| v.as_str()) {
            config.server.ui_path = ui_path.to_string();
            changed = true;
        }
        if let Some(request_timeout_ms) = server.get("request_timeout_ms").and_then(|v| v.as_u64())
        {
            config.server.request_timeout_ms = request_timeout_ms;
            changed = true;
        }
    }

    // --- Security, retention, and update settings ---
    if let Some(security) = body.get("security") {
        config.security = serde_json::from_value(security.clone()).map_err(|error| {
            ApiError::InvalidRequest(format!("Invalid security config: {error}"))
        })?;
        changed = true;
    }

    if let Some(retention) = body.get("retention") {
        config.retention = serde_json::from_value(retention.clone()).map_err(|error| {
            ApiError::InvalidRequest(format!("Invalid retention config: {error}"))
        })?;
        changed = true;
    }

    if let Some(updates) = body.get("updates") {
        config.updates = serde_json::from_value(updates.clone()).map_err(|error| {
            ApiError::InvalidRequest(format!("Invalid updates config: {error}"))
        })?;
        changed = true;
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
        if let Some(objective) = routing.get("objective") {
            config.routing.objective =
                serde_json::from_value(objective.clone()).map_err(|error| {
                    ApiError::InvalidRequest(format!("Invalid routing.objective: {error}"))
                })?;
            changed = true;
        }
        if let Some(overrides) = routing.get("model_group_objectives") {
            config.routing.model_group_objectives = serde_json::from_value(overrides.clone())
                .map_err(|error| {
                    ApiError::InvalidRequest(format!(
                        "Invalid routing.model_group_objectives: {error}"
                    ))
                })?;
            changed = true;
        }
        if let Some(budgets) = routing.get("budgets") {
            config.routing.budgets = serde_json::from_value(budgets.clone()).map_err(|error| {
                ApiError::InvalidRequest(format!("Invalid routing.budgets: {error}"))
            })?;
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
    let mut providers_changed = false;
    if let Some(providers) = body.get("providers").and_then(|p| p.as_array()) {
        for provider_update in providers {
            if let Some(id) = provider_update.get("id").and_then(|v| v.as_str()) {
                let enabled = provider_update.get("enabled");
                let api_key = provider_update
                    .get("api_key")
                    .and_then(|v| v.as_str())
                    .and_then(|s| {
                        if s.is_empty() || crate::util::redact::is_redacted_secret(s) {
                            None
                        } else {
                            Some(s)
                        }
                    });
                let base_url = provider_update
                    .get("base_url")
                    .and_then(|v| v.as_str())
                    .and_then(|s| if s.is_empty() { None } else { Some(s) });
                let free_only = provider_update.get("free_only").and_then(|v| v.as_bool());
                let embedding_support = provider_update
                    .get("embedding_support")
                    .and_then(|v| v.as_str())
                    .and_then(|value| match value {
                        "auto" => Some(crate::config::schema::ProviderEmbeddingSupport::Auto),
                        "enabled" => Some(crate::config::schema::ProviderEmbeddingSupport::Enabled),
                        "disabled" => {
                            Some(crate::config::schema::ProviderEmbeddingSupport::Disabled)
                        }
                        _ => None,
                    });
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
                    providers_changed = true;
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
                    if let Some(support) = embedding_support {
                        provider.embedding_support = support;
                    }
                    // Persist provider state to DB
                    let display_name = &provider.id;
                    let _ = sqlx::query(
                        "INSERT INTO providers (provider_id, display_name, enabled, base_url, free_only)
                         VALUES (?, ?, ?, ?, ?)
                         ON CONFLICT(provider_id) DO UPDATE SET
                             display_name = excluded.display_name,
                             enabled = excluded.enabled,
                             base_url = excluded.base_url,
                             free_only = excluded.free_only"
                    )
                    .bind(id)
                    .bind(display_name)
                    .bind(provider.enabled)
                    .bind(provider.base_url.as_deref())
                    .bind(provider.free_only)
                    .execute(&state.db)
                    .await;
                    changed = true;
                    providers_changed = true;
                } else if let Some(e) = enabled.and_then(|v| v.as_bool()).or(Some(true)) {
                    // New provider (add)
                    let new_provider = crate::config::schema::ProviderConfig {
                        id: id.to_string(),
                        enabled: e,
                        base_url: base_url.map(String::from),
                        api_key: api_key.map(String::from),
                        free_only: free_only.unwrap_or(true),
                        discover_models: true,
                        embedding_support: embedding_support.unwrap_or_default(),
                    };
                    config.providers.push(new_provider);
                    let _ = sqlx::query(
                        "INSERT INTO providers (provider_id, display_name, enabled, base_url, free_only)
                         VALUES (?, ?, ?, ?, ?)
                         ON CONFLICT(provider_id) DO UPDATE SET
                             display_name = excluded.display_name,
                             enabled = excluded.enabled,
                             base_url = excluded.base_url,
                             free_only = excluded.free_only"
                    )
                    .bind(id)
                    .bind(id)
                    .bind(e)
                    .bind(base_url)
                    .bind(free_only.unwrap_or(true))
                    .execute(&state.db)
                    .await;
                    changed = true;
                    providers_changed = true;
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
                let supports_tools = model_update.get("supports_tools").and_then(|v| v.as_bool());
                let supports_json_mode = model_update
                    .get("supports_json_mode")
                    .and_then(|v| v.as_bool());
                let supports_vision = model_update
                    .get("supports_vision")
                    .and_then(|v| v.as_bool());
                let metadata_json = model_update.get("metadata").or_else(|| {
                    model_update
                        .get("metadata_json")
                        .filter(|value| value.is_object())
                });

                if enabled.is_some()
                    || priority.is_some()
                    || supports_tools.is_some()
                    || supports_json_mode.is_some()
                    || supports_vision.is_some()
                    || metadata_json.is_some()
                {
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
                    if supports_tools.is_some() {
                        query.push_str(", supports_tools");
                        values.push_str(", ?");
                        update.push_str(", supports_tools = excluded.supports_tools");
                    }
                    if supports_json_mode.is_some() {
                        query.push_str(", supports_json_mode");
                        values.push_str(", ?");
                        update.push_str(", supports_json_mode = excluded.supports_json_mode");
                    }
                    if supports_vision.is_some() {
                        query.push_str(", supports_vision");
                        values.push_str(", ?");
                        update.push_str(", supports_vision = excluded.supports_vision");
                    }
                    if metadata_json.is_some() {
                        query.push_str(", metadata_json");
                        values.push_str(", ?");
                        update.push_str(", metadata_json = excluded.metadata_json");
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
                    if let Some(supports_tools) = supports_tools {
                        sql = sql.bind(supports_tools);
                    }
                    if let Some(supports_json_mode) = supports_json_mode {
                        sql = sql.bind(supports_json_mode);
                    }
                    if let Some(supports_vision) = supports_vision {
                        sql = sql.bind(supports_vision);
                    }
                    if let Some(metadata_json) = metadata_json {
                        sql = sql.bind(metadata_json.to_string());
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
            .bind(audit_actor(auth_context.as_ref()))
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
        .bind(audit_actor(auth_context.as_ref()))
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

        if providers_changed {
            crate::discovery::refresh::refresh_all(&state).await;
        }

        // Persist runtime overrides to disk
        crate::config::overrides::save_runtime_overrides(config_path, &config)
            .map_err(|error| ApiError::InternalError(error.to_string()))?;

        // Record audit entry
        let _ = sqlx::query(
            "INSERT INTO config_audit_log (actor, action, target_type) VALUES (?, ?, ?)",
        )
        .bind(audit_actor(auth_context.as_ref()))
        .bind("config_update")
        .bind("config")
        .execute(&state.db)
        .await;
    }

    Ok(Json(
        serde_json::json!({"status": "ok", "message": "Config saved and applied without restart"}),
    ))
}

fn audit_actor(context: Option<&crate::api::auth::AuthContext>) -> String {
    context
        .map(crate::api::auth::AuthContext::audit_actor)
        .unwrap_or_else(|| "operator".to_string())
}

fn config_update_changes_credentials(body: &serde_json::Value) -> bool {
    let server_key_changes = body
        .get("server")
        .and_then(|server| server.get("master_api_key"))
        .and_then(|value| value.as_str())
        .map(|value| !value.is_empty() && !crate::util::redact::is_redacted_secret(value))
        .unwrap_or(false);
    if server_key_changes {
        return true;
    }

    body.get("providers")
        .and_then(|providers| providers.as_array())
        .map(|providers| {
            providers.iter().any(|provider| {
                provider
                    .get("api_key")
                    .and_then(|value| value.as_str())
                    .map(|value| {
                        !value.is_empty() && !crate::util::redact::is_redacted_secret(value)
                    })
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
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
    crate::config::overrides::save_runtime_overrides(&state.boot_config_file, &config)
        .map_err(|error| ApiError::InternalError(error.to_string()))?;
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

pub async fn admin_projects(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(crate::projects::list_projects(&state).await?))
}

pub async fn admin_project_create(
    State(state): State<AppState>,
    auth: Option<Extension<crate::api::auth::AuthContext>>,
    axum::Json(body): axum::Json<crate::projects::ProjectUpsert>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let auth_context = auth.as_ref().map(|Extension(context)| context);
    if auth_context.is_some_and(|context| context.role < crate::api::auth::AdminRole::ConfigEditor)
    {
        return Err(ApiError::Forbidden);
    }
    let actor = audit_actor(auth.as_ref().map(|Extension(context)| context));
    Ok(Json(
        crate::projects::create_project(&state, body, &actor).await?,
    ))
}

pub async fn admin_project_update(
    State(state): State<AppState>,
    auth: Option<Extension<crate::api::auth::AuthContext>>,
    axum::extract::Path(project_id): axum::extract::Path<String>,
    axum::Json(body): axum::Json<crate::projects::ProjectPatch>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let auth_context = auth.as_ref().map(|Extension(context)| context);
    if !crate::projects::can_manage_project(&state, auth_context, &project_id).await? {
        return Err(ApiError::Forbidden);
    }
    let actor = audit_actor(auth_context);
    Ok(Json(
        crate::projects::update_project(&state, &project_id, body, &actor).await?,
    ))
}

pub async fn admin_project_delete(
    State(state): State<AppState>,
    auth: Option<Extension<crate::api::auth::AuthContext>>,
    axum::extract::Path(project_id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let auth_context = auth.as_ref().map(|Extension(context)| context);
    if !crate::projects::can_manage_project(&state, auth_context, &project_id).await? {
        return Err(ApiError::Forbidden);
    }
    let actor = audit_actor(auth_context);
    Ok(Json(
        crate::projects::delete_project(&state, &project_id, &actor).await?,
    ))
}

pub async fn admin_project_issue_key(
    State(state): State<AppState>,
    auth: Option<Extension<crate::api::auth::AuthContext>>,
    axum::extract::Path(project_id): axum::extract::Path<String>,
    axum::Json(body): axum::Json<crate::projects::IssueKeyRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let auth_context = auth.as_ref().map(|Extension(context)| context);
    if !crate::projects::can_manage_project(&state, auth_context, &project_id).await? {
        return Err(ApiError::Forbidden);
    }
    let actor = audit_actor(auth_context);
    let issued = crate::projects::issue_project_key(&state, &project_id, body, &actor).await?;
    Ok(Json(serde_json::json!({
        "project_id": issued.project_id,
        "key_prefix": issued.key_prefix,
        "api_key": issued.api_key,
        "message": "Store this API key now; TokenScavenger will not show it again."
    })))
}

pub async fn admin_project_revoke_key(
    State(state): State<AppState>,
    auth: Option<Extension<crate::api::auth::AuthContext>>,
    axum::extract::Path((project_id, key_prefix)): axum::extract::Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let auth_context = auth.as_ref().map(|Extension(context)| context);
    if !crate::projects::can_manage_project(&state, auth_context, &project_id).await? {
        return Err(ApiError::Forbidden);
    }
    let actor = audit_actor(auth_context);
    Ok(Json(
        crate::projects::revoke_project_key(&state, &project_id, &key_prefix, &actor).await?,
    ))
}

pub async fn admin_project_usage(
    State(state): State<AppState>,
    axum::extract::Path(project_id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        crate::projects::project_usage(&state, &project_id).await?,
    ))
}

pub async fn admin_project_export(
    State(state): State<AppState>,
    axum::extract::Path(project_id): axum::extract::Path<String>,
) -> Result<axum::response::Response, ApiError> {
    let csv = crate::projects::project_export_csv(&state, &project_id).await?;
    Ok(([(header::CONTENT_TYPE, "text/csv; charset=utf-8")], csv).into_response())
}

pub async fn admin_project_diagnostic_bundle(
    State(state): State<AppState>,
    axum::extract::Path(project_id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let bundle = crate::projects::project_diagnostic_bundle(&state, &project_id).await?;
    Ok(Json(serde_json::json!({
        "bundle": bundle,
        "encoded": crate::projects::encode_diagnostic_bundle(&bundle)
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
    pub target: serde_json::Value,
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
    pub limit: Option<u32>,
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

pub async fn admin_observability_summary(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<AnalyticsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let period = query.period.unwrap_or_else(|| "24h".to_string());
    Ok(Json(
        crate::observability::get_observability_summary(&state, &period).await,
    ))
}

pub async fn admin_request_traces(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<AnalyticsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        crate::observability::get_request_traces(
            &state,
            crate::observability::bounded_limit(query.limit),
        )
        .await,
    ))
}

pub async fn admin_request_trace(
    State(state): State<AppState>,
    axum::extract::Path(request_id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    crate::observability::get_request_trace(&state, &request_id)
        .await
        .map(Json)
        .ok_or_else(|| ApiError::InvalidRequest(format!("request trace '{request_id}' not found")))
}

pub async fn admin_incidents(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<AnalyticsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        crate::observability::get_incidents(
            &state,
            crate::observability::bounded_limit(query.limit),
        )
        .await,
    ))
}

pub async fn admin_diagnostic_bundle(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        crate::observability::get_diagnostic_bundle(&state).await,
    ))
}

#[derive(serde::Deserialize)]
pub struct BackfillPricingQuery {
    pub dry_run: Option<bool>,
}

pub async fn admin_pricing(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        crate::usage::pricing_catalog::get_pricing_state(&state.db).await,
    ))
}

pub async fn admin_pricing_refresh(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let result =
        crate::usage::pricing_catalog::refresh_pricing_sources(&state.db, &state.http_client, true)
            .await
            .map_err(|e| ApiError::InternalError(e.to_string()))?;
    Ok(Json(result))
}

pub async fn admin_pricing_backfill(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<BackfillPricingQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let dry_run = query.dry_run.unwrap_or(true);
    let result = crate::usage::pricing_catalog::backfill_zero_cost_paid_usage(&state.db, dry_run)
        .await
        .map_err(|e| ApiError::InternalError(e.to_string()))?;
    Ok(Json(result))
}

pub async fn admin_update_check(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let status = crate::update::check_for_update(&state.config().updates)
        .await
        .map_err(|error| ApiError::InternalError(error.to_string()))?;
    Ok(Json(serde_json::to_value(status).unwrap_or_default()))
}

pub async fn admin_update_apply(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let status = crate::update::apply_update(state)
        .await
        .map_err(|error| ApiError::InternalError(error.to_string()))?;
    Ok(Json(serde_json::json!({
        "status": "restart_scheduled",
        "update": status
    })))
}
