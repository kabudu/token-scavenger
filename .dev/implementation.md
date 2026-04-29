# TokenScavenger Implementation Plan

This document expands the original high-level specification into a build-ready implementation package for a seasoned engineer. It preserves the stated product requirements and decomposes them into concrete architecture, delivery, testing, and benchmarking guidance.

Use this file as the entrypoint. The detailed design is split across companion documents in `.dev/`:

- [implementation-architecture.md](/Users/kabudu/projex/token-scavenger/.dev/implementation-architecture.md)
- [implementation-providers-and-routing.md](/Users/kabudu/projex/token-scavenger/.dev/implementation-providers-and-routing.md)
- [implementation-api-ui-and-data.md](/Users/kabudu/projex/token-scavenger/.dev/implementation-api-ui-and-data.md)
- [implementation-quality-and-delivery.md](/Users/kabudu/projex/token-scavenger/.dev/implementation-quality-and-delivery.md)

## 1. Product Definition

### 1.1 Purpose
TokenScavenger is a lightweight, self-hosted, single-binary LLM proxy/router that prioritizes permanent free-tier inference providers and automatically falls back to paid providers when free quota or health conditions require it.

It must expose an OpenAI-compatible HTTP API so existing OpenAI SDK clients can switch to TokenScavenger by changing only the `base_url` and, optionally, the API key.

### 1.2 Core Goals
- Single static binary for core usage. No Python, Node, or Docker required for basic runtime.
- Full operator control over providers, models, provider ordering, model aliases, and fallback chains.
- Automatic provider model discovery plus a curated built-in model/provider catalog.
- Strong fault tolerance: retries, circuit breakers, health monitoring, graceful degradation.
- Strong observability: structured logs, Prometheus metrics, usage persistence, latency and error tracking.
- Built-in web UI for configuration, analytics, charts, and live operational visibility.
- Suitable for production use by PubTrackr and similar systems that depend on an OpenAI-compatible upstream.

### 1.3 Non-Goals
- No first-party model serving or local inference engine.
- No managed SaaS control plane.
- No requirement to support every OpenAI endpoint on day one if the documented MVP endpoint set is satisfied first.

## 2. Binding Requirements

The following are considered immutable unless the product owner explicitly changes them:

- Runtime implementation language: Rust.
- HTTP server stack: Axum + Tokio.
- Embedded persistence: SQLite.
- OpenAI-compatible interface including chat completions and embeddings, with streaming support for chat completions.
- Single-binary deployment as the primary operating mode.
- Built-in web UI served from the same binary.
- Provider scavenging strategy that prefers free tiers first and falls back when quotas are exhausted or providers are unhealthy.
- Automatic model discovery from provider model-list endpoints, merged with a curated built-in provider/model catalog.
- Observability surface including Prometheus metrics, structured logs, and persisted usage data.
- Fault-tolerance mechanisms including retries, circuit breakers, provider health checks, and graceful shutdown.
- Default product positioning must include the baked-in free-provider matrix from the original specification.

## 3. Required Endpoint Surface

The implementation must support these endpoint groups:

- `POST /v1/chat/completions`
- `POST /v1/embeddings`
- `GET /v1/models`
- `GET /healthz`
- `GET /readyz`
- `GET /metrics`
- `GET /ui`

The implementation may add internal/admin routes, but they must be namespaced and secured appropriately. The detailed route map is defined in [implementation-api-ui-and-data.md](/Users/kabudu/projex/token-scavenger/.dev/implementation-api-ui-and-data.md).

## 4. Product Scope by Phase

### 4.1 MVP
- OpenAI-compatible proxy for chat completions and embeddings.
- Streaming SSE for chat completions.
- SQLite-backed config and usage persistence.
- Config file loading from `tokenscavenger.toml` plus environment-variable secret expansion.
- Provider adapters for the initial free-tier set needed to satisfy the documented default provider matrix strategy.
- Provider discovery and cached model catalog.
- Prometheus metrics and structured logs.
- Basic but production-usable web UI for config, health, usage, and logs.

### 4.2 Phase 1
- Guided model store with richer discovery metadata.
- Drag-and-drop provider/model ordering in the UI.
- Usage-based cost estimation and free-versus-paid dashboards.
- Full audit log of config changes.
- Rate limiting and expanded auth controls.

### 4.3 Phase 2
- Richer charts and interactive dashboards.
- Advanced custom aliases and policy-based routing.
- PubTrackr-focused example integration and deployment recipe.

## 5. Source-of-Truth Documents

Each companion document has a strict scope:

- [implementation-architecture.md](/Users/kabudu/projex/token-scavenger/.dev/implementation-architecture.md): runtime topology, module design, concurrency model, resilience, and deployment architecture.
- [implementation-providers-and-routing.md](/Users/kabudu/projex/token-scavenger/.dev/implementation-providers-and-routing.md): provider abstraction, model discovery, routing policy, fallback logic, health evaluation, and baked-in provider catalog requirements.
- [implementation-api-ui-and-data.md](/Users/kabudu/projex/token-scavenger/.dev/implementation-api-ui-and-data.md): HTTP contract, request/response normalization, database schema, config schema, and UI requirements.
- [implementation-quality-and-delivery.md](/Users/kabudu/projex/token-scavenger/.dev/implementation-quality-and-delivery.md): test strategy, benchmark plan, security controls, release packaging, operational readiness, and acceptance criteria.

## 6. Engineering Principles

The engineer implementing this system must follow these guardrails:

- Preserve OpenAI API compatibility over internal convenience.
- Normalize provider differences at the adapter boundary, not in route handlers.
- Prefer explicit policy configuration over hidden heuristics.
- Keep provider failures isolated and observable.
- Make discovery, routing, and fallback decisions explainable in logs and UI.
- Treat quotas, token accounting, and cost estimation as approximate unless provider data is authoritative; surface confidence level when needed.
- Maintain a clean separation between config state, runtime state, and historical analytics state.
- Keep the binary self-contained: static assets embedded, database file local, zero external control-plane dependency.

## 7. Delivery Expectations

The implementation is not complete until all of the following are true:

- The system builds into a single runnable binary for the target platforms.
- The documented endpoint surface works against real or simulated providers.
- Streaming behavior, fallback behavior, and token accounting are covered by automated tests.
- Benchmarks are executed and recorded using the plan in [implementation-quality-and-delivery.md](/Users/kabudu/projex/token-scavenger/.dev/implementation-quality-and-delivery.md).
- The built-in provider catalog and discovery logic are exercised in integration tests.
- The web UI provides the configuration and operational visibility promised by the spec.
- A release artifact and startup flow are documented and reproducible.

## 8. Implementation Order

Recommended build order:

1. Core configuration loading, runtime state container, and database migrations.
2. OpenAI-compatible route skeleton and response passthrough harness.
3. Provider adapter trait, HTTP client wrapper, and one fully working provider adapter.
4. Routing engine, health model, retries, and circuit breaker behavior.
5. Model discovery, catalog persistence, and `GET /v1/models`.
6. Usage persistence, Prometheus metrics, and structured tracing.
7. Embeddings support and full chat streaming support.
8. Web UI with configuration, model store, health, and metrics views.
9. Security hardening, benchmark execution, packaging, and release validation.

## 9. Change Control

Any implementation choice that appears to conflict with the original spec should be resolved in favor of the original requirement set, even if that means extra engineering effort. The only permitted deviations are:

- Filling in unspecified technical details necessary to implement the system.
- Sequencing work into phases while preserving the promised end-state.
- Explicitly marking future-only features when they were already described as future in the source spec.

If new requirements emerge during build, this implementation package should be updated rather than allowing code and docs to drift apart.

## 10. Implementation Checklist

Mark these items as work lands. Keep this checklist synchronized with the companion document checklists.

- [x] Scaffold the Rust project and confirm the primary binary starts.
- [x] Implement config loading, validation, and environment secret expansion.
- [x] Initialize SQLite, migrations, and core persistence tables.
- [x] Add OpenAI-compatible route skeletons for chat, embeddings, and models.
- [x] Implement provider adapter abstraction and at least one complete provider.
- [x] Implement routing, aliases, retries, health scoring, and circuit breakers.
- [x] Add model discovery, curated catalog merge, and cached `/v1/models`.
- [x] Add usage accounting, structured logs, Prometheus metrics, and redaction.
- [x] Implement streaming chat completions with OpenAI-style SSE.
- [x] Build the embedded operator UI for core configuration and diagnostics.
- [x] Add tests for routing, fallback, streaming, persistence, and token accounting.
- [x] Run benchmarks and record results.
- [x] Document startup, release artifacts, and operational handoff.
