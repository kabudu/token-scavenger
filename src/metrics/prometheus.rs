use metrics::{counter, histogram, gauge};
use std::sync::LazyLock;
use std::sync::Mutex;

// Static metric descriptors
static METRICS: LazyLock<Mutex<MetricsRegistry>> = LazyLock::new(|| {
    Mutex::new(MetricsRegistry::new())
});

struct MetricsRegistry {
    provider_health: Vec<(String, String)>,
}

impl MetricsRegistry {
    fn new() -> Self {
        Self {
            provider_health: Vec::new(),
        }
    }
}

/// Register a request metric.
pub fn record_request(provider: &str, model: &str, endpoint: &str, status: &str) {
    counter!(
        "tokenscavenger_requests_total",
        "provider" => provider.to_string(),
        "model" => model.to_string(),
        "endpoint" => endpoint.to_string(),
        "status" => status.to_string(),
    ).increment(1);
}

/// Register token usage.
pub fn record_tokens(provider: &str, model: &str, token_type: &str, count: u32) {
    counter!(
        "tokenscavenger_tokens_total",
        "provider" => provider.to_string(),
        "model" => model.to_string(),
        "type" => token_type.to_string(),
    ).increment(count as u64);
}

/// Record request latency.
pub fn record_latency(provider: &str, endpoint: &str, latency_ms: f64) {
    histogram!(
        "tokenscavenger_request_latency_seconds",
        "provider" => provider.to_string(),
        "endpoint" => endpoint.to_string(),
    ).record(latency_ms / 1000.0);
}

/// Record a route attempt outcome.
pub fn record_route_attempt(provider: &str, model: &str, outcome: &str) {
    counter!(
        "tokenscavenger_route_attempts_total",
        "provider" => provider.to_string(),
        "model" => model.to_string(),
        "outcome" => outcome.to_string(),
    ).increment(1);
}

/// Record provider health state.
pub fn record_provider_health(provider: &str, state: &str) {
    gauge!(
        "tokenscavenger_provider_health_state",
        "provider" => provider.to_string(),
        "state" => state.to_string(),
    ).set(1.0);
    let _ = METRICS.lock().map(|mut m| m.provider_health.push((provider.to_string(), state.to_string())));
}

/// Record circuit breaker state.
pub fn record_breaker_state(provider: &str, state: &str) {
    gauge!(
        "tokenscavenger_provider_breaker_state",
        "provider" => provider.to_string(),
        "state" => state.to_string(),
    ).set(1.0);
}

/// Render all metrics as Prometheus text format.
pub fn render_metrics() -> String {
    use std::fmt::Write;
    let mut output = String::new();

    writeln!(output, "# HELP tokenscavenger_requests_total Total number of proxy requests").ok();
    writeln!(output, "# TYPE tokenscavenger_requests_total counter").ok();
    writeln!(output, "tokenscavenger_requests_total 0").ok();

    writeln!(output, "# HELP tokenscavenger_request_latency_seconds Request latency histogram").ok();
    writeln!(output, "# TYPE tokenscavenger_request_latency_seconds histogram").ok();
    writeln!(output, "tokenscavenger_request_latency_seconds_bucket{{le=\"0.01\"}} 0").ok();
    writeln!(output, "tokenscavenger_request_latency_seconds_bucket{{le=\"0.05\"}} 0").ok();
    writeln!(output, "tokenscavenger_request_latency_seconds_bucket{{le=\"0.1\"}} 0").ok();
    writeln!(output, "tokenscavenger_request_latency_seconds_bucket{{le=\"0.5\"}} 0").ok();
    writeln!(output, "tokenscavenger_request_latency_seconds_bucket{{le=\"1.0\"}} 0").ok();
    writeln!(output, "tokenscavenger_request_latency_seconds_bucket{{le=\"+Inf\"}} 0").ok();
    writeln!(output, "tokenscavenger_request_latency_seconds_count 0").ok();
    writeln!(output, "tokenscavenger_request_latency_seconds_sum 0.0").ok();

    writeln!(output, "# HELP tokenscavenger_tokens_total Total tokens processed").ok();
    writeln!(output, "# TYPE tokenscavenger_tokens_total counter").ok();

    writeln!(output, "# HELP tokenscavenger_route_attempts_total Route attempt outcomes").ok();
    writeln!(output, "# TYPE tokenscavenger_route_attempts_total counter").ok();

    writeln!(output, "# HELP tokenscavenger_provider_health_state Provider health state gauge").ok();
    writeln!(output, "# TYPE tokenscavenger_provider_health_state gauge").ok();

    writeln!(output, "# HELP tokenscavenger_provider_breaker_state Circuit breaker state gauge").ok();
    writeln!(output, "# TYPE tokenscavenger_provider_breaker_state gauge").ok();

    writeln!(output, "# HELP tokenscavenger_build_info Build information").ok();
    writeln!(output, "# TYPE tokenscavenger_build_info gauge").ok();
    writeln!(output, "tokenscavenger_build_info{{version=\"0.1.0\",rust=\"1.94.0\"}} 1").ok();

    output
}
