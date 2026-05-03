# TokenScavenger Roadmap

TokenScavenger already solves a very practical problem: one local OpenAI-compatible gateway that makes free-tier and explicitly approved paid fallback routing observable and controllable. The next wave should turn it from a useful router into an operator-grade LLM traffic control plane.

This roadmap is intentionally focused on five high-value enhancements. Each one preserves the project invariants: self-hosted first, single-binary runtime, OpenAI-compatible behavior, explicit paid-provider policy, redacted secrets, and clear routing explanations.

## 1. Policy Engine For Cost, Latency, And Quality

**Current baseline:** TokenScavenger already has provider ordering, model enablement, free-first routing, explicit paid fallback gating, retries, health checks, circuit breakers, usage accounting, and estimated cost surfaces.

**Why it matters:** The next step is to make those primitives policy-driven. Operators should be able to route each request by intent: cheapest acceptable model, fastest healthy route, best quality route, privacy-preferred route, or hard budget cap.

**What good looks like:**

- Per-alias routing objectives such as `min_cost`, `min_latency`, `balanced`, `quality_first`, and `local_only`.
- Configurable max cost per request, per day, per provider, and per alias.
- Provider/model scoring that combines price, observed latency, recent failure rate, context window, endpoint capability, and operator priority.
- Route-plan explanations that score each candidate and show why one model won over alternatives.
- Deterministic tests for budget enforcement, paid fallback gating, and policy tie-breaking.

## 2. Rich Provider Marketplace And Plugin SDK

**Current baseline:** The repo already has a shared provider adapter trait, OpenAI-compatible helpers, provider registry initialization, curated catalog data, provider matrix documentation, and contract tests for the built-in provider set.

**Why it matters:** TokenScavenger becomes much more valuable if contributors can add providers with less bespoke code and less reviewer guesswork. A provider marketplace would make the project feel alive while keeping runtime operation simple.

**What good looks like:**

- A documented provider adapter SDK that packages the existing trait, shared helpers, fixtures, contract tests, and compatibility checklist into a contributor-facing workflow.
- Provider metadata manifests for capabilities, model naming rules, pricing hints, docs URLs, free-tier terms, and known quirks.
- Optional community provider bundles that can be compiled in by feature flag.
- Admin UI pages for provider capability inspection, discovery diagnostics, and adapter version details.
- A contributor workflow that makes adding a provider mostly: adapter, manifest, fixtures, docs, tests.

## 3. Model Intelligence Layer

**Current baseline:** TokenScavenger already merges curated and discovered model catalogs, supports aliases, exposes model enablement and priority controls, and documents provider capabilities. Alias editing exists in the admin UI.

**Why it matters:** Operators should not have to memorize every upstream model name or manually guess equivalent fallbacks. TokenScavenger can become the map between task intent and concrete provider models.

**What good looks like:**

- Normalized model families, task tags, context windows, modality flags, JSON/tool support, reasoning support, and pricing metadata.
- Higher-level smart aliases such as `fast:chat`, `cheap:code`, `reasoning:deep`, and `vision:balanced` built on top of the existing alias system.
- Compatibility checks that reject or reroute when a provider cannot satisfy tools, JSON mode, vision, embeddings, or context length.
- Automatic catalog freshness scoring so stale discovery data is visible.
- Admin UI flows for comparing models across providers and editing advanced alias strategies without hand-writing JSON.

## 4. Operator-Grade Observability And Incident Workflow

**Current baseline:** TokenScavenger already exposes Prometheus metrics, structured logs, usage views, health views, audit history, route-plan explanations, live log streaming, and documented 429/503 response behavior.

**Why it matters:** A router is only as useful as its explanations. When an app sees slow requests, 429s, or degraded quality, TokenScavenger should make the cause obvious in seconds.

**What good looks like:**

- A request trace view that shows alias resolution, candidate providers, skip reasons, retries, selected route, upstream response class, latency, and token usage.
- Deeper time-window analytics for success rate, 429 rate, cost estimate, token volume, fallback count, and provider saturation.
- Incident annotations for provider outages, quota exhaustion, config changes, and breaker transitions.
- Exportable diagnostic bundles with secrets redacted.
- Ready-to-import Grafana dashboards and alerting rules built on the existing Prometheus metrics.

## 5. Deployment, Security, And Team Controls

**Current baseline:** The project already documents static binary builds, Docker, Docker Compose, systemd, reverse proxy deployment, Prometheus scraping, health checks, SQLite backup, and upgrade steps in `documentation/deployment.md`. It also supports bearer auth, CORS controls, query-key opt-in, session-cookie UI auth, secret redaction, and release artifacts with checksums.

**Why it matters:** To be an enterprise self-hosted gateway, TokenScavenger should deepen security and team workflows without becoming a managed platform or weakening the single-binary deployment path.

**What good looks like:**

- Role-aware admin access for read-only operators, config editors, and credential managers.
- Optional encrypted provider credential storage using OS keychains or an operator-supplied encryption key.
- Cryptographically signed release artifacts, SBOM generation, and documented verification steps beyond the existing checksum release flow.
- Homebrew packaging and Kubernetes manifests that complement the existing binary, Docker, Docker Compose, and systemd deployment docs.
- Restore drills, retention policy controls, and migration rollback guidance for SQLite state, extending the current backup and automatic migration docs.
- Optional external identity integration for the admin UI, such as OIDC reverse-proxy headers, while keeping local auth simple.

## How To Contribute

Roadmap work should start with a small design issue or draft PR that explains the operator problem, config/API impact, UI impact, and tests. Large features should land in narrow slices that keep the current router reliable after every merge.

Before opening a PR, read:

- [CONTRIBUTING.md](CONTRIBUTING.md)
- [AGENTS.md](AGENTS.md)
- [documentation/configuration.md](documentation/configuration.md)
- [documentation/provider-matrix.md](documentation/provider-matrix.md)
