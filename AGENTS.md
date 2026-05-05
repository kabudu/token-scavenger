# Agent Guidelines

## Project Shape

TokenScavenger is planned as a lightweight, self-hosted Rust LLM proxy/router. The core runtime should be a single binary using Axum, Tokio, SQLite, embedded UI assets, structured tracing, Prometheus metrics, and OpenAI-compatible HTTP endpoints.

The detailed implementation plan lives in `.dev/`. Treat `.dev/implementation.md` as the entry point and consult the focused companion files before making architectural changes.

Each `.dev/implementation*.md` document contains a task checklist. As you implement features, mark the relevant checklist items in the same change so the docs remain a live progress tracker.

## Product Invariants

- Preserve OpenAI API compatibility for supported endpoints:
  - `POST /v1/chat/completions`
  - `POST /v1/embeddings`
  - `GET /v1/models`
  - `GET /healthz`, `GET /readyz`, `GET /metrics`, `GET /ui`
- Keep the primary deployment path as a self-contained binary. Do not introduce required Python, Node, Docker, external queues, or managed control planes for runtime operation.
- Normalize provider differences at the adapter boundary, not in route handlers.
- Prefer free-tier providers first. Never route to paid providers unless policy explicitly allows it.
- Keep config state, runtime health state, model catalog state, and historical usage data separate.
- Secrets must be redacted in logs, UI, API responses, fixtures, and test output.

## Expected Rust Layout

Use the module boundaries described in `.dev/implementation-architecture.md`: `api`, `app`, `config`, `db`, `discovery`, `metrics`, `providers`, `resilience`, `router`, `ui`, `usage`, and `util`.

If starting the codebase from scratch, a single crate is acceptable for MVP, but keep these boundaries clear from the beginning.

## Provider And Routing Rules

- Each upstream belongs behind a `ProviderAdapter`-style abstraction with discovery, request translation, response normalization, auth, and quota/rate-limit parsing.
- Provider adapters should not know global routing order or fallback policy.
- The routing engine should produce an explainable attempt plan using model groups, operator priority, health, breaker state, quota hints, and endpoint capabilities.
- Mid-stream fallback for chat completions is not allowed after bytes have been sent to the caller.
- `GET /v1/models` should return the merged public catalog, not raw provider payloads.

## Data And UI Expectations

- SQLite is the only required durable store. Use migrations, WAL mode, busy timeouts, and indexes for dashboard/retention queries.
- Avoid storing full request/response bodies by default. Any diagnostic sampling must be redacted and size-limited.
- The UI is operational, not decorative: providers, models, model groups, usage, health, logs, config, and audit views should help an operator understand and change routing behavior.
- UI assets should be embedded into the binary for release builds.

## Quality Bar

Add tests in proportion to the risk of the change. Core routing, normalization, retry classification, circuit breakers, config validation, usage accounting, and secret redaction should have deterministic tests.

Use mock provider servers or fixtures for integration and adapter tests. Do not require real provider credentials for ordinary CI.

Before considering a substantial change complete, run the relevant checks, typically:

```sh
cargo fmt --all
cargo clippy --all-targets --all-features
cargo test --all-features
```

If those commands are not yet available because the Rust project has not been scaffolded, say so clearly in the handoff.

## Change Discipline

- Keep `README.md`, `AGENTS.md`, and `.dev/` aligned when behavior or architecture changes.
- Update the relevant `.dev/implementation*.md` checkboxes when work starts, lands, or is intentionally deferred.
- Keep generated build artifacts, local databases, local configs, and secrets out of git.
- Avoid broad rewrites unless they directly support the current task.
- Favor explicit configuration and observable decisions over hidden heuristics.
- Preserve the MIT license header and project naming conventions.
