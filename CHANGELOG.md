# Changelog

All notable changes to TokenScavenger will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

### Changed

### Fixed

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
