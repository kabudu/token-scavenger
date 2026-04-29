use crate::app::state::AppState;

/// Get usage time-series data for the analytics view.
pub async fn get_usage_series(state: &AppState) -> serde_json::Value {
    let result = sqlx::query_as::<_, (String, i64, i64, f64, bool)>(
        "SELECT provider_id, SUM(input_tokens), SUM(output_tokens), SUM(estimated_cost_usd), free_tier
         FROM usage_events
         WHERE timestamp > datetime('now', '-24 hours')
         GROUP BY provider_id, free_tier"
    )
    .fetch_all(&state.db)
    .await;

    match result {
        Ok(rows) => {
            let series: Vec<serde_json::Value> = rows
                .into_iter()
                .map(|(p, inp, out, cost, free)| {
                    serde_json::json!({
                        "provider_id": p,
                        "input_tokens": inp,
                        "output_tokens": out,
                        "estimated_cost_usd": cost,
                        "free_tier": free,
                    })
                })
                .collect();
            serde_json::json!({"series": series, "period": "24h"})
        }
        Err(_) => serde_json::json!({"series": [], "period": "24h"}),
    }
}
