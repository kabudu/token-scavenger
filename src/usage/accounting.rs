use crate::api::openai::chat::UsageResponse;
use crate::app::state::AppState;
use crate::usage::pricing_catalog::{
    PricingUsage, calculate_cost, free_tier_estimate, lookup_rate, unknown_price_estimate,
};
use tracing::{info, warn};

/// Inputs needed to persist a completed request and usage event.
pub struct UsageRecord<'a> {
    pub provider_id: &'a str,
    pub model_id: &'a str,
    pub requested_model: &'a str,
    pub usage: Option<&'a UsageResponse>,
    pub latency_ms: i64,
    pub free_tier: bool,
    pub request_id: &'a str,
    pub endpoint_kind: &'a str,
    pub streaming: bool,
}

/// Inputs needed to persist a failed request when no usage event exists.
pub struct FailureRecord<'a> {
    pub request_id: &'a str,
    pub endpoint_kind: &'a str,
    pub requested_model: &'a str,
    pub selected_provider_id: Option<&'a str>,
    pub selected_model_id: Option<&'a str>,
    pub status: &'a str,
    pub http_status: i64,
    pub latency_ms: i64,
    pub streaming: bool,
    pub error_code: Option<&'a str>,
    pub error_summary: Option<&'a str>,
}

/// Record a usage event for a completed request.
pub async fn record_usage(state: &AppState, record: UsageRecord<'_>) -> Result<(), sqlx::Error> {
    let project = crate::projects::remove_request_project(state, record.request_id)
        .unwrap_or_else(crate::projects::ClientProjectContext::master_default);
    let usage = record.usage.unwrap_or(&UsageResponse {
        prompt_tokens: 0,
        completion_tokens: 0,
        total_tokens: 0,
        prompt_cache_hit_tokens: None,
        prompt_cache_miss_tokens: None,
        reasoning_tokens: None,
    });

    let pricing_usage = PricingUsage {
        input_tokens: usage.prompt_tokens,
        cached_input_tokens: usage.prompt_cache_hit_tokens,
        cache_miss_input_tokens: usage.prompt_cache_miss_tokens,
        output_tokens: usage.completion_tokens,
        reasoning_tokens: usage.reasoning_tokens,
    };

    let cost = if record.free_tier {
        free_tier_estimate()
    } else {
        match lookup_rate(&state.db, record.provider_id, record.model_id).await? {
            Some(rate) => calculate_cost(&rate, &pricing_usage),
            None => {
                warn!(
                    provider = %record.provider_id,
                    model = %record.model_id,
                    "Paid usage recorded without known model pricing"
                );
                crate::metrics::prometheus::record_unknown_price(
                    record.provider_id,
                    record.model_id,
                );
                unknown_price_estimate(record.provider_id, record.model_id, &pricing_usage)
            }
        }
    };

    sqlx::query(
        "INSERT INTO request_log (request_id, endpoint_kind, requested_model, selected_provider_id, selected_model_id, status, http_status, latency_ms, streaming, project_id, api_key_prefix)
         VALUES (?, ?, ?, ?, ?, 'success', 200, ?, ?, ?, ?)
         ON CONFLICT(request_id) DO UPDATE SET
            endpoint_kind = excluded.endpoint_kind,
            requested_model = excluded.requested_model,
            selected_provider_id = excluded.selected_provider_id,
            selected_model_id = excluded.selected_model_id,
            status = 'success',
            http_status = 200,
            latency_ms = excluded.latency_ms,
            streaming = excluded.streaming,
            project_id = excluded.project_id,
            api_key_prefix = excluded.api_key_prefix"
    )
    .bind(record.request_id)
    .bind(record.endpoint_kind)
    .bind(record.requested_model)
    .bind(record.provider_id)
    .bind(record.model_id)
    .bind(record.latency_ms)
    .bind(record.streaming)
    .bind(&project.project_id)
    .bind(&project.api_key_prefix)
    .execute(&state.db)
    .await?;

    sqlx::query(
        "INSERT INTO usage_events
         (request_id, provider_id, model_id, input_tokens, output_tokens, estimated_cost_usd, cost_confidence, free_tier, cached_input_tokens, cache_miss_input_tokens, reasoning_tokens, pricing_model_id, cost_formula_json, cost_calculated_at, project_id, api_key_prefix)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), ?, ?)",
    )
    .bind(record.request_id)
    .bind(record.provider_id)
    .bind(record.model_id)
    .bind(usage.prompt_tokens as i64)
    .bind(usage.completion_tokens as i64)
    .bind(cost.amount_usd)
    .bind(&cost.confidence)
    .bind(record.free_tier)
    .bind(usage.prompt_cache_hit_tokens.map(|v| v as i64))
    .bind(usage.prompt_cache_miss_tokens.map(|v| v as i64))
    .bind(usage.reasoning_tokens.map(|v| v as i64))
    .bind(cost.pricing_model_id)
    .bind(cost.formula_json.to_string())
    .bind(&project.project_id)
    .bind(&project.api_key_prefix)
    .execute(&state.db)
    .await?;

    // Emit metrics
    crate::metrics::prometheus::record_request(
        record.provider_id,
        record.model_id,
        record.endpoint_kind,
        "success",
    );
    crate::metrics::prometheus::record_tokens(
        record.provider_id,
        record.model_id,
        "input",
        usage.prompt_tokens,
    );
    crate::metrics::prometheus::record_tokens(
        record.provider_id,
        record.model_id,
        "output",
        usage.completion_tokens,
    );
    crate::metrics::prometheus::record_estimated_cost(
        record.provider_id,
        record.model_id,
        &cost.confidence,
        cost.amount_usd,
    );
    crate::metrics::prometheus::record_project_usage(
        &project.project_id,
        record.endpoint_kind,
        "success",
        usage.prompt_tokens,
        usage.completion_tokens,
        cost.amount_usd,
    );

    info!(
        request_id = %record.request_id,
        project_id = %project.project_id,
        api_key_prefix = %project.api_key_prefix,
        provider = %record.provider_id,
        model = %record.model_id,
        prompt_tokens = usage.prompt_tokens,
        completion_tokens = usage.completion_tokens,
        estimated_cost_usd = cost.amount_usd,
        cost_confidence = %cost.confidence,
        latency_ms = record.latency_ms,
        "Usage recorded"
    );

    Ok(())
}

/// Record a failed request row so exhausted routes remain auditable.
pub async fn record_failure(
    state: &AppState,
    record: FailureRecord<'_>,
) -> Result<(), sqlx::Error> {
    let project = crate::projects::remove_request_project(state, record.request_id)
        .unwrap_or_else(crate::projects::ClientProjectContext::master_default);
    let error_summary = record.error_summary.map(|summary| {
        let config = state.config();
        let redacted = crate::util::redact::redact_config_secrets(&config, summary);
        crate::observability::short_error(&redacted)
    });
    sqlx::query(
        "INSERT INTO request_log (request_id, endpoint_kind, requested_model, selected_provider_id, selected_model_id, status, http_status, latency_ms, streaming, error_code, error_summary, project_id, api_key_prefix)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(request_id) DO UPDATE SET
            status = excluded.status,
            http_status = excluded.http_status,
            latency_ms = excluded.latency_ms,
            error_code = excluded.error_code,
            error_summary = excluded.error_summary,
            selected_provider_id = COALESCE(excluded.selected_provider_id, request_log.selected_provider_id),
            selected_model_id = COALESCE(excluded.selected_model_id, request_log.selected_model_id)
         WHERE request_log.status = 'pending'"
    )
    .bind(record.request_id)
    .bind(record.endpoint_kind)
    .bind(record.requested_model)
    .bind(record.selected_provider_id)
    .bind(record.selected_model_id)
    .bind(record.status)
    .bind(record.http_status)
    .bind(record.latency_ms)
    .bind(record.streaming)
    .bind(record.error_code)
    .bind(error_summary)
    .bind(&project.project_id)
    .bind(&project.api_key_prefix)
    .execute(&state.db)
    .await?;

    crate::metrics::prometheus::record_request(
        record.selected_provider_id.unwrap_or("none"),
        record.selected_model_id.unwrap_or(record.requested_model),
        record.endpoint_kind,
        record.status,
    );
    crate::metrics::prometheus::record_project_usage(
        &project.project_id,
        record.endpoint_kind,
        record.status,
        0,
        0,
        0.0,
    );

    info!(
        request_id = %record.request_id,
        project_id = %project.project_id,
        provider = record.selected_provider_id.unwrap_or("none"),
        model = record.requested_model,
        status = record.status,
        error_code = record.error_code.unwrap_or("none"),
        latency_ms = record.latency_ms,
        "Failed request recorded"
    );

    Ok(())
}

pub async fn ensure_pending_request(
    state: &AppState,
    request_id: &str,
    requested_model: &str,
    streaming: bool,
) -> Result<(), sqlx::Error> {
    let project = crate::projects::project_for_request(state, request_id)
        .unwrap_or_else(crate::projects::ClientProjectContext::master_default);
    sqlx::query(
        "INSERT OR IGNORE INTO request_log
         (request_id, endpoint_kind, requested_model, status, http_status, latency_ms, streaming, project_id, api_key_prefix)
         VALUES (?, 'chat', ?, 'pending', 0, 0, ?, ?, ?)",
    )
    .bind(request_id)
    .bind(requested_model)
    .bind(streaming)
    .bind(project.project_id)
    .bind(project.api_key_prefix)
    .execute(&state.db)
    .await?;
    Ok(())
}

pub struct DecisionStamp<'a> {
    pub session_digest: Option<&'a str>,
    pub subtask_digest: Option<&'a str>,
    pub task_phase: Option<&'a str>,
    pub tier: Option<&'a str>,
    pub selection_source: Option<&'a str>,
    pub profile_revision: Option<&'a str>,
    pub classifier_status: Option<&'a str>,
}

pub async fn annotate_request_decision(
    state: &AppState,
    request_id: &str,
    stamp: DecisionStamp<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE request_log SET
            session_digest = ?,
            subtask_digest = ?,
            task_phase = ?,
            tier = ?,
            selection_source = ?,
            profile_revision = ?,
            classifier_status = ?
         WHERE request_id = ?",
    )
    .bind(stamp.session_digest)
    .bind(stamp.subtask_digest)
    .bind(stamp.task_phase)
    .bind(stamp.tier)
    .bind(stamp.selection_source)
    .bind(stamp.profile_revision)
    .bind(stamp.classifier_status)
    .bind(request_id)
    .execute(&state.db)
    .await?;
    Ok(())
}

pub struct InternalUsage<'a> {
    pub parent_request_id: &'a str,
    pub provider_id: &'a str,
    pub model_id: &'a str,
    pub purpose: &'a str,
    pub usage: Option<&'a UsageResponse>,
    pub unknown_usage: bool,
    pub latency_ms: i64,
}

/// Persist classifier or other internal usage against the parent request.
/// Does not remove the parent project context and does not insert a second
/// external request row.
pub async fn record_internal_usage(
    state: &AppState,
    record: InternalUsage<'_>,
) -> Result<(), sqlx::Error> {
    let project = crate::projects::project_for_request(state, record.parent_request_id)
        .unwrap_or_else(crate::projects::ClientProjectContext::master_default);
    sqlx::query(
        "INSERT OR IGNORE INTO request_log
         (request_id, endpoint_kind, requested_model, status, http_status, latency_ms, streaming, project_id, api_key_prefix)
         VALUES (?, 'chat', '', 'pending', 0, 0, 0, ?, ?)",
    )
    .bind(record.parent_request_id)
    .bind(&project.project_id)
    .bind(&project.api_key_prefix)
    .execute(&state.db)
    .await?;

    let usage = record.usage.cloned().unwrap_or(UsageResponse {
        prompt_tokens: 0,
        completion_tokens: 0,
        total_tokens: 0,
        prompt_cache_hit_tokens: None,
        prompt_cache_miss_tokens: None,
        reasoning_tokens: None,
    });
    let pricing_usage = crate::usage::pricing_catalog::PricingUsage {
        input_tokens: usage.prompt_tokens,
        cached_input_tokens: usage.prompt_cache_hit_tokens,
        cache_miss_input_tokens: usage.prompt_cache_miss_tokens,
        output_tokens: usage.completion_tokens,
        reasoning_tokens: usage.reasoning_tokens,
    };
    let free_tier = state
        .config()
        .providers
        .iter()
        .find(|provider| provider.id == record.provider_id)
        .map(|provider| provider.free_only)
        .unwrap_or(true);
    let cost = if record.unknown_usage {
        let mut estimate = crate::usage::pricing_catalog::unknown_price_estimate(
            record.provider_id,
            record.model_id,
            &pricing_usage,
        );
        estimate.confidence = "unknown".to_string();
        estimate
    } else if free_tier {
        crate::usage::pricing_catalog::free_tier_estimate()
    } else {
        match crate::usage::pricing_catalog::lookup_rate(
            &state.db,
            record.provider_id,
            record.model_id,
        )
        .await?
        {
            Some(rate) => crate::usage::pricing_catalog::calculate_cost(&rate, &pricing_usage),
            None => {
                let mut estimate = crate::usage::pricing_catalog::unknown_price_estimate(
                    record.provider_id,
                    record.model_id,
                    &pricing_usage,
                );
                estimate.confidence = "unknown".to_string();
                estimate
            }
        }
    };
    let attempt_id = format!(
        "{}:{}:{}",
        record.parent_request_id,
        record.purpose,
        uuid::Uuid::new_v4()
    );
    sqlx::query(
        "INSERT INTO usage_events
         (request_id, provider_id, model_id, input_tokens, output_tokens, estimated_cost_usd, cost_confidence, free_tier, cached_input_tokens, cache_miss_input_tokens, reasoning_tokens, pricing_model_id, cost_formula_json, cost_calculated_at, project_id, api_key_prefix, purpose, parent_request_id, attempt_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), ?, ?, ?, ?, ?)",
    )
    .bind(record.parent_request_id)
    .bind(record.provider_id)
    .bind(record.model_id)
    .bind(usage.prompt_tokens as i64)
    .bind(usage.completion_tokens as i64)
    .bind(cost.amount_usd)
    .bind(&cost.confidence)
    .bind(free_tier)
    .bind(usage.prompt_cache_hit_tokens.map(|value| value as i64))
    .bind(usage.prompt_cache_miss_tokens.map(|value| value as i64))
    .bind(usage.reasoning_tokens.map(|value| value as i64))
    .bind(cost.pricing_model_id)
    .bind(cost.formula_json.to_string())
    .bind(&project.project_id)
    .bind(&project.api_key_prefix)
    .bind(record.purpose)
    .bind(record.parent_request_id)
    .bind(&attempt_id)
    .execute(&state.db)
    .await?;
    crate::metrics::prometheus::record_internal_usage(
        record.purpose,
        if record.unknown_usage {
            "unknown"
        } else {
            &cost.confidence
        },
        usage.prompt_tokens,
        usage.completion_tokens,
        cost.amount_usd,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::Config;

    #[tokio::test]
    async fn failure_rows_preserve_classification_and_redact_configured_secrets() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("src/db/migrations")
            .run(&pool)
            .await
            .unwrap();
        let mut config = Config::default();
        config.server.master_api_key = "secret-test-key".into();
        let state = AppState::new(
            config,
            pool,
            Default::default(),
            tokio::sync::broadcast::channel(1).0,
        );

        record_failure(
            &state,
            FailureRecord {
                request_id: "failed-stream",
                endpoint_kind: "chat",
                requested_model: "preview:ox-alpha",
                selected_provider_id: Some("openrouter"),
                selected_model_id: Some("stealth/ox-alpha"),
                status: "route_exhausted",
                http_status: 503,
                latency_ms: 180_000,
                streaming: true,
                error_code: Some("stream_timeout_pre_content"),
                error_summary: Some("provider echoed secret-test-key before timeout"),
            },
        )
        .await
        .unwrap();

        let row = sqlx::query_as::<_, (Option<String>, Option<String>)>(
            "SELECT error_code, error_summary FROM request_log WHERE request_id = ?",
        )
        .bind("failed-stream")
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(row.0.as_deref(), Some("stream_timeout_pre_content"));
        assert_eq!(
            row.1.as_deref(),
            Some("provider echoed [REDACTED] before timeout")
        );
    }
}
