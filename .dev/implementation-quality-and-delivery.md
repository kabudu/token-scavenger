# TokenScavenger Quality, Benchmarking, and Delivery

## 1. Quality Bar

The product is only complete when it is robust, fully tested, and benchmarked. That means:

- unit tests for core normalization and routing logic
- integration tests for API behavior and persistence
- provider-adapter fixture tests
- end-to-end tests that exercise streaming and fallback
- fault-injection tests
- repeatable performance benchmarks
- release validation on the supported distribution targets

## 2. Test Strategy

### 2.1 Unit Tests

Cover:

- config parsing and validation
- alias resolution
- provider selection ordering
- retry classification
- circuit breaker transitions
- health score calculation
- request/response normalization
- token and cost accounting
- log redaction helpers

Minimum expectation:

- deterministic tests for each branch of routing and breaker logic
- table-driven tests for provider error classification

### 2.2 Integration Tests

Use `axum` test harness plus ephemeral SQLite databases.

Cover:

- `POST /v1/chat/completions`
- `POST /v1/embeddings`
- `GET /v1/models`
- admin config routes
- metrics endpoint
- readiness behavior with cached and uncached discovery states

Integration tests should use mock provider servers to simulate:

- success
- timeout
- 429 rate limit
- quota exhaustion
- malformed provider payload
- partial streaming failure

### 2.3 Adapter Contract Tests

Every provider adapter must pass a shared contract suite:

- discovery returns normalized model records
- chat response normalizes correctly
- embeddings response normalizes correctly
- usage extraction works
- rate-limit and quota headers parse correctly
- streaming frames are transformed into OpenAI-style SSE

### 2.4 End-to-End Tests

Spin up the full binary against mock providers and run black-box tests through HTTP.

Required E2E scenarios:

- free-tier happy path
- same-provider retry then success
- provider fallback to second free provider
- all free providers exhausted and paid fallback disabled
- all free providers exhausted and paid fallback enabled
- streaming success
- circuit breaker opens after repeated failures
- circuit breaker half-open recovery
- manual discovery refresh updates `/v1/models`
- UI renders core pages and admin actions persist changes

### 2.5 UI Tests

Use browser automation for smoke coverage of the embedded UI.

Cover:

- dashboard loads
- provider reorder persists
- provider test connection action works against mock endpoint
- model enable/disable persists
- alias edit/save works
- analytics and logs pages render with seeded data

### 2.6 Migration Tests

Migration coverage must include:

- empty database bootstrap
- forward migration from previous schema versions
- rollback safety at least in test environments where possible
- compatibility of persisted config and catalog records

## 3. Benchmark Plan

Benchmarks must be checked into the repo and executable by another engineer.

### 3.1 Core Performance Benchmarks

Measure:

- non-streaming chat proxy overhead versus direct provider call
- streaming time-to-first-byte overhead
- warm-cache `GET /v1/models` latency
- route-planning latency under large catalog sizes
- SQLite write throughput for usage events

### 3.2 Load Tests

Run load tests with configurable concurrency against mock providers.

Scenarios:

- 50 concurrent non-streaming requests
- 200 concurrent non-streaming requests
- 100 concurrent streaming requests
- mixed chat and embeddings workload
- discovery refresh while serving live traffic

Collect:

- throughput
- p50/p95/p99 latency
- error rate
- CPU and memory
- database write lag
- breaker transition counts

### 3.3 Failure Benchmarks

Measure behavior under stress and partial outage:

- one provider timing out consistently
- multiple providers returning 429
- discovery endpoint slowness
- SQLite fsync pressure

Success condition:

- service continues routing where possible
- median latency degrades gracefully rather than catastrophically

### 3.4 Benchmark Tooling

Suggested approach:

- `criterion` for microbenchmarks
- a repo-local load test harness in Rust for repeatability
- optional `k6` or `vegeta` scripts if the team wants external validation

Benchmark outputs should be stored as:

- machine-readable JSON
- human-readable markdown summary

## 4. Observability Validation

Test and benchmark runs must assert that:

- Prometheus metrics are emitted with expected labels
- request IDs correlate between logs and persisted request rows
- UI usage charts reflect inserted usage events
- circuit breaker state appears in metrics and UI

## 5. Security Validation

Required checks:

- secrets never appear in logs
- secrets are masked in UI and config APIs
- unauthenticated protected routes are rejected when auth is enabled
- rate limiting works when configured
- invalid config updates are rejected with audit entries
- CORS configuration behaves as expected

## 6. Release Engineering

### 6.1 Build Targets

Required build targets:

- Linux `x86_64`
- Linux `aarch64`
- macOS Apple Silicon
- macOS Intel if still needed by the deployment target
- Windows `x86_64`

### 6.2 Release Artifacts

Publish:

- static binaries where feasible
- checksums
- example config file
- migration notes
- changelog
- optional minimal container image

### 6.3 CI Pipeline

The CI pipeline should include:

- formatting
- clippy/lint
- unit tests
- integration tests
- adapter contract suite
- UI smoke tests
- benchmark smoke subset
- cross-platform build matrix

### 6.4 Pre-Release Checklist

Before shipping any release:

- run full automated test suite
- run benchmark suite and compare against baseline
- verify startup on clean machine with example config
- verify provider discovery on the baked-in provider set where credentials are available
- verify embedded UI assets load from the binary
- verify database migration from previous release
- verify log redaction manually on representative scenarios

## 7. Documentation Deliverables

The code implementation should be accompanied by:

- operator deployment guide
- config reference
- provider support matrix
- troubleshooting guide
- benchmark results summary
- API compatibility notes

These can be separate docs, but they should be derived from the implementation package rather than diverging from it.

## 8. Acceptance Criteria

The implementation is ready to hand off or release when:

- all required docs in this package are reflected in code
- tests cover happy path, fallback path, and failure path behavior
- benchmarks are executed and recorded
- release artifacts build reproducibly
- operational controls in UI and metrics are sufficient for production debugging
- no unresolved gap remains against the original spec without being explicitly marked as future-scope per the roadmap

## 9. Quality And Delivery Checklist

Mark each item as evidence exists in code, CI, docs, or recorded benchmark output.

- [x] Add unit tests for config validation, alias resolution, routing order, retry classification, breakers, health scoring, normalization, accounting, and redaction.
- [x] Add integration tests using Axum harnesses and ephemeral SQLite databases.
- [x] Add mock provider servers for success, timeout, 429, quota exhaustion, malformed payload, and partial streaming failure.
- [x] Add shared adapter contract tests and fixtures for every provider adapter.
- [x] Add end-to-end tests for free-tier routing, retries, fallback, paid fallback policy, streaming, breakers, discovery refresh, and core UI actions.
- [x] Add migration tests for clean bootstrap and forward upgrades.
- [x] Add UI smoke tests for dashboard, provider reorder, provider test, model enablement, alias save, analytics, and logs.
- [x] Produce release artifacts, checksums, example config, migration notes, changelog, and optional container image.
- [x] Record load and failure benchmark results in machine-readable and markdown formats.
- [x] Assert expected metrics, logs, request IDs, health states, and UI analytics in validation tests.
- [x] Verify secrets are redacted and protected routes reject unauthenticated requests when auth is enabled.
- [x] Add CI jobs for formatting, linting, tests, adapter contracts, UI smoke tests, benchmark smoke subset, and cross-platform builds.
- [x] Produce release artifacts, checksums, example config, migration notes, changelog, and optional container image.
- [x] Complete operator deployment, config reference, provider matrix, troubleshooting, benchmark summary, and API compatibility documentation.
