use crate::app::state::AppState;

/// Get usage time-series data for the analytics view.
pub async fn get_usage_series(state: &AppState, period: &str) -> serde_json::Value {
    let interval = match period {
        "7d" => "-7 days",
        "30d" => "-30 days",
        "1y" => "-1 year",
        _ => "-24 hours",
    };

    let result = sqlx::query_as::<_, (String, i64, i64, f64, bool)>(
        &format!(
            "SELECT provider_id, SUM(input_tokens), SUM(output_tokens), SUM(estimated_cost_usd), free_tier
             FROM usage_events
             WHERE timestamp > datetime('now', '{}')
             GROUP BY provider_id, free_tier",
            interval
        )
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
            serde_json::json!({"series": series, "period": period})
        }
        Err(_) => serde_json::json!({"series": [], "period": period}),
    }
}

/// Get hourly/daily traffic data for the given period.
pub async fn get_hourly_traffic(state: &AppState, period: &str) -> serde_json::Value {
    let (interval, format) = match period {
        "7d" => ("-7 days", "%Y-%m-%d 00:00:00"),
        "30d" => ("-30 days", "%Y-%m-%d 00:00:00"),
        "1y" => ("-1 year", "%Y-%m-01 00:00:00"),
        _ => ("-24 hours", "%Y-%m-%d %H:00:00"),
    };

    let result = sqlx::query_as::<_, (String, i64, bool)>(&format!(
        "SELECT strftime('{}', timestamp) as time_bucket,
                    SUM(input_tokens + output_tokens) as total_tokens,
                    free_tier
             FROM usage_events
             WHERE timestamp > datetime('now', '{}')
             GROUP BY time_bucket, free_tier
             ORDER BY time_bucket ASC",
        format, interval
    ))
    .fetch_all(&state.db)
    .await;

    match result {
        Ok(rows) => {
            let mut labels = std::collections::BTreeSet::new();
            let mut free_data = std::collections::HashMap::new();
            let mut paid_data = std::collections::HashMap::new();

            for (bucket, tokens, is_free) in rows {
                let label = if period == "24h" {
                    if bucket.len() >= 16 {
                        bucket[11..16].to_string()
                    } else {
                        bucket.clone()
                    }
                } else if period == "1y" {
                    if bucket.len() >= 7 {
                        bucket[0..7].to_string()
                    } else {
                        bucket.clone()
                    }
                } else {
                    if bucket.len() >= 10 {
                        bucket[5..10].to_string()
                    } else {
                        bucket.clone()
                    }
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

/// Get provider token distribution for the given period.
pub async fn get_provider_distribution(state: &AppState, period: &str) -> serde_json::Value {
    let interval = match period {
        "7d" => "-7 days",
        "30d" => "-30 days",
        "1y" => "-1 year",
        _ => "-24 hours",
    };

    let result = sqlx::query_as::<_, (String, i64)>(&format!(
        "SELECT provider_id, SUM(input_tokens + output_tokens) as total_tokens
             FROM usage_events
             WHERE timestamp > datetime('now', '{}')
             GROUP BY provider_id
             ORDER BY total_tokens DESC",
        interval
    ))
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

/// Get request count and average latency for the given period.
pub async fn get_period_summary(state: &AppState, period: &str) -> serde_json::Value {
    let interval = match period {
        "7d" => "-7 days",
        "30d" => "-30 days",
        "1y" => "-1 year",
        _ => "-24 hours",
    };

    let metrics = sqlx::query_as::<_, (i64, i64)>(&format!(
        "SELECT COUNT(*), COALESCE(CAST(AVG(latency_ms) AS INTEGER), 0)
             FROM request_log
             WHERE received_at > datetime('now', '{}')",
        interval
    ))
    .fetch_one(&state.db)
    .await
    .unwrap_or((0, 0));

    serde_json::json!({
        "request_count": metrics.0,
        "avg_latency": metrics.1,
    })
}
