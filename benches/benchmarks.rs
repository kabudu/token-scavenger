//! Criterion benchmarks for TokenScavenger core operations.
//!
//! Run with: cargo bench
//!
//! Measures:
//! - Route plan building latency
//! - Alias resolution throughput
//! - Circuit breaker transitions
//! - Secret redaction throughput
//! - Config loading/validation latency
//! - SQLite write throughput for usage events
//! - Direct-provider baseline versus proxy route overhead
//! - Streaming first-byte formatting
//! - Warm model catalog rendering
//! - Large-catalog route planning

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use std::sync::Arc;
use tokio::runtime::Runtime;

// ---------- Route Planning Benchmark ----------

mod route_plan {
    use super::*;
    use async_trait::async_trait;
    use reqwest::header::HeaderMap;
    use tokenscavenger::providers::traits::{
        AuthKind, EndpointKind, ProviderAdapter, ProviderContext, ProviderError,
    };
    use url::Url;

    struct BenchAdapter(&'static str);

    #[async_trait]
    impl ProviderAdapter for BenchAdapter {
        fn provider_id(&self) -> &'static str {
            self.0
        }
        fn display_name(&self) -> &'static str {
            self.0
        }
        fn supports_endpoint(&self, _: &EndpointKind) -> bool {
            true
        }
        fn auth_kind(&self) -> AuthKind {
            AuthKind::None
        }
        fn capabilities(&self) -> tokenscavenger::providers::normalization::ProviderCapabilities {
            tokenscavenger::providers::normalization::ProviderCapabilities::default()
        }
        fn base_url(&self, _: &tokenscavenger::config::schema::ProviderConfig) -> Url {
            "http://bench.local".parse().unwrap()
        }
        fn default_headers(&self, _: &tokenscavenger::config::schema::ProviderConfig) -> HeaderMap {
            HeaderMap::new()
        }
        async fn discover_models(
            &self,
            _: &ProviderContext,
        ) -> Result<Vec<tokenscavenger::discovery::curated::DiscoveredModel>, ProviderError>
        {
            Ok(Vec::new())
        }
        async fn chat_completions(
            &self,
            _: &ProviderContext,
            req: tokenscavenger::api::openai::chat::NormalizedChatRequest,
        ) -> Result<tokenscavenger::api::openai::chat::ProviderChatResponse, ProviderError>
        {
            Ok(tokenscavenger::api::openai::chat::ProviderChatResponse {
                provider_id: self.0.into(),
                model_id: req.model,
                content: Some("ok".into()),
                tool_calls: None,
                finish_reason: Some("stop".into()),
                usage: None,
                latency_ms: 0,
            })
        }
        async fn embeddings(
            &self,
            _: &ProviderContext,
            _: tokenscavenger::api::openai::embeddings::NormalizedEmbeddingsRequest,
        ) -> Result<
            tokenscavenger::api::openai::embeddings::ProviderEmbeddingsResponse,
            ProviderError,
        > {
            Err(ProviderError::UnsupportedFeature("bench".into()))
        }
    }

    pub fn bench(c: &mut Criterion) {
        let rt = Runtime::new().unwrap();

        c.bench_function("route_plan_build_empty", |b| {
            b.to_async(&rt).iter(|| async {
                let registry =
                    Arc::new(tokenscavenger::providers::registry::ProviderRegistry::new());
                let policy = tokenscavenger::router::policy::RoutePolicy::from_config(
                    &tokenscavenger::config::schema::Config::default(),
                );
                let plan = tokenscavenger::router::selection::build_attempt_plan(
                    &policy,
                    &registry,
                    "llama3-70b-8192",
                    tokenscavenger::providers::traits::EndpointKind::ChatCompletions,
                )
                .await;
                black_box(plan);
            });
        });

        c.bench_function("route_plan_large_catalog_1000", |b| {
            b.to_async(&rt).iter(|| async {
                let registry = tokenscavenger::providers::registry::ProviderRegistry::new();
                let mut config = tokenscavenger::config::schema::Config::default();
                for i in 0..1_000 {
                    let id = format!("bench-{i}");
                    config.routing.provider_order.push(id.clone());
                    registry
                        .register(Arc::new(BenchAdapter(Box::leak(id.into_boxed_str()))))
                        .await;
                }
                let policy = tokenscavenger::router::policy::RoutePolicy::from_config(&config);
                let plan = tokenscavenger::router::selection::build_attempt_plan(
                    &policy,
                    &registry,
                    "bench-model",
                    tokenscavenger::providers::traits::EndpointKind::ChatCompletions,
                )
                .await;
                black_box(plan);
            });
        });
    }
}

mod proxy_overhead {
    use super::*;

    pub fn bench(c: &mut Criterion) {
        let rt = Runtime::new().unwrap();
        c.bench_function("direct_provider_baseline_json", |b| {
            b.iter(|| {
                black_box(serde_json::json!({
                    "id": "direct",
                    "choices": [{"message": {"content": "ok"}}]
                }))
            });
        });
        c.bench_function("warm_models_catalog_render", |b| {
            b.to_async(&rt).iter(|| async {
                let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
                sqlx::migrate!("src/db/migrations")
                    .run(&pool)
                    .await
                    .unwrap();
                let state = tokenscavenger::app::state::AppState::new(
                    tokenscavenger::config::schema::Config::default(),
                    pool,
                    Default::default(),
                );
                black_box(tokenscavenger::discovery::merge::build_model_list(&state).await);
            });
        });
    }
}

mod streaming {
    use super::*;

    pub fn bench(c: &mut Criterion) {
        c.bench_function("streaming_first_byte_payload", |b| {
            b.iter(|| {
                let event = tokenscavenger::api::openai::stream::StreamEvent::Chunk {
                    id: "bench".into(),
                    created: 1,
                    model: "bench-model".into(),
                    delta: tokenscavenger::api::openai::chat::StreamDelta {
                        role: Some("assistant".into()),
                        content: Some("o".into()),
                    },
                    finish_reason: None,
                };
                black_box(tokenscavenger::api::openai::stream::format_sse_payload(
                    &event,
                ));
            });
        });
    }
}

// ---------- Circuit Breaker Benchmark ----------

mod breaker {
    use super::*;
    use tokenscavenger::resilience::breaker::CircuitBreaker;
    use tokio::runtime::Runtime;

    pub fn bench(c: &mut Criterion) {
        let rt = Runtime::new().unwrap();

        let mut group = c.benchmark_group("circuit_breaker");
        for num_ops in [100, 1_000, 10_000].iter() {
            group.bench_with_input(
                BenchmarkId::new("allow_request", num_ops),
                num_ops,
                |b, &n| {
                    b.to_async(&rt).iter(|| async {
                        let cb = CircuitBreaker::new(3, 60);
                        for _ in 0..n {
                            black_box(cb.allow_request().await);
                        }
                    });
                },
            );

            group.bench_with_input(
                BenchmarkId::new("record_failure", num_ops),
                num_ops,
                |b, &n| {
                    b.to_async(&rt).iter(|| async {
                        let cb = CircuitBreaker::new(3, 60);
                        for _ in 0..n {
                            cb.record_failure().await;
                        }
                    });
                },
            );
        }
        group.finish();
    }
}

// ---------- Secret Redaction Benchmark ----------

mod redaction {
    use super::*;

    pub fn bench(c: &mut Criterion) {
        let mut group = c.benchmark_group("redaction");
        let input = "sk-this-is-a-very-long-secret-key-that-needs-redaction-abc123def456";
        let json_input = serde_json::json!({
            "api_key": "sk-secret-key-1234567890",
            "base_url": "https://api.example.com",
            "nested": {"api_key": "another-secret-key"}
        });

        group.bench_function("redact_secret", |b| {
            b.iter(|| tokenscavenger::util::redact::redact_secret(black_box(input)));
        });

        group.bench_function("redact_json_value", |b| {
            b.iter(|| {
                tokenscavenger::util::redact::redact_json_value(black_box(json_input.clone()))
            });
        });

        group.finish();
    }
}

// ---------- Config Loading Benchmark ----------

mod config_load {
    use super::*;

    pub fn bench(c: &mut Criterion) {
        let toml_str = r#"
[server]
bind = "0.0.0.0:8000"

[database]
path = "tokenscavenger.db"

[logging]
level = "info"

[routing]
free_first = true

[[providers]]
id = "groq"
enabled = true
api_key = "gsk_test123"

[[providers]]
id = "google"
enabled = true
api_key = "AIza_test123"

[[providers]]
id = "cerebras"
enabled = true
api_key = "csk_test123"
"#;

        c.bench_function("config_parse", |b| {
            b.iter(|| tokenscavenger::config::loader::load_config_from_str(black_box(toml_str)));
        });
    }
}

// ---------- Alias Resolution Benchmark ----------

mod aliases {
    use super::*;

    pub fn bench(c: &mut Criterion) {
        let rt = Runtime::new().unwrap();
        let pool =
            rt.block_on(async { sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap() });
        rt.block_on(async {
            sqlx::migrate!("src/db/migrations")
                .run(&pool)
                .await
                .unwrap();
            sqlx::query("INSERT INTO aliases (alias, target_json) VALUES (?, ?)")
                .bind("test-alias")
                .bind("\"llama3-70b-8192\"")
                .execute(&pool)
                .await
                .unwrap();
        });

        let config = tokenscavenger::config::schema::Config::default();
        let state = tokenscavenger::app::state::AppState::new(config, pool, Default::default());

        c.bench_function("alias_resolve_hit", |b| {
            b.to_async(&rt).iter(|| async {
                let result = tokenscavenger::router::aliases::resolve_alias(
                    black_box(&state),
                    black_box("test-alias"),
                )
                .await;
                black_box(result);
            });
        });

        c.bench_function("alias_resolve_miss", |b| {
            b.to_async(&rt).iter(|| async {
                let result = tokenscavenger::router::aliases::resolve_alias(
                    black_box(&state),
                    black_box("no-such-alias"),
                )
                .await;
                black_box(result);
            });
        });
    }
}

// ---------- SQLite Write Throughput ----------

mod sqlite_write {
    use super::*;

    pub fn bench(c: &mut Criterion) {
        let rt = Runtime::new().unwrap();

        let mut group = c.benchmark_group("sqlite_write");
        for batch_size in [1, 10, 100].iter() {
            group.bench_with_input(BenchmarkId::new("usage_events_insert", batch_size), batch_size, |b, &n| {
                b.to_async(&rt).iter(|| async {
                    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
                    sqlx::migrate!("src/db/migrations").run(&pool).await.unwrap();

                    for i in 0..n {
                        sqlx::query(
                            "INSERT INTO request_log (request_id, endpoint_kind, status) VALUES (?, ?, ?)"
                        )
                        .bind(format!("req-{}", i))
                        .bind("chat")
                        .bind("success")
                        .execute(&pool).await.unwrap();

                        sqlx::query(
                            "INSERT INTO usage_events (request_id, provider_id, model_id, input_tokens, output_tokens, estimated_cost_usd, free_tier) VALUES (?, ?, ?, ?, ?, ?, ?)"
                        )
                        .bind(format!("req-{}", i))
                        .bind("groq")
                        .bind("llama3-70b")
                        .bind(100i64)
                        .bind(50i64)
                        .bind(0.0f64)
                        .bind(true)
                        .execute(&pool).await.unwrap();
                    }

                    black_box(());
                });
            });
        }
        group.finish();
    }
}

// ---------- Health State Computation ----------

mod health {
    use super::*;
    use tokenscavenger::app::state::AppState;
    use tokenscavenger::config::schema::Config;
    use tokio::runtime::Runtime;

    pub fn bench(c: &mut Criterion) {
        let rt = Runtime::new().unwrap();

        c.bench_function("health_record_failure", |b| {
            b.to_async(&rt).iter(|| async {
                let config = Config::default();
                let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
                let state = AppState::new(config, pool, Default::default());
                for _ in 0..100 {
                    tokenscavenger::resilience::health::record_failure(
                        black_box(&state),
                        black_box("test-provider"),
                    )
                    .await;
                }
            });
        });
    }
}

criterion_group!(
    benches,
    route_plan::bench,
    proxy_overhead::bench,
    streaming::bench,
    breaker::bench,
    redaction::bench,
    config_load::bench,
    aliases::bench,
    sqlite_write::bench,
    health::bench,
);

criterion_main!(benches);
