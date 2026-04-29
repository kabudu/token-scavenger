# TokenScavenger Implementation Architecture

## 1. Runtime Topology

The runtime must be a single Rust binary composed of these subsystems:

1. HTTP server.
2. OpenAI-compatible API layer.
3. Routing engine.
4. Provider adapter layer.
5. Model discovery and catalog service.
6. Config and policy service.
7. Usage accounting and analytics writer.
8. Health monitoring and resilience service.
9. Metrics, tracing, and UI service.
10. Embedded SQLite persistence.

All subsystems run in-process. There is no sidecar, worker process, or external queue.

## 2. Crate and Module Layout

Use a single cargo workspace if helpful, but a single crate is acceptable for MVP. The code layout should still be modular:

```text
src/
  main.rs
  app/
    mod.rs
    state.rs
    startup.rs
    shutdown.rs
  config/
    mod.rs
    schema.rs
    loader.rs
    env.rs
    validation.rs
  api/
    mod.rs
    routes.rs
    auth.rs
    error.rs
    openai/
      mod.rs
      chat.rs
      embeddings.rs
      models.rs
      types.rs
      stream.rs
  router/
    mod.rs
    engine.rs
    policy.rs
    selection.rs
    fallback.rs
    aliases.rs
  providers/
    mod.rs
    traits.rs
    registry.rs
    http.rs
    normalization.rs
    groq.rs
    google.rs
    openrouter.rs
    cloudflare.rs
    cerebras.rs
    nvidia.rs
    cohere.rs
    mistral.rs
    github_models.rs
    huggingface.rs
    zai.rs
    siliconflow.rs
  discovery/
    mod.rs
    refresh.rs
    curated.rs
    merge.rs
  resilience/
    mod.rs
    breaker.rs
    retry.rs
    health.rs
    rate_limits.rs
  usage/
    mod.rs
    accounting.rs
    pricing.rs
    aggregation.rs
  db/
    mod.rs
    models.rs
    migrations/
  metrics/
    mod.rs
    prometheus.rs
    tracing.rs
  ui/
    mod.rs
    routes.rs
    templates.rs
    assets.rs
  util/
    mod.rs
    time.rs
    redact.rs
```

## 3. Core State Model

Define a single `AppState` shared through `Arc` containing:

- Immutable boot configuration after validation.
- Hot-reloadable effective runtime config in `ArcSwap` or `RwLock`.
- SQLite connection pool.
- Shared `reqwest::Client`.
- Provider registry.
- Routing engine.
- Model catalog cache.
- Provider health state map.
- Circuit breaker map.
- Usage aggregator channels.
- Metrics registry handles.
- UI/event broadcast channels for live views.

Avoid storing large mutable maps directly behind coarse-grained locks if they are read on the request path. Favor:

- `DashMap` for concurrent per-provider state.
- `tokio::sync::watch` for config snapshots.
- `tokio::sync::broadcast` for live log/health/UI streams.
- `moka` or equivalent in-memory cache for model catalog and lookup acceleration.

## 4. Concurrency and Background Jobs

Background services should be explicit tasks started during app bootstrap:

- Provider discovery refresh loop.
- Provider health probe loop.
- Usage aggregation flush loop.
- Circuit breaker decay/reset loop.
- Optional stale-model cleanup loop.
- Optional configuration backup loop.

Each task must:

- Have a name in logs.
- Be cancellable via shutdown token.
- Use bounded channels.
- Expose heartbeat or last-success timestamp for diagnostics.

## 5. Request Lifecycle

For `POST /v1/chat/completions`:

1. Parse OpenAI-compatible request into normalized internal request.
2. Authenticate caller if API key enforcement is enabled.
3. Resolve requested model or alias.
4. Build candidate route plan from policy engine.
5. Filter providers by enablement, feature support, health, breaker state, and quota hints.
6. Attempt providers in priority order with retry and fallback policy.
7. Stream or return the normalized provider response.
8. Persist usage, latency, and decision metadata asynchronously.
9. Emit metrics and structured logs regardless of success or failure.

For `POST /v1/embeddings`, the flow is the same minus chat streaming and tool-call normalization concerns.

## 6. OpenAI Compatibility Strategy

Compatibility must be enforced by a normalization layer, not by directly relaying arbitrary JSON. The internal types should represent the supported subset plus safe passthrough fields.

Design principle:

- Parse the user request into typed internal structs.
- Preserve unknown optional fields in an `extra` map where safe.
- Let providers declare unsupported fields.
- Strip or reject fields only when a provider truly cannot support them.
- Normalize provider output back into OpenAI-shaped responses.

This is necessary because several free providers expose "OpenAI-compatible" endpoints with subtle schema differences.

## 7. Failure Domain Design

Failure isolation rules:

- One provider's failures must not poison the entire request path.
- Discovery failures must not block serving traffic if a cached catalog exists.
- SQLite write lag must not block upstream response completion.
- UI failures must not affect API serving.
- Metrics export failures must degrade silently and log warnings.

Implement bounded retries with policy-aware classification:

- Retry network timeouts, transient 5xx, and provider throttling when allowed.
- Do not retry malformed requests, auth failures caused by local config, or provider-declared unsupported parameter errors unless the fallback engine can try a different provider.

## 8. Resilience Architecture

Circuit breaker requirements:

- Per provider and optionally per provider-model pair.
- Closed -> Open after configurable consecutive failures or rolling error-rate threshold.
- Half-open after cooldown.
- Small configurable trial count in half-open.
- State transitions logged and exposed in metrics/UI.

Health model requirements:

- Active probe health and passive request-path health both contribute.
- Health score should consider:
  - recent success rate
  - recent latency percentile
  - most recent auth/quota/rate-limit result
  - breaker state
  - last successful discovery timestamp

The routing engine should use a weighted health-aware priority, while still respecting explicit operator ordering.

## 9. Persistence Architecture

SQLite is the only required durable store. Split tables into four domains:

- Configuration and audit.
- Provider/model catalog.
- Request/usage analytics.
- Runtime snapshots and health events.

SQLite requirements:

- WAL mode enabled.
- Busy timeout configured.
- Migrations managed through `sqlx`.
- Indexes for dashboard queries and retention cleanup.
- Async writes batched where possible to reduce request-path latency.

Large request/response payload bodies should not be stored by default. If optional diagnostic sampling is added, it must be redacted and size-limited.

## 10. Static Asset Embedding

UI assets must be embedded into the binary using `rust-embed` or `include_dir!`.

Build-time workflow:

- Frontend assets built before release build.
- Minified CSS/JS embedded into Rust binary.
- Asset fingerprinting generated for cache-busting.

Runtime behavior:

- Serve embedded assets with ETag and cache headers.
- No runtime dependency on Node for serving.

## 11. Startup Sequence

Startup order must be deterministic:

1. Parse CLI args.
2. Load config file and environment overlays.
3. Validate config and fail fast on unsafe or incomplete configuration.
4. Initialize tracing and metrics.
5. Open SQLite and run migrations.
6. Build HTTP client and provider registry.
7. Load model catalog cache from SQLite.
8. Start background tasks.
9. Perform initial provider discovery in parallel with timeout budget.
10. Bind HTTP listener only after readiness prerequisites are satisfied.

Readiness prerequisites:

- Database ready.
- Config valid.
- At least one provider configured.
- Provider registry initialized.

Provider discovery may be incomplete at readiness time if cached catalog exists; that should still count as ready.

## 12. Shutdown Sequence

Graceful shutdown must:

1. Stop accepting new connections.
2. Allow in-flight streaming requests to finish within a grace period.
3. Cancel background tasks.
4. Flush usage and audit buffers.
5. Close database pool cleanly.

Expose a configurable shutdown timeout and log any incomplete task teardown.

## 13. Performance Targets

These targets guide implementation and are verified in the benchmark plan:

- Proxy overhead for non-streaming chat requests: median under 5 ms versus direct provider call in local benchmarks.
- Streaming first-byte overhead: median under 20 ms beyond provider first token in local benchmarks.
- UI dashboard page load on local network: under 500 ms server-side response time for default views with a moderately sized SQLite file.
- `GET /v1/models` response from warm cache: under 50 ms median.
- Support sustained concurrency of at least 200 simultaneous proxied requests on a typical 4 vCPU deployment without internal queue collapse.

## 14. Packaging and Distribution

Primary artifact:

- Static binary, preferably `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl` for Linux.

Secondary artifacts:

- macOS binaries.
- Windows binary.
- Minimal Docker image based on `scratch` or distroless.

The architecture must not require platform-specific runtime features outside the documented targets.

## 15. Architecture Checklist

Mark each item as the corresponding implementation is completed and verified.

- [x] Scaffold the Rust binary and module layout with clear subsystem boundaries.
- [x] Define `AppState` with validated config, database pool, provider registry, router, health state, caches, metrics, and UI channels.
- [x] Implement deterministic startup with config loading, tracing, SQLite migrations, provider registry initialization, catalog cache loading, background tasks, discovery, and listener binding.
- [x] Implement graceful shutdown with connection draining, cancellable tasks, buffer flushing, and database pool closure.
- [x] Add background jobs for discovery refresh, health probes, usage flushing, circuit breaker reset, and retention cleanup.
- [x] Use bounded channels and avoid coarse locks on hot request paths.
- [x] Configure SQLite WAL mode, busy timeout, migrations, indexes, and batched async writes.
- [x] Embed UI assets into the release binary with cache headers.
- [x] Expose readiness based on database, config, provider registry, and provider availability prerequisites.
- [x] Validate architecture performance targets with benchmarks.
- [ ] Package reproducible binary artifacts for supported platforms.
