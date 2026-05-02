use crate::app::state::AppState;
use crate::resilience::breaker::{BreakerState, CircuitBreakerState};

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
    entry.state = HealthState::Healthy;
    let _ = sqlx::query(
        "INSERT INTO provider_health_events (provider_id, health_state, breaker_state, event_type)
         VALUES (?, 'healthy', ?, 'passive_success')",
    )
    .bind(provider_id)
    .bind(breaker_state_name(state, provider_id))
    .execute(&state.db)
    .await;
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
    let _ = sqlx::query(
        "INSERT INTO provider_health_events (provider_id, health_state, breaker_state, event_type)
         VALUES (?, ?, ?, 'passive_failure')",
    )
    .bind(provider_id)
    .bind(health_state_name(&entry.state))
    .bind(breaker_state_name(state, provider_id))
    .execute(&state.db)
    .await;
}

/// Probe a provider by running a bounded adapter discovery call.
pub async fn probe_provider(state: &AppState, provider_id: &str) -> bool {
    let config = state.config();
    let Some(provider_cfg) = config
        .providers
        .iter()
        .find(|p| p.id == provider_id && p.enabled)
    else {
        state.health_states.insert(
            provider_id.to_string(),
            ProviderHealthState {
                state: HealthState::Disabled,
                ..ProviderHealthState::new()
            },
        );
        return false;
    };
    let Some(adapter) = state.provider_registry.get(provider_id).await else {
        record_failure(state, provider_id).await;
        return false;
    };

    let ctx = crate::providers::traits::ProviderContext {
        base_url: adapter.base_url(provider_cfg),
        api_key: provider_cfg.api_key.clone(),
        config: std::sync::Arc::new(provider_cfg.clone()),
        client: state.http_client.clone(),
    };
    let started = std::time::Instant::now();
    let timeout = std::time::Duration::from_millis(config.server.request_timeout_ms);
    let result = tokio::time::timeout(timeout, adapter.discover_models(&ctx)).await;
    let latency_ms = started.elapsed().as_millis() as i64;

    match result {
        Ok(Ok(_)) => {
            record_success(state, provider_id).await;
            state.breaker_states.insert(
                provider_id.to_string(),
                CircuitBreakerState::new(
                    BreakerState::Closed,
                    0,
                    config.resilience.breaker_failure_threshold,
                ),
            );
            crate::metrics::prometheus::record_breaker_state(provider_id, "closed");
            let _ = sqlx::query(
                "INSERT INTO provider_health_events (provider_id, health_state, breaker_state, latency_ms, event_type)
                 VALUES (?, 'healthy', 'closed', ?, 'active_probe_success')",
            )
            .bind(provider_id)
            .bind(latency_ms)
            .execute(&state.db)
            .await;
            true
        }
        Ok(Err(err)) => {
            record_failure(state, provider_id).await;
            let failures = state
                .health_states
                .get(provider_id)
                .map(|entry| entry.recent_failures)
                .unwrap_or(1);
            let breaker_state = if failures >= config.resilience.breaker_failure_threshold {
                BreakerState::Open
            } else {
                BreakerState::Closed
            };
            let breaker_name = breaker_state_label(&breaker_state);
            state.breaker_states.insert(
                provider_id.to_string(),
                CircuitBreakerState::new(
                    breaker_state,
                    failures,
                    config.resilience.breaker_failure_threshold,
                ),
            );
            crate::metrics::prometheus::record_breaker_state(provider_id, breaker_name);
            let _ = sqlx::query(
                "INSERT INTO provider_health_events (provider_id, health_state, breaker_state, latency_ms, event_type, details_json)
                 VALUES (?, 'unhealthy', ?, ?, 'active_probe_failure', ?)",
            )
            .bind(provider_id)
            .bind(breaker_name)
            .bind(latency_ms)
            .bind(serde_json::json!({"error": err.to_string()}).to_string())
            .execute(&state.db)
            .await;
            false
        }
        Err(_) => {
            record_failure(state, provider_id).await;
            state.breaker_states.insert(
                provider_id.to_string(),
                CircuitBreakerState::new(
                    BreakerState::Open,
                    config.resilience.breaker_failure_threshold,
                    config.resilience.breaker_failure_threshold,
                ),
            );
            crate::metrics::prometheus::record_breaker_state(provider_id, "open");
            let _ = sqlx::query(
                "INSERT INTO provider_health_events (provider_id, health_state, breaker_state, latency_ms, event_type, details_json)
                 VALUES (?, 'unhealthy', 'open', ?, 'active_probe_timeout', ?)",
            )
            .bind(provider_id)
            .bind(latency_ms)
            .bind(serde_json::json!({"error": "probe timed out"}).to_string())
            .execute(&state.db)
            .await;
            false
        }
    }
}

/// Attempt explicit half-open recovery for open breakers.
pub async fn recover_open_breakers(state: &AppState) {
    let provider_ids: Vec<String> = state
        .breaker_states
        .iter()
        .filter(|entry| entry.value().state() == BreakerState::Open)
        .map(|entry| entry.key().clone())
        .collect();

    for provider_id in provider_ids {
        state.breaker_states.insert(
            provider_id.clone(),
            CircuitBreakerState::new(
                BreakerState::HalfOpen,
                0,
                state.config().resilience.breaker_failure_threshold,
            ),
        );
        crate::metrics::prometheus::record_breaker_state(&provider_id, "half_open");
        let recovered = probe_provider(state, &provider_id).await;
        if !recovered {
            state.breaker_states.insert(
                provider_id.clone(),
                CircuitBreakerState::new(
                    BreakerState::Open,
                    state.config().resilience.breaker_failure_threshold,
                    state.config().resilience.breaker_failure_threshold,
                ),
            );
        }
    }
}

fn health_state_name(state: &HealthState) -> &'static str {
    match state {
        HealthState::Healthy => "healthy",
        HealthState::Degraded => "degraded",
        HealthState::RateLimited => "rate_limited",
        HealthState::QuotaExhausted => "quota_exhausted",
        HealthState::Unhealthy => "unhealthy",
        HealthState::Disabled => "disabled",
    }
}

fn breaker_state_name(state: &AppState, provider_id: &str) -> &'static str {
    state
        .breaker_states
        .get(provider_id)
        .map(|b| breaker_state_label(&b.state()))
        .unwrap_or("closed")
}

fn breaker_state_label(state: &BreakerState) -> &'static str {
    match state {
        BreakerState::Closed => "closed",
        BreakerState::Open => "open",
        BreakerState::HalfOpen => "half_open",
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
