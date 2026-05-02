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

/// Get hourly traffic data for the last 24 hours, split by free/paid tier.
pub async fn get_hourly_traffic(state: &AppState) -> serde_json::Value {
    let result = sqlx::query_as::<_, (String, i64, bool)>(
        "SELECT strftime('%Y-%m-%d %H:00:00', timestamp) as hour,
                SUM(input_tokens + output_tokens) as total_tokens,
                free_tier
         FROM usage_events
         WHERE timestamp > datetime('now', '-24 hours')
         GROUP BY hour, free_tier
         ORDER BY hour ASC",
    )
    .fetch_all(&state.db)
    .await;

    match result {
        Ok(rows) => {
            let mut labels = std::collections::BTreeSet::new();
            let mut free_data = std::collections::HashMap::new();
            let mut paid_data = std::collections::HashMap::new();

            for (hour, tokens, is_free) in rows {
                // Keep only the HH:00 part for labels
                let label = if hour.len() >= 16 {
                    hour[11..16].to_string()
                } else {
                    hour.clone()
                };
                labels.insert(label.clone());
                if is_free {
                    *free_data.entry(label).or_insert(0) += tokens;
                } else {
                    *paid_data.entry(label).or_insert(0) += tokens;
                }
            }

            let sorted_labels: Vec<String> = labels.into_iter().collect();
            let free_series: Vec<i64> = sorted_labels
                .iter()
                .map(|l| *free_data.get(l).unwrap_or(&0))
                .collect();
            let paid_series: Vec<i64> = sorted_labels
                .iter()
                .map(|l| *paid_data.get(l).unwrap_or(&0))
                .collect();

            serde_json::json!({
                "labels": sorted_labels,
                "free_tokens": free_series,
                "paid_tokens": paid_series
            })
        }
        Err(_) => serde_json::json!({"labels": [], "free_tokens": [], "paid_tokens": []}),
    }
}

/// Get provider token distribution for the last 24 hours.
pub async fn get_provider_distribution(state: &AppState) -> serde_json::Value {
    let result = sqlx::query_as::<_, (String, i64)>(
        "SELECT provider_id, SUM(input_tokens + output_tokens) as total_tokens
         FROM usage_events
         WHERE timestamp > datetime('now', '-24 hours')
         GROUP BY provider_id
         ORDER BY total_tokens DESC",
    )
    .fetch_all(&state.db)
    .await;

    match result {
        Ok(rows) => {
            let labels: Vec<String> = rows.iter().map(|(p, _)| p.clone()).collect();
            let data: Vec<i64> = rows.iter().map(|(_, t)| *t).collect();
            serde_json::json!({
                "labels": labels,
                "data": data
            })
        }
        Err(_) => serde_json::json!({"labels": [], "data": []}),
    }
}
