# Changelog

All notable changes to TokenScavenger will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

### Changed

### Fixed

## [0.1.8] - 2026-05-10

### Added

- Added release workflow automation that promotes `[Unreleased]` changelog
  entries into a dated release section and uses that section as the GitHub
  release notes.

### Changed

- Non-positive client `max_tokens` values are now normalized to `1` at the API
  boundary so provider fallbacks never receive invalid negative token limits.
- OpenAI-compatible and Gemini adapters now omit absent token-limit fields
  instead of forwarding JSON `null` values to upstream providers.
- OpenAI-compatible providers now retry once with `max_tokens: 1` when an
  upstream rejects an omitted token limit as zero or missing, while negative
  context-budget errors fall through to the next planned model without a
  doomed same-model retry.
- Graceful shutdown now signals Axum immediately, then drains background tasks
  and closes SQLite after the HTTP server stops.
- HTTP shutdown now has a bounded drain window so open SSE/streaming
  connections cannot prevent the process from completing shutdown.
- Streaming fallback now logs each provider/model attempt as it starts and
  advances to the next planned attempt if no content arrives within a bounded
  pre-content timeout.
- Recent negative context-budget failures are remembered per provider/model and
  prompt size so equal-or-larger prompts skip doomed attempts without blocking
  shorter fresh sessions.
- Recent streaming attempts that time out or end before content are remembered
  per provider/model and prompt size for a short TTL, avoiding repeated silent
  attempts while preserving routing for smaller fresh sessions.
- Recent upstream rate limits are remembered per provider/model for a short
  TTL, using `Retry-After` when available, so repeated requests skip known
  limited attempts instead of hammering the same route.

### Fixed

## [0.1.7] - 2026-05-08

### Added

- Added model-group targets that can mix portable model IDs with
  provider-qualified `{ provider, model }` entries, preserving ordered fallback
  while allowing explicit upstream pinning.
- Added admin UI support for model-group target modes, with separate
  "Any provider" and "Specific provider" selection flows.

### Changed

- Updated route-plan and streaming diagnostics to log full provider/model
  attempt labels, making provider-qualified and cross-provider model-group
  routing easier to audit.
- Updated tool-request routing to preserve operator/model-group order among
  tool-capable attempts instead of reshuffling by provider reliability rank.
- Updated streaming failure handling to surface upstream errors that occur
  before any response content is emitted, including provider rate-limit details.
- Treat upstream token-per-minute/request-size `413 rate_limit_exceeded` errors
  as rate limits so they drive request fallback and preserve the upstream
  diagnostic body.
- Rate-limit and quota errors no longer poison provider health for later
  attempts; they remain per-request fallback signals.
- Updated model-group documentation with both portable model ID and
  provider-qualified target examples.

### Fixed

- Fixed admin "Deploy Configuration" so redacted provider API keys are treated
  as display masks and the existing stored secret is preserved.
- Fixed Google Gemini tool-call requests by translating OpenAI tool definitions
  to Gemini `functionDeclarations` and `toolConfig` instead of forwarding the
  OpenAI `type/function` shape.
- Fixed Google Gemini tool-result continuation requests by translating OpenAI
  `role: "tool"` messages into Gemini `functionResponse` parts.
- Fixed SSE parsing to accept compact `data:{...}` frames as well as
  `data: {...}` frames.
- Fixed route-plan and streaming model-group expansion so provider-qualified
  targets are routed only through their configured provider.

## [0.1.6] - 2026-05-08

### Added

### Changed

- Improved streaming route diagnostics with selected model IDs so fallback
  attempts are explainable when model groups span multiple providers.
- Added tool-aware route reprioritization for chat requests with OpenAI `tools`
  so agentic clients prefer providers/models with more reliable tool-call
  behavior without requiring a separate model group.

### Fixed

- Fixed streaming chat routing so disabled or undiscovered provider/model pairs
  are filtered before upstream calls, matching non-streaming routing behavior.
- Fixed empty streaming attempts being treated as successful completions before
  any content or tool-call delta was forwarded, allowing fallback to continue to
  the next planned attempt.
- Fixed OpenAI-compatible streamed tool calls by forwarding `delta.tool_calls`
  chunks instead of treating tool-call-only streams as empty responses.
- Fixed OpenAI-compatible tool continuation requests by preserving
  `assistant.tool_calls` and `tool.tool_call_id` fields when forwarding message
  history to upstream providers.
- Improved authentication failure logs with method, path, and header-presence
  metadata while continuing to avoid logging API key material.

## [0.1.5] - 2026-05-07

### Added

### Changed

### Fixed

- Fixed model discovery persistence for file-configured providers by seeding
  provider rows before discovery and recording only persisted model counts.
- Fixed provider hot-reload persistence so base URLs and free-only mode are
  stored with provider rows before model discovery refreshes.
- Fixed first-run provider saves so model discovery runs immediately after
  providers are added and the Models page has server-rendered fallback rows.
- Fixed the admin Models page to request `/admin/models` on load instead of
  relying only on model JSON embedded in the page HTML.
- Fixed streaming chat completions so usage chunks are recorded in token usage
  metrics and cost accounting, including requests sent from the Chat Tester.
- Added the running binary version to `/readyz` so setup does not silently apply
  configuration to an older incompatible process.

## [0.1.4] - 2026-05-07

### Added

### Changed

### Fixed

- Improved first-run setup behavior when TokenScavenger is already running by
  hot-reloading the generated config into the live server instead of requiring a
  manual process kill and restart.

## [0.1.3] - 2026-05-07

### Added

### Changed

### Fixed

- Improved first-run startup behavior when the configured bind address is
  already in use, replacing the raw OS error with an actionable message.
- Fixed admin UI access when a master API key is configured by enabling browser
  session auth during setup and adding a `/ui/login` flow.
- Fixed empty admin model catalogs by returning the curated model catalog and
  overlaying discovered database rows instead of relying on DB rows only.

## [0.1.2] - 2026-05-06

### Added

### Changed

- Updated the release workflow to sign and notarize the macOS ARM64 binary
  using Apple Developer ID credentials, distributed as a notarized zip archive.
- Updated README, marketing site, and generated release notes for the notarized
  macOS archive install flow.

### Fixed

## [0.1.1] - 2026-05-06

### Added

### Changed

- Added a sticky marketing site header and a compact tabbed install section for
  macOS, Linux, and Windows release binaries.

### Fixed

- Fixed Windows release builds by gating Unix-only shutdown signal handling
  behind Unix targets.

## [0.1.0] - 2026-05-06

### Added

- OpenAI-compatible API surface for `POST /v1/chat/completions`, streaming chat completions, `POST /v1/embeddings`, and `GET /v1/models`.
- Health, readiness, metrics, admin, and embedded operator UI routes.
- Single-binary Axum/Tokio runtime backed by SQLite with WAL mode, migrations, usage accounting, health events, audit entries, and retention-ready timestamps.
- Provider adapter framework with shared OpenAI-compatible helpers, provider capabilities, auth handling, model discovery, response normalization, rate-limit classification, and streaming support.
- Fourteen built-in provider integrations: Groq, Google Gemini, OpenRouter, Cloudflare Workers AI, Cerebras, NVIDIA NIM, Cohere, Mistral AI, GitHub Models, HuggingFace, ZAI/Zhipu AI, SiliconFlow, DeepSeek, and xAI/Grok.
- Free-first routing with configurable provider order, model groups, model enablement, model priority, health filtering, circuit breakers, retry/backoff behavior, and explicit paid fallback policy.
- OpenAI-style error responses with upstream rate-limit exhaustion surfaced as `429 rate_limit_exceeded` or `quota_exhausted`, and non-rate-limit route exhaustion surfaced as `503 route_exhausted`.
- Runtime config hot reload through the admin API and web UI, including server, routing, resilience, provider, model, and model group changes.
- Runtime overrides persisted to a sidecar `.overrides.toml` file and merged back on startup.
- First-run guided setup wizard and interactive `tokenscavenger config` editor.
- `tokenscavenger service install` and `tokenscavenger service uninstall` commands for macOS LaunchAgent installation/removal and Linux systemd command generation.
- Embedded operator UI with dashboard, providers, models, routing, usage, analytics, health, logs, config, and audit views.
- Dashboard analytics auto-refresh and a provider management flow for adding and testing providers from the UI.
- Prometheus metrics, structured request-path logging, live log streaming, request IDs, and secret redaction in logs/UI/API responses.
- Release workflow for GitHub Actions with Linux, macOS ARM64, and Windows binaries plus SHA256 checksums.
- CI workflow with formatting, clippy, build, tests, integration tests, benchmark smoke, and cross-build checks.
- Documentation set covering getting started, configuration, API behavior, provider matrix, deployment, contributing, roadmap, pull request template, changelog, and the marketing website in `docs/`.
- Price refresh for supported LLM providers and models

### Changed

- Moved long-form project documentation from the marketing `docs/` site into the repository `documentation/` folder.
- Expanded project positioning from free-tier-only routing to free-first routing with explicit opt-in paid fallback.
- Updated provider catalog, setup wizard, README, provider matrix, and marketing site to reflect the direct DeepSeek and xAI/Grok paid fallback integrations.
- Updated route-plan explanations to include resolved model groups, paid fallback eligibility, model enablement, health, breaker state, and filtering reasons.
- Updated usage accounting to distinguish free-tier and paid fallback provider attempts.
- Updated the release workflow to support creating a release from the current `Cargo.toml` version as well as patch/minor/major bumps.
- Updated README and CONTRIBUTING links to point at `documentation/` and `ROADMAP.md`.

### Fixed

- Returned OpenAI-compatible `429` responses when all exhausted routes failed because of upstream rate limits or quota exhaustion.
- Preserved `Retry-After` hints when upstream providers expose reliable rate-limit reset metadata.
- Enforced the invariant that providers marked `free_only = false` are not routed unless `[routing].allow_paid_fallback = true`.
- Prevented disabled providers and disabled models from being selected during routing.
- Improved route exhaustion accounting so failed requests record the appropriate status and HTTP status.
- Fixed the marketing website preview route so `pnpm run preview` serves the site instead of returning `Cannot GET //`.
- Added dashboard analytics auto-refresh so operators do not need to manually reload the UI.
- Added a scroll-to-top control on the marketing website for easier navigation.
- Removed stale public-facing placeholders from contributor documentation.
