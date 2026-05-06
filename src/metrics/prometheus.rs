use std::collections::BTreeMap;
use std::sync::LazyLock;
use std::sync::Mutex;

static METRICS: LazyLock<Mutex<MetricsRegistry>> =
    LazyLock::new(|| Mutex::new(MetricsRegistry::default()));

#[derive(Default)]
struct MetricsRegistry {
    requests: BTreeMap<(String, String, String, String), u64>,
    tokens: BTreeMap<(String, String, String), u64>,
    latencies: BTreeMap<(String, String), Vec<f64>>,
    route_attempts: BTreeMap<(String, String, String), u64>,
    provider_health: BTreeMap<(String, String), f64>,
    breaker_states: BTreeMap<(String, String), f64>,
    quota_remaining: BTreeMap<String, f64>,
    discovery_runs: BTreeMap<(String, String), u64>,
    estimated_cost: BTreeMap<(String, String, String), f64>,
    pricing_refresh: BTreeMap<(String, String), u64>,
    pricing_age_seconds: BTreeMap<String, f64>,
    unknown_price: BTreeMap<(String, String), u64>,
}

/// Register a request metric.
pub fn record_request(provider: &str, model: &str, endpoint: &str, status: &str) {
    if let Ok(mut metrics) = METRICS.lock() {
        *metrics
            .requests
            .entry((
                provider.into(),
                model.into(),
                endpoint.into(),
                status.into(),
            ))
            .or_default() += 1;
    }
}

/// Register token usage.
pub fn record_tokens(provider: &str, model: &str, token_type: &str, count: u32) {
    if let Ok(mut metrics) = METRICS.lock() {
        *metrics
            .tokens
            .entry((provider.into(), model.into(), token_type.into()))
            .or_default() += count as u64;
    }
}

/// Record request latency.
pub fn record_latency(provider: &str, endpoint: &str, latency_ms: f64) {
    if let Ok(mut metrics) = METRICS.lock() {
        metrics
            .latencies
            .entry((provider.into(), endpoint.into()))
            .or_default()
            .push(latency_ms / 1000.0);
    }
}

/// Record a route attempt outcome.
pub fn record_route_attempt(provider: &str, model: &str, outcome: &str) {
    if let Ok(mut metrics) = METRICS.lock() {
        *metrics
            .route_attempts
            .entry((provider.into(), model.into(), outcome.into()))
            .or_default() += 1;
    }
}

/// Record provider health state.
pub fn record_provider_health(provider: &str, state: &str) {
    if let Ok(mut metrics) = METRICS.lock() {
        metrics
            .provider_health
            .insert((provider.into(), state.into()), 1.0);
    }
}

/// Record circuit breaker state.
pub fn record_breaker_state(provider: &str, state: &str) {
    if let Ok(mut metrics) = METRICS.lock() {
        metrics
            .breaker_states
            .insert((provider.into(), state.into()), 1.0);
    }
}

/// Record quota remaining for a provider.
pub fn record_quota_remaining(provider: &str, remaining: f64) {
    if let Ok(mut metrics) = METRICS.lock() {
        metrics.quota_remaining.insert(provider.into(), remaining);
    }
}

/// Record a discovery run for a provider.
pub fn record_discovery_run(provider: &str, status: &str) {
    if let Ok(mut metrics) = METRICS.lock() {
        *metrics
            .discovery_runs
            .entry((provider.into(), status.into()))
            .or_default() += 1;
    }
}

/// Record estimated spend for a completed request.
pub fn record_estimated_cost(provider: &str, model: &str, confidence: &str, amount_usd: f64) {
    if let Ok(mut metrics) = METRICS.lock() {
        *metrics
            .estimated_cost
            .entry((provider.into(), model.into(), confidence.into()))
            .or_default() += amount_usd;
    }
}

/// Record a pricing refresh attempt.
pub fn record_pricing_refresh(provider: &str, status: &str) {
    if let Ok(mut metrics) = METRICS.lock() {
        *metrics
            .pricing_refresh
            .entry((provider.into(), status.into()))
            .or_default() += 1;
    }
}

/// Record the age of a provider pricing source.
pub fn record_pricing_age(provider: &str, age_seconds: f64) {
    if let Ok(mut metrics) = METRICS.lock() {
        metrics
            .pricing_age_seconds
            .insert(provider.into(), age_seconds);
    }
}

/// Record paid usage where no price is known.
pub fn record_unknown_price(provider: &str, model: &str) {
    if let Ok(mut metrics) = METRICS.lock() {
        *metrics
            .unknown_price
            .entry((provider.into(), model.into()))
            .or_default() += 1;
    }
}

/// Render all metrics as Prometheus text format.
pub fn render_metrics() -> String {
    use std::fmt::Write;
    let mut output = String::new();
    let metrics = METRICS.lock().ok();

    writeln!(
        output,
        "# HELP tokenscavenger_requests_total Total number of proxy requests"
    )
    .ok();
    writeln!(output, "# TYPE tokenscavenger_requests_total counter").ok();
    if let Some(metrics) = metrics.as_ref() {
        for ((provider, model, endpoint, status), value) in &metrics.requests {
            writeln!(output, "tokenscavenger_requests_total{{provider=\"{provider}\",model=\"{model}\",endpoint=\"{endpoint}\",status=\"{status}\"}} {value}").ok();
        }
    }

    writeln!(
        output,
        "# HELP tokenscavenger_request_latency_seconds Request latency histogram"
    )
    .ok();
    writeln!(
        output,
        "# TYPE tokenscavenger_request_latency_seconds histogram"
    )
    .ok();
    if let Some(metrics) = metrics.as_ref() {
        for ((provider, endpoint), values) in &metrics.latencies {
            let sum: f64 = values.iter().sum();
            writeln!(output, "tokenscavenger_request_latency_seconds_count{{provider=\"{provider}\",endpoint=\"{endpoint}\"}} {}", values.len()).ok();
            writeln!(output, "tokenscavenger_request_latency_seconds_sum{{provider=\"{provider}\",endpoint=\"{endpoint}\"}} {sum}").ok();
        }
    }

    writeln!(
        output,
        "# HELP tokenscavenger_tokens_total Total tokens processed"
    )
    .ok();
    writeln!(output, "# TYPE tokenscavenger_tokens_total counter").ok();
    if let Some(metrics) = metrics.as_ref() {
        for ((provider, model, token_type), value) in &metrics.tokens {
            writeln!(output, "tokenscavenger_tokens_total{{provider=\"{provider}\",model=\"{model}\",type=\"{token_type}\"}} {value}").ok();
        }
    }

    writeln!(
        output,
        "# HELP tokenscavenger_route_attempts_total Route attempt outcomes"
    )
    .ok();
    writeln!(output, "# TYPE tokenscavenger_route_attempts_total counter").ok();
    if let Some(metrics) = metrics.as_ref() {
        for ((provider, model, outcome), value) in &metrics.route_attempts {
            writeln!(output, "tokenscavenger_route_attempts_total{{provider=\"{provider}\",model=\"{model}\",outcome=\"{outcome}\"}} {value}").ok();
        }
    }

    writeln!(
        output,
        "# HELP tokenscavenger_provider_health_state Provider health state gauge"
    )
    .ok();
    writeln!(output, "# TYPE tokenscavenger_provider_health_state gauge").ok();
    if let Some(metrics) = metrics.as_ref() {
        for ((provider, state), value) in &metrics.provider_health {
            writeln!(output, "tokenscavenger_provider_health_state{{provider=\"{provider}\",state=\"{state}\"}} {value}").ok();
        }
    }

    writeln!(
        output,
        "# HELP tokenscavenger_provider_breaker_state Circuit breaker state gauge"
    )
    .ok();
    writeln!(output, "# TYPE tokenscavenger_provider_breaker_state gauge").ok();
    if let Some(metrics) = metrics.as_ref() {
        for ((provider, state), value) in &metrics.breaker_states {
            writeln!(output, "tokenscavenger_provider_breaker_state{{provider=\"{provider}\",state=\"{state}\"}} {value}").ok();
        }
    }

    writeln!(
        output,
        "# HELP tokenscavenger_quota_remaining Quota remaining per provider"
    )
    .ok();
    writeln!(output, "# TYPE tokenscavenger_quota_remaining gauge").ok();
    if let Some(metrics) = metrics.as_ref() {
        for (provider, value) in &metrics.quota_remaining {
            writeln!(
                output,
                "tokenscavenger_quota_remaining{{provider=\"{provider}\"}} {value}"
            )
            .ok();
        }
    }

    writeln!(
        output,
        "# HELP tokenscavenger_discovery_runs_total Discovery runs per provider"
    )
    .ok();
    writeln!(output, "# TYPE tokenscavenger_discovery_runs_total counter").ok();
    if let Some(metrics) = metrics.as_ref() {
        for ((provider, status), value) in &metrics.discovery_runs {
            writeln!(output, "tokenscavenger_discovery_runs_total{{provider=\"{provider}\",status=\"{status}\"}} {value}").ok();
        }
    }

    writeln!(
        output,
        "# HELP tokenscavenger_estimated_cost_usd_total Estimated request cost in USD"
    )
    .ok();
    writeln!(
        output,
        "# TYPE tokenscavenger_estimated_cost_usd_total counter"
    )
    .ok();
    if let Some(metrics) = metrics.as_ref() {
        for ((provider, model, confidence), value) in &metrics.estimated_cost {
            writeln!(output, "tokenscavenger_estimated_cost_usd_total{{provider=\"{provider}\",model=\"{model}\",confidence=\"{confidence}\"}} {value}").ok();
        }
    }

    writeln!(
        output,
        "# HELP tokenscavenger_pricing_refresh_total Pricing refresh attempts"
    )
    .ok();
    writeln!(
        output,
        "# TYPE tokenscavenger_pricing_refresh_total counter"
    )
    .ok();
    if let Some(metrics) = metrics.as_ref() {
        for ((provider, status), value) in &metrics.pricing_refresh {
            writeln!(output, "tokenscavenger_pricing_refresh_total{{provider=\"{provider}\",status=\"{status}\"}} {value}").ok();
        }
    }

    writeln!(
        output,
        "# HELP tokenscavenger_pricing_age_seconds Age of the active pricing source"
    )
    .ok();
    writeln!(output, "# TYPE tokenscavenger_pricing_age_seconds gauge").ok();
    if let Some(metrics) = metrics.as_ref() {
        for (provider, value) in &metrics.pricing_age_seconds {
            writeln!(
                output,
                "tokenscavenger_pricing_age_seconds{{provider=\"{provider}\"}} {value}"
            )
            .ok();
        }
    }

    writeln!(
        output,
        "# HELP tokenscavenger_usage_unknown_price_total Paid usage events without known pricing"
    )
    .ok();
    writeln!(
        output,
        "# TYPE tokenscavenger_usage_unknown_price_total counter"
    )
    .ok();
    if let Some(metrics) = metrics.as_ref() {
        for ((provider, model), value) in &metrics.unknown_price {
            writeln!(output, "tokenscavenger_usage_unknown_price_total{{provider=\"{provider}\",model=\"{model}\"}} {value}").ok();
        }
    }

    writeln!(output, "# HELP tokenscavenger_build_info Build information").ok();
    writeln!(output, "# TYPE tokenscavenger_build_info gauge").ok();
    writeln!(
        output,
        "tokenscavenger_build_info{{version=\"0.1.0\",rust=\"1.94.0\"}} 1"
    )
    .ok();

    output
}
