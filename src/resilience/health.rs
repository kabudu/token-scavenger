use crate::app::state::AppState;

/// Provider health states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthState {
    Healthy,
    Degraded,
    RateLimited,
    QuotaExhausted,
    Unhealthy,
    Disabled,
}

/// Serializable health state for the UI/API.
#[derive(Debug, Clone)]
pub struct ProviderHealthState {
    pub state: HealthState,
    pub last_success_at: Option<i64>,
    pub last_error_at: Option<i64>,
    pub recent_successes: u32,
    pub recent_failures: u32,
}

impl ProviderHealthState {
    pub fn new() -> Self {
        Self {
            state: HealthState::Healthy,
            last_success_at: None,
            last_error_at: None,
            recent_successes: 0,
            recent_failures: 0,
        }
    }

    pub fn value(&self) -> &HealthState {
        &self.state
    }
}

impl Default for ProviderHealthState {
    fn default() -> Self {
        Self::new()
    }
}

/// Record a successful request for a provider.
pub async fn record_success(state: &AppState, provider_id: &str) {
    let mut entry = state
        .health_states
        .entry(provider_id.to_string())
        .or_default();
    entry.recent_successes += 1;
    entry.last_success_at = Some(chrono::Utc::now().timestamp());
}

/// Record a failed request for a provider.
pub async fn record_failure(state: &AppState, provider_id: &str) {
    let mut entry = state
        .health_states
        .entry(provider_id.to_string())
        .or_default();
    entry.recent_failures += 1;
    entry.last_error_at = Some(chrono::Utc::now().timestamp());

    // Update health state based on failure ratio
    let total = entry.recent_successes + entry.recent_failures;
    if total > 5 {
        let failure_ratio = entry.recent_failures as f64 / total as f64;
        entry.state = if failure_ratio > 0.5 {
            HealthState::Unhealthy
        } else if failure_ratio > 0.2 {
            HealthState::Degraded
        } else {
            HealthState::Healthy
        };
    }
}

/// Get recent health events for the admin API.
pub async fn get_recent_events(state: &AppState) -> serde_json::Value {
    let result = sqlx::query_as::<_, (String, String, String, Option<i64>)>(
        "SELECT provider_id, health_state, event_type, recorded_at FROM provider_health_events ORDER BY recorded_at DESC LIMIT 50"
    )
    .fetch_all(&state.db)
    .await;

    match result {
        Ok(rows) => {
            let events: Vec<serde_json::Value> = rows
                .into_iter()
                .map(|(p, h, et, _ra)| {
                    serde_json::json!({
                        "provider_id": p,
                        "health_state": h,
                        "event_type": et,
                    })
                })
                .collect();
            serde_json::json!({"events": events})
        }
        Err(_) => serde_json::json!({"events": []}),
    }
}
