use crate::api::openai::chat::UsageResponse;
use crate::app::state::AppState;
use uuid::Uuid;
use tracing::info;

/// Record a usage event for a completed request.
pub async fn record_usage(
    state: &AppState,
    provider_id: &str,
    model_id: &str,
    usage: Option<&UsageResponse>,
    latency_ms: i64,
    free_tier: bool,
) -> Result<(), sqlx::Error> {
    let usage = usage.unwrap_or(&UsageResponse {
        prompt_tokens: 0,
        completion_tokens: 0,
        total_tokens: 0,
    });

    let request_id = Uuid::new_v4().to_string();
    let estimated_cost = crate::usage::pricing::estimate_cost(
        usage.prompt_tokens,
        usage.completion_tokens,
        provider_id,
    );

    sqlx::query(
        "INSERT INTO usage_events (request_id, provider_id, model_id, input_tokens, output_tokens, estimated_cost_usd, free_tier)
         VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&request_id)
    .bind(provider_id)
    .bind(model_id)
    .bind(usage.prompt_tokens as i64)
    .bind(usage.completion_tokens as i64)
    .bind(estimated_cost)
    .bind(free_tier)
    .execute(&state.db)
    .await?;

    // Also record to request_log
    sqlx::query(
        "INSERT INTO request_log (request_id, endpoint_kind, requested_model, selected_provider_id, selected_model_id, status, http_status, latency_ms, streaming)
         VALUES (?, 'chat', ?, ?, ?, 'success', 200, ?, 0)"
    )
    .bind(&request_id)
    .bind(model_id)
    .bind(provider_id)
    .bind(model_id)
    .bind(latency_ms)
    .execute(&state.db)
    .await?;

    // Emit metrics
    crate::metrics::prometheus::record_request(provider_id, model_id, "chat", "success");
    crate::metrics::prometheus::record_tokens(provider_id, model_id, "input", usage.prompt_tokens);
    crate::metrics::prometheus::record_tokens(provider_id, model_id, "output", usage.completion_tokens);

    info!(
        request_id = %request_id,
        provider = %provider_id,
        model = %model_id,
        prompt_tokens = usage.prompt_tokens,
        completion_tokens = usage.completion_tokens,
        latency_ms = latency_ms,
        "Usage recorded"
    );

    Ok(())
}
