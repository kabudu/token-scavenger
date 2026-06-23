use crate::app::state::AppState;
use crate::router::selection::RouteAttempt;
use serde_json::json;

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;

pub fn bounded_limit(limit: Option<u32>) -> i64 {
    limit
        .map(i64::from)
        .unwrap_or(DEFAULT_LIMIT)
        .clamp(1, MAX_LIMIT)
}

fn interval_for_period(period: &str) -> &'static str {
    match period {
        "7d" => "-7 days",
        "30d" => "-30 days",
        "1y" => "-1 year",
        _ => "-24 hours",
    }
}

fn safe_details(value: serde_json::Value) -> String {
    let redacted = crate::util::redact::redact_json_value(value);
    let text = serde_json::to_string(&redacted).unwrap_or_else(|_| "{}".to_string());
    if text.len() > 8192 {
        let preview = text.chars().take(8192).collect::<String>();
        return serde_json::json!({
            "truncated": true,
            "preview": preview,
        })
        .to_string();
    }
    text
}

fn short_error(error: &str) -> String {
    let mut text = error.replace('\n', " ");
    if text.len() > 512 {
        text.truncate(512);
        text.push_str("...");
    }
    text
}

pub struct TraceEventRecord<'a> {
    pub request_id: &'a str,
    pub event_type: &'a str,
    pub provider_id: Option<&'a str>,
    pub model_id: Option<&'a str>,
    pub outcome: Option<&'a str>,
    pub latency_ms: Option<i64>,
    pub details: serde_json::Value,
}

pub async fn record_event(state: &AppState, event: TraceEventRecord<'_>) {
    let _ = sqlx::query(
        "INSERT INTO request_trace_events
         (request_id, event_type, provider_id, model_id, outcome, latency_ms, details_json)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(event.request_id)
    .bind(event.event_type)
    .bind(event.provider_id)
    .bind(event.model_id)
    .bind(event.outcome)
    .bind(event.latency_ms)
    .bind(safe_details(event.details))
    .execute(&state.db)
    .await;
}

pub async fn record_route_plan(
    state: &AppState,
    request_id: &str,
    endpoint_kind: &str,
    requested_model: &str,
    resolved_models: &[String],
    plan: &[RouteAttempt],
) {
    let candidates = plan
        .iter()
        .map(|attempt| {
            json!({
                "provider_id": attempt.provider_id,
                "model_id": attempt.model_id,
                "priority": attempt.priority,
            })
        })
        .collect::<Vec<_>>();

    record_event(
        state,
        TraceEventRecord {
            request_id,
            event_type: "route_plan",
            provider_id: None,
            model_id: None,
            outcome: Some(if plan.is_empty() { "empty" } else { "planned" }),
            latency_ms: None,
            details: json!({
            "endpoint_kind": endpoint_kind,
            "requested_model": requested_model,
            "resolved_models": resolved_models,
            "candidate_count": candidates.len(),
            "candidates": candidates,
            }),
        },
    )
    .await;
}

pub async fn record_attempt_started(
    state: &AppState,
    request_id: &str,
    endpoint_kind: &str,
    attempt: &RouteAttempt,
) {
    record_event(
        state,
        TraceEventRecord {
            request_id,
            event_type: "attempt_started",
            provider_id: Some(&attempt.provider_id),
            model_id: Some(&attempt.model_id),
            outcome: Some("started"),
            latency_ms: None,
            details: json!({
                "endpoint_kind": endpoint_kind,
                "priority": attempt.priority,
            }),
        },
    )
    .await;
}

pub async fn record_attempt_result(
    state: &AppState,
    request_id: &str,
    endpoint_kind: &str,
    attempt: &RouteAttempt,
    outcome: &str,
    latency_ms: Option<i64>,
    error_summary: Option<&str>,
) {
    record_event(
        state,
        TraceEventRecord {
            request_id,
            event_type: "attempt_result",
            provider_id: Some(&attempt.provider_id),
            model_id: Some(&attempt.model_id),
            outcome: Some(outcome),
            latency_ms,
            details: json!({
                "endpoint_kind": endpoint_kind,
                "priority": attempt.priority,
                "error_summary": error_summary.map(short_error),
            }),
        },
    )
    .await;
}

pub async fn record_skip(
    state: &AppState,
    request_id: &str,
    endpoint_kind: &str,
    attempt: &RouteAttempt,
    reason: &str,
) {
    record_event(
        state,
        TraceEventRecord {
            request_id,
            event_type: "attempt_skipped",
            provider_id: Some(&attempt.provider_id),
            model_id: Some(&attempt.model_id),
            outcome: Some("skipped"),
            latency_ms: None,
            details: json!({
                "endpoint_kind": endpoint_kind,
                "priority": attempt.priority,
                "reason": reason,
            }),
        },
    )
    .await;
}

pub async fn get_request_traces(state: &AppState, limit: i64) -> serde_json::Value {
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            String,
            Option<i64>,
            i64,
            i64,
            f64,
            Option<String>,
            Option<String>,
        ),
    >(
        "SELECT
            r.request_id,
            r.received_at,
            r.endpoint_kind,
            r.requested_model,
            r.selected_provider_id,
            r.status,
            r.http_status,
            COALESCE(r.latency_ms, 0),
            COALESCE(r.fallback_count, 0),
            COALESCE(SUM(u.estimated_cost_usd), 0.0),
            r.project_id,
            r.api_key_prefix
         FROM request_log r
         LEFT JOIN usage_events u ON u.request_id = r.request_id
         GROUP BY r.request_id
         ORDER BY r.received_at DESC
         LIMIT ?",
    )
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    json!({
        "traces": rows.into_iter().map(|row| json!({
            "request_id": row.0,
            "received_at": row.1,
            "endpoint_kind": row.2,
            "requested_model": row.3,
            "selected_provider_id": row.4,
            "status": row.5,
            "http_status": row.6,
            "latency_ms": row.7,
            "fallback_count": row.8,
            "estimated_cost_usd": row.9,
            "project_id": row.10,
            "api_key_prefix": row.11,
        })).collect::<Vec<_>>()
    })
}

pub async fn get_request_trace(state: &AppState, request_id: &str) -> Option<serde_json::Value> {
    let request = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
            Option<i64>,
            Option<i64>,
            bool,
            i64,
            i64,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    >(
        "SELECT request_id, received_at, endpoint_kind, requested_model, resolved_model_group,
                selected_provider_id, status, http_status, latency_ms, streaming, retry_count,
                fallback_count, error_code, error_summary, project_id, api_key_prefix
         FROM request_log
         WHERE request_id = ?",
    )
    .bind(request_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()?;

    let usage = sqlx::query_as::<_, (String, Option<String>, i64, i64, f64, String, bool, String)>(
        "SELECT provider_id, model_id, input_tokens, output_tokens, estimated_cost_usd,
                cost_confidence, free_tier, timestamp
         FROM usage_events
         WHERE request_id = ?
         ORDER BY id ASC",
    )
    .bind(request_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let events = sqlx::query_as::<
        _,
        (
            i64,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<i64>,
            String,
        ),
    >(
        "SELECT id, recorded_at, event_type, provider_id, model_id, outcome, latency_ms, details_json
         FROM request_trace_events
         WHERE request_id = ?
         ORDER BY id ASC",
    )
    .bind(request_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let event_values = events
        .into_iter()
        .map(|event| {
            let details =
                serde_json::from_str::<serde_json::Value>(&event.7).unwrap_or_else(|_| json!({}));
            json!({
                "id": event.0,
                "recorded_at": event.1,
                "event_type": event.2,
                "provider_id": event.3,
                "model_id": event.4,
                "outcome": event.5,
                "latency_ms": event.6,
                "details": details,
            })
        })
        .collect::<Vec<_>>();

    let usage_values = usage
        .into_iter()
        .map(|row| {
            json!({
                "provider_id": row.0,
                "model_id": row.1,
                "input_tokens": row.2,
                "output_tokens": row.3,
                "estimated_cost_usd": row.4,
                "cost_confidence": row.5,
                "free_tier": row.6,
                "timestamp": row.7,
            })
        })
        .collect::<Vec<_>>();

    Some(json!({
        "request": {
            "request_id": request.0,
            "received_at": request.1,
            "endpoint_kind": request.2,
            "requested_model": request.3,
            "resolved_model_group": request.4,
            "selected_provider_id": request.5,
            "status": request.6,
            "http_status": request.7,
            "latency_ms": request.8,
            "streaming": request.9,
            "retry_count": request.10,
            "fallback_count": request.11,
            "error_code": request.12,
            "error_summary": request.13,
            "project_id": request.14,
            "api_key_prefix": request.15,
        },
        "usage": usage_values,
        "events": event_values,
    }))
}

pub async fn get_observability_summary(state: &AppState, period: &str) -> serde_json::Value {
    let interval = interval_for_period(period);
    let requests = sqlx::query_as::<_, (i64, i64, i64, i64, i64, i64)>(&format!(
        "SELECT
                COUNT(*),
                COALESCE(SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN http_status = 429 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(fallback_count), 0),
                COALESCE(CAST(AVG(latency_ms) AS INTEGER), 0),
                COALESCE(SUM(CASE WHEN status != 'success' THEN 1 ELSE 0 END), 0)
             FROM request_log
             WHERE received_at > datetime('now', '{}')",
        interval
    ))
    .fetch_one(&state.db)
    .await
    .unwrap_or((0, 0, 0, 0, 0, 0));

    let usage = sqlx::query_as::<_, (i64, i64, f64)>(&format!(
        "SELECT
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(estimated_cost_usd), 0.0)
             FROM usage_events
             WHERE timestamp > datetime('now', '{}')",
        interval
    ))
    .fetch_one(&state.db)
    .await
    .unwrap_or((0, 0, 0.0));

    let saturation = sqlx::query_as::<_, (String, i64, i64, i64)>(&format!(
        "SELECT
                selected_provider_id,
                COUNT(*),
                COALESCE(SUM(CASE WHEN http_status = 429 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status != 'success' THEN 1 ELSE 0 END), 0)
             FROM request_log
             WHERE selected_provider_id IS NOT NULL
               AND received_at > datetime('now', '{}')
             GROUP BY selected_provider_id
             ORDER BY COUNT(*) DESC
             LIMIT 20",
        interval
    ))
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let total = requests.0.max(0) as f64;
    json!({
        "period": period,
        "request_count": requests.0,
        "success_count": requests.1,
        "failure_count": requests.5,
        "success_rate": if total > 0.0 { requests.1 as f64 / total } else { 0.0 },
        "rate_limit_count": requests.2,
        "rate_limit_rate": if total > 0.0 { requests.2 as f64 / total } else { 0.0 },
        "fallback_count": requests.3,
        "avg_latency_ms": requests.4,
        "input_tokens": usage.0,
        "output_tokens": usage.1,
        "total_tokens": usage.0 + usage.1,
        "estimated_cost_usd": usage.2,
        "provider_saturation": saturation.into_iter().map(|row| json!({
            "provider_id": row.0,
            "request_count": row.1,
            "rate_limit_count": row.2,
            "failure_count": row.3,
            "rate_limit_rate": if row.1 > 0 { row.2 as f64 / row.1 as f64 } else { 0.0 },
            "failure_rate": if row.1 > 0 { row.3 as f64 / row.1 as f64 } else { 0.0 },
        })).collect::<Vec<_>>()
    })
}

pub async fn get_incidents(state: &AppState, limit: i64) -> serde_json::Value {
    let health = sqlx::query_as::<_, (String, String, String, String, Option<String>)>(
        "SELECT recorded_at, provider_id, health_state, event_type, details_json
         FROM provider_health_events
         WHERE event_type LIKE '%failure%'
            OR event_type LIKE '%timeout%'
            OR health_state IN ('degraded', 'rate_limited', 'quota_exhausted', 'unhealthy')
            OR breaker_state IN ('open', 'half_open')
         ORDER BY recorded_at DESC
         LIMIT ?",
    )
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let audit = sqlx::query_as::<_, (String, String, Option<String>, Option<String>)>(
        "SELECT created_at, action, target_type, target_id
         FROM config_audit_log
         ORDER BY created_at DESC
         LIMIT ?",
    )
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let failures =
        sqlx::query_as::<_, (String, Option<String>, Option<String>, String, Option<i64>)>(
            "SELECT received_at, selected_provider_id, selected_model_id, status, http_status
         FROM request_log
         WHERE status != 'success'
         ORDER BY received_at DESC
         LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

    let mut incidents = Vec::new();
    incidents.extend(health.into_iter().map(|row| json!({
        "kind": "provider_health",
        "severity": if row.2 == "unhealthy" { "critical" } else { "warning" },
        "recorded_at": row.0,
        "provider_id": row.1,
        "title": format!("Provider {} {}", row.1, row.3),
        "details": crate::util::redact::redact_json_value(serde_json::from_str::<serde_json::Value>(row.4.as_deref().unwrap_or("{}")).unwrap_or_else(|_| json!({}))),
    })));
    incidents.extend(audit.into_iter().map(|row| {
        json!({
            "kind": "config_change",
            "severity": if row.1.contains("rejected") { "warning" } else { "info" },
            "recorded_at": row.0,
            "title": row.1,
            "target_type": row.2,
            "target_id": row.3,
        })
    }));
    incidents.extend(failures.into_iter().map(|row| {
        json!({
            "kind": "request_failure",
            "severity": if row.4 == Some(429) { "warning" } else { "critical" },
            "recorded_at": row.0,
            "provider_id": row.1,
            "model_id": row.2,
            "title": row.3,
            "http_status": row.4,
        })
    }));

    incidents.sort_by(|left, right| {
        right
            .get("recorded_at")
            .and_then(|value| value.as_str())
            .cmp(&left.get("recorded_at").and_then(|value| value.as_str()))
    });
    incidents.truncate(limit as usize);

    json!({ "incidents": incidents })
}

pub async fn get_diagnostic_bundle(state: &AppState) -> serde_json::Value {
    let config = serde_json::to_value(&*state.config()).unwrap_or_else(|_| json!({}));
    let health_states = state
        .health_states
        .iter()
        .map(|entry| {
            json!({
                "provider_id": entry.key(),
                "state": format!("{:?}", entry.value().state),
                "recent_successes": entry.value().recent_successes,
                "recent_failures": entry.value().recent_failures,
                "last_success_at": entry.value().last_success_at,
                "last_error_at": entry.value().last_error_at,
            })
        })
        .collect::<Vec<_>>();

    json!({
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_secs": state.start_time.elapsed().as_secs(),
        "config": crate::util::redact::redact_json_value(config),
        "observability_summary_24h": get_observability_summary(state, "24h").await,
        "recent_request_traces": get_request_traces(state, 25).await["traces"].clone(),
        "recent_incidents": get_incidents(state, 25).await["incidents"].clone(),
        "health_states": health_states,
    })
}
