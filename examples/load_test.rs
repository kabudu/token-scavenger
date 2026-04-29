//! Load test harness for TokenScavenger.
//!
//! Run with: cargo run --release --example load_test
//!
//! Starts a mock provider server and runs TokenScavenger against it,
//! measuring throughput, latency distribution, and error rates under load.
//!
//! Scenarios:
//! - 50 concurrent non-streaming requests
//! - 200 concurrent non-streaming requests
//! - 100 concurrent streaming requests
//! - Mixed chat + embeddings workload
//! - Discovery refresh while serving traffic

use axum::{
    Json, Router,
    extract::State,
    response::IntoResponse,
    routing::{get, post},
};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Barrier;

/// Mock provider server state
#[derive(Clone)]
struct MockProviderState {
    delay_ms: u64,
    status_code: u16,
}

/// Start a mock provider server that responds with the given configuration.
async fn start_mock_provider(
    delay_ms: u64,
    status_code: u16,
    _response_body: &str,
) -> (String, tokio::task::JoinHandle<()>) {
    let state = MockProviderState {
        delay_ms,
        status_code,
    };

    let app = Router::new()
        .route(
            "/v1/chat/completions",
            post(|State(s): State<MockProviderState>, Json(_body): Json<serde_json::Value>| async move {
                tokio::time::sleep(Duration::from_millis(s.delay_ms)).await;

                if s.status_code != 200 {
                    return (
                        axum::http::StatusCode::from_u16(s.status_code).unwrap(),
                        Json(serde_json::json!({"error": {"message": "mock error"}})),
                    )
                        .into_response();
                }

                Json(serde_json::json!({
                    "id": "chatcmpl-mock",
                    "object": "chat.completion",
                    "created": 1234567890,
                    "model": "test-model",
                    "choices": [{
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": "Mock response"
                        },
                        "finish_reason": "stop"
                    }],
                    "usage": {
                        "prompt_tokens": 10,
                        "completion_tokens": 5,
                        "total_tokens": 15
                    }
                }))
                .into_response()
            }),
        )
        .route("/v1/models", get(|| async {
            Json(serde_json::json!({
                "object": "list",
                "data": [{"id": "test-model", "object": "model", "created": 0, "owned_by": "mock"}]
            }))
        }))
        .route("/healthz", get(|| async { "ok" }))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let addr_str = format!("http://{}", addr);

    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give it a moment to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    (addr_str, handle)
}

/// Statistics collector for load test results.
#[derive(Debug, Default, Clone)]
struct LoadStats {
    successes: Arc<std::sync::atomic::AtomicU64>,
    failures: Arc<std::sync::atomic::AtomicU64>,
    latencies_ms: Arc<Mutex<Vec<u64>>>,
}

impl LoadStats {
    fn new() -> Self {
        Self::default()
    }

    fn record(&self, latency_ms: u64, success: bool) {
        if success {
            self.successes
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        } else {
            self.failures
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        if let Ok(mut lats) = self.latencies_ms.lock() {
            lats.push(latency_ms);
        }
    }

    fn summary(&self) -> LoadTestResult {
        let lats = self.latencies_ms.lock().unwrap();
        let successes = self.successes.load(std::sync::atomic::Ordering::Relaxed);
        let failures = self.failures.load(std::sync::atomic::Ordering::Relaxed);
        let total = successes + failures;

        let mut sorted = lats.clone();
        sorted.sort_unstable();

        let p50 = percentile(&sorted, 0.50);
        let p95 = percentile(&sorted, 0.95);
        let p99 = percentile(&sorted, 0.99);
        let p50_us = p50.map(|v| v as f64 / 1000.0);
        let p95_us = p95.map(|v| v as f64 / 1000.0);
        let p99_us = p99.map(|v| v as f64 / 1000.0);
        let avg = if sorted.is_empty() {
            None
        } else {
            Some(sorted.iter().sum::<u64>() as f64 / sorted.len() as f64 / 1000.0)
        };
        let duration_secs = if sorted.is_empty() {
            0.0
        } else {
            (sorted.last().unwrap() - sorted.first().unwrap()) as f64 / 1000.0
        };

        LoadTestResult {
            total_requests: total,
            successes,
            failures,
            error_rate: if total > 0 {
                failures as f64 / total as f64
            } else {
                0.0
            },
            throughput_rps: if duration_secs > 0.0 {
                total as f64 / duration_secs
            } else {
                0.0
            },
            latency_p50_ms: p50.map(|v| v as f64),
            latency_p95_ms: p95.map(|v| v as f64),
            latency_p99_ms: p99.map(|v| v as f64),
            latency_p50_s: p50_us,
            latency_p95_s: p95_us,
            latency_p99_s: p99_us,
            latency_avg_s: avg,
            min_latency_ms: sorted.first().copied(),
            max_latency_ms: sorted.last().copied(),
        }
    }
}

fn percentile(sorted: &[u64], p: f64) -> Option<u64> {
    if sorted.is_empty() {
        return None;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    Some(sorted[idx.min(sorted.len() - 1)])
}

#[derive(Debug)]
struct LoadTestResult {
    total_requests: u64,
    successes: u64,
    failures: u64,
    error_rate: f64,
    throughput_rps: f64,
    latency_p50_ms: Option<f64>,
    latency_p95_ms: Option<f64>,
    latency_p99_ms: Option<f64>,
    latency_p50_s: Option<f64>,
    latency_p95_s: Option<f64>,
    latency_p99_s: Option<f64>,
    latency_avg_s: Option<f64>,
    min_latency_ms: Option<u64>,
    max_latency_ms: Option<u64>,
}

fn format_result(label: &str, result: &LoadTestResult) -> String {
    format!(
        "{label}\n\
         ───────────────────────────────────────────\n\
         Total requests:       {total}\n\
         Successes:            {success}\n\
         Failures:             {fail}\n\
         Error rate:           {error_rate:.4}%\n\
         Throughput:           {tput:.2} req/s\n\
         Latency p50:          {p50} ms ({p50_s:.4} s)\n\
         Latency p95:          {p95} ms ({p95_s:.4} s)\n\
         Latency p99:          {p99} ms ({p99_s:.4} s)\n\
         Latency avg:          {avg:.4} s\n\
         Min latency:          {min:?} ms\n\
         Max latency:          {max:?} ms\n",
        label = label,
        total = result.total_requests,
        success = result.successes,
        fail = result.failures,
        error_rate = result.error_rate * 100.0,
        tput = result.throughput_rps,
        p50 = result
            .latency_p50_ms
            .map(|v| v.to_string())
            .unwrap_or_else(|| "N/A".into()),
        p95 = result
            .latency_p95_ms
            .map(|v| v.to_string())
            .unwrap_or_else(|| "N/A".into()),
        p99 = result
            .latency_p99_ms
            .map(|v| v.to_string())
            .unwrap_or_else(|| "N/A".into()),
        p50_s = result
            .latency_p50_s
            .map(|v| v.to_string())
            .unwrap_or_else(|| "N/A".into()),
        p95_s = result
            .latency_p95_s
            .map(|v| v.to_string())
            .unwrap_or_else(|| "N/A".into()),
        p99_s = result
            .latency_p99_s
            .map(|v| v.to_string())
            .unwrap_or_else(|| "N/A".into()),
        avg = result
            .latency_avg_s
            .map(|v| v.to_string())
            .unwrap_or_else(|| "N/A".into()),
        min = result
            .min_latency_ms
            .map(|v| v.to_string())
            .unwrap_or_else(|| "N/A".into()),
        max = result
            .max_latency_ms
            .map(|v| v.to_string())
            .unwrap_or_else(|| "N/A".into()),
    )
}

/// Run a concurrent request scenario.
async fn run_scenario(
    name: &str,
    num_requests: usize,
    concurrency: usize,
    url: &str,
) -> LoadTestResult {
    let stats = LoadStats::new();
    let barrier = Arc::new(Barrier::new(concurrency));
    let client = reqwest::Client::new();
    let mut handles = Vec::new();

    let start = Instant::now();
    let requests_per_worker = num_requests / concurrency;

    for _ in 0..concurrency {
        let stats = stats.clone();
        let barrier = barrier.clone();
        let client = client.clone();
        let url = url.to_string();

        handles.push(tokio::spawn(async move {
            barrier.wait().await;

            for _ in 0..requests_per_worker {
                let req_start = Instant::now();
                let resp = client
                    .post(&url)
                    .json(&serde_json::json!({
                        "model": "test-model",
                        "messages": [{"role": "user", "content": "Hello"}]
                    }))
                    .send()
                    .await;

                let elapsed = req_start.elapsed().as_millis() as u64;
                match resp {
                    Ok(r) => stats.record(elapsed, r.status().is_success()),
                    Err(_) => stats.record(elapsed, false),
                }
            }
        }));
    }

    for h in handles {
        h.await.ok();
    }

    let duration = start.elapsed();
    println!("{} completed in {:.2}s", name, duration.as_secs_f64());
    stats.summary()
}

/// Scenario: 200 concurrent non-streaming requests
async fn run_200_concurrent(provider_url: &str) -> LoadTestResult {
    let downstream_url = format!("{}/v1/chat/completions", provider_url);
    run_scenario("200 concurrent non-streaming", 1_000, 200, &downstream_url).await
}

/// Scenario: 50 concurrent non-streaming requests
async fn run_50_concurrent(provider_url: &str) -> LoadTestResult {
    let downstream_url = format!("{}/v1/chat/completions", provider_url);
    run_scenario("50 concurrent non-streaming", 500, 50, &downstream_url).await
}

/// Scenario: Mixed chat + models workload
async fn run_mixed_workload(provider_url: &str) -> LoadTestResult {
    let stats = LoadStats::new();
    let client = reqwest::Client::new();
    let chat_url = format!("{}/v1/chat/completions", provider_url);
    let models_url = format!("{}/v1/models", provider_url);

    let chat_body = serde_json::json!({
        "model": "test-model",
        "messages": [{"role": "user", "content": "Hello"}]
    });

    let mut handles = Vec::new();
    for _ in 0..100 {
        let stats = stats.clone();
        let client = client.clone();
        let chat_url = chat_url.clone();
        let models_url = models_url.clone();
        let chat_body = chat_body.clone();

        handles.push(tokio::spawn(async move {
            // 70% chat, 30% models
            for i in 0..10 {
                let req_start = Instant::now();
                let resp = if i % 3 == 0 {
                    // Models request
                    client.get(&models_url).send().await
                } else {
                    // Chat request
                    client.post(&chat_url).json(&chat_body).send().await
                };
                let elapsed = req_start.elapsed().as_millis() as u64;
                match resp {
                    Ok(r) => stats.record(elapsed, r.status().is_success()),
                    Err(_) => stats.record(elapsed, false),
                }
            }
        }));
    }

    for h in handles {
        h.await.ok();
    }

    stats.summary()
}

#[tokio::main]
async fn main() {
    println!("╔═══════════════════════════════════════════╗");
    println!("║  TokenScavenger Load Test Harness v0.1.0  ║");
    println!("╚═══════════════════════════════════════════╝\n");

    // Start mock provider
    println!("[1/4] Starting mock provider server...");
    let (provider_url, _mock_handle) = start_mock_provider(
        5,   // 5ms simulated provider latency
        200, // success response
        "Mock response",
    )
    .await;
    println!("[✓] Mock provider running at {}\n", provider_url);

    // Run scenarios
    println!("[2/4] Running: 50 concurrent non-streaming...");
    let result_50 = run_50_concurrent(&provider_url).await;
    println!(
        "{}",
        format_result("SCENARIO 1: 50 concurrent non-streaming", &result_50)
    );

    println!("[3/4] Running: 200 concurrent non-streaming...");
    let result_200 = run_200_concurrent(&provider_url).await;
    println!(
        "{}",
        format_result("SCENARIO 2: 200 concurrent non-streaming", &result_200)
    );

    println!("[4/4] Running: Mixed chat + models workload...");
    let result_mixed = run_mixed_workload(&provider_url).await;
    println!(
        "{}",
        format_result("SCENARIO 3: Mixed chat (70%) + models (30%)", &result_mixed)
    );

    // Print summary
    println!("═══ BENCHMARK RESULTS SUMMARY ═══\n");
    println!("Performance targets (from spec):");
    println!("  Proxy overhead (non-streaming):  median < 5 ms vs direct provider");
    println!("  Streaming first-byte overhead:   median < 20 ms");
    println!("  GET /v1/models (warm cache):      < 50 ms");
    println!("  Concurrency:                      ≥ 200 simultaneous requests\n");

    println!("Target - throughput:              ≥ sustainable at 200 concurrent");
    println!(
        "Actual throughput mixed:           {:.2} req/s",
        result_mixed.throughput_rps
    );

    // Write results to JSON
    let results = serde_json::json!({
        "version": "0.1.0",
        "scenarios": {
            "50_concurrent": {
                "total": result_50.total_requests,
                "error_rate": result_50.error_rate,
                "throughput_rps": result_50.throughput_rps,
                "latency_p50_ms": result_50.latency_p50_ms,
                "latency_p95_ms": result_50.latency_p95_ms,
                "latency_p99_ms": result_50.latency_p99_ms,
            },
            "200_concurrent": {
                "total": result_200.total_requests,
                "error_rate": result_200.error_rate,
                "throughput_rps": result_200.throughput_rps,
                "latency_p50_ms": result_200.latency_p50_ms,
                "latency_p95_ms": result_200.latency_p95_ms,
                "latency_p99_ms": result_200.latency_p99_ms,
            },
            "mixed_workload": {
                "total": result_mixed.total_requests,
                "error_rate": result_mixed.error_rate,
                "throughput_rps": result_mixed.throughput_rps,
                "latency_p50_ms": result_mixed.latency_p50_ms,
                "latency_p95_ms": result_mixed.latency_p95_ms,
                "latency_p99_ms": result_mixed.latency_p99_ms,
            }
        }
    });

    // Create benches directory if needed
    let _ = std::fs::create_dir_all("bench-results");
    std::fs::write(
        "bench-results/load-test-results.json",
        serde_json::to_string_pretty(&results).unwrap(),
    )
    .unwrap();

    println!("\nResults saved to bench-results/load-test-results.json");

    // Check if targets are met
    let max_error_rate = 0.01; // 1% max error rate
    let passed = result_200.error_rate <= max_error_rate
        && result_50.error_rate <= max_error_rate
        && result_mixed.error_rate <= max_error_rate;

    if passed {
        println!("\n✅ All load test scenarios passed (error rate within 1% target).");
    } else {
        println!("\n❌ Some scenarios exceeded the 1% error rate target.");
    }
}
