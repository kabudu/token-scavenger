use crate::api::openai::chat::UsageResponse;
use crate::app::state::AppState;
use tracing::info;

/// Inputs needed to persist a completed request and usage event.
pub struct UsageRecord<'a> {
    pub provider_id: &'a str,
    pub model_id: &'a str,
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
}

/// Record a usage event for a completed request.
pub async fn record_usage(state: &AppState, record: UsageRecord<'_>) -> Result<(), sqlx::Error> {
    let usage = record.usage.unwrap_or(&UsageResponse {
        prompt_tokens: 0,
        completion_tokens: 0,
        total_tokens: 0,
    });

    let estimated_cost = crate::usage::pricing::estimate_cost(
        usage.prompt_tokens,
        usage.completion_tokens,
        record.provider_id,
    );

    sqlx::query(
        "INSERT INTO request_log (request_id, endpoint_kind, requested_model, selected_provider_id, selected_model_id, status, http_status, latency_ms, streaming)
         VALUES (?, ?, ?, ?, ?, 'success', 200, ?, ?)"
    )
    .bind(record.request_id)
    .bind(record.endpoint_kind)
    .bind(record.model_id)
    .bind(record.provider_id)
    .bind(record.model_id)
    .bind(record.latency_ms)
    .bind(record.streaming)
    .execute(&state.db)
    .await?;

    sqlx::query(
        "INSERT INTO usage_events (request_id, provider_id, model_id, input_tokens, output_tokens, estimated_cost_usd, free_tier)
         VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(record.request_id)
    .bind(record.provider_id)
    .bind(record.model_id)
    .bind(usage.prompt_tokens as i64)
    .bind(usage.completion_tokens as i64)
    .bind(estimated_cost)
    .bind(record.free_tier)
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

    info!(
        request_id = %record.request_id,
        provider = %record.provider_id,
        model = %record.model_id,
        prompt_tokens = usage.prompt_tokens,
        completion_tokens = usage.completion_tokens,
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
    sqlx::query(
        "INSERT OR IGNORE INTO request_log (request_id, endpoint_kind, requested_model, selected_provider_id, selected_model_id, status, http_status, latency_ms, streaming)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
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
    .execute(&state.db)
    .await?;

    crate::metrics::prometheus::record_request(
        record.selected_provider_id.unwrap_or("none"),
        record.selected_model_id.unwrap_or(record.requested_model),
        record.endpoint_kind,
        record.status,
    );

    info!(
        request_id = %record.request_id,
        provider = record.selected_provider_id.unwrap_or("none"),
        model = record.requested_model,
        status = record.status,
        latency_ms = record.latency_ms,
        "Failed request recorded"
    );

    Ok(())
}
