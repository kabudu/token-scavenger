# API Behavior

TokenScavenger exposes OpenAI-compatible HTTP endpoints while routing each request through one or more upstream providers. The response body follows the OpenAI error shape, but the HTTP status code is chosen to help downstream clients decide whether to retry immediately, back off, fix the request, or treat the proxy as unavailable.

## Supported Endpoints

| Endpoint | Description |
|----------|-------------|
| `POST /v1/chat/completions` | OpenAI-compatible chat completions, including streaming SSE when `stream: true`. |
| `POST /v1/embeddings` | OpenAI-compatible embeddings for providers that support embeddings. |
| `GET /v1/models` | Merged public model catalog from curated models, discovery, operator configuration, and model intelligence metadata. |
| `GET /healthz` | Lightweight process health check. |
| `GET /readyz` | Readiness check with dependency state. |
| `GET /metrics` | Prometheus metrics. |
| `GET /ui` | Embedded operator dashboard, when enabled. |
| `GET /admin/observability/summary` | Time-window success, 429, fallback, token, cost, and provider saturation summary. |
| `GET /admin/request-traces` | Recent request trace summaries. |
| `GET /admin/request-traces/{request_id}` | Detailed route timeline, usage, and request outcome for one request. |
| `GET /admin/incidents` | Incident feed derived from provider health events, route failures, and config audit history. |
| `GET /admin/diagnostics/bundle` | Redacted diagnostic bundle for support and incident handoff. |
| `GET /admin/whoami` | Current authenticated admin principal, auth source, role, and credential-management permission. |
| `GET /admin/projects` | List project-scoped client applications, redacted key metadata, and budgets. |
| `POST /admin/projects` | Create a project for app/team/environment-scoped client access. |
| `PUT /admin/projects/{project_id}` | Update project policy, owner metadata, budgets, provider restrictions, and webhooks. |
| `DELETE /admin/projects/{project_id}` | Disable a project and revoke its keys. |
| `POST /admin/projects/{project_id}/keys` | Issue a show-once OpenAI-compatible project API key. |
| `DELETE /admin/projects/{project_id}/keys/{key_prefix}` | Revoke one project API key by prefix. |
| `GET /admin/projects/{project_id}/usage` | Project-scoped usage summary and recent request rows. |
| `GET /admin/projects/{project_id}/export.csv` | CSV export for project-scoped usage and request attribution. |
| `GET /admin/projects/{project_id}/diagnostics/bundle` | Redacted project-scoped diagnostic bundle. |
| `GET /admin/update/check` | Self-update status for the configured GitHub release source. Network failures return `200 OK` with `update_available = false` and `check_error`. |
| `POST /admin/update/apply` | Download, checksum-verify, install, and restart onto the latest release when self-update is enabled. |

## Error Shape

Errors use this OpenAI-style JSON envelope:

```json
{
  "error": {
    "message": "All providers failed. Last error: Rate limited: retry after Some(7)",
    "type": "rate_limit_error",
    "param": null,
    "code": "rate_limit_exceeded"
  }
}
```

The `message` is intended for logs and operator diagnostics. Client retry logic should key primarily off the HTTP status and `error.code`.

## Status Codes

| Status | Code | Meaning | Client behavior |
|--------|------|---------|-----------------|
| `200` | n/a | TokenScavenger successfully served the request, possibly after internal fallback. | Use the response normally. |
| `400` | `invalid_request` or `unsupported_feature` | The request is malformed or asks for a feature unsupported by the selected route. | Fix the request; do not retry unchanged. |
| `401` | `auth_error` | The proxy master key is missing or invalid. | Refresh credentials; do not retry unchanged. |
| `404` | route-specific | Unknown TokenScavenger endpoint. | Fix the URL. |
| `429` | `rate_limit_exceeded` | An upstream provider rate limit or quota condition prevented completion and no fallback succeeded. | Back off. Respect `Retry-After` when present. |
| `429` | `quota_exhausted` | Configured provider quota is exhausted until a reset window. | Back off until the indicated reset, or choose another model group. |
| `503` | `route_exhausted` | No viable route remained for reasons other than rate limits, such as unhealthy providers, open circuit breakers, unsupported models, or upstream 5xx failures. | Retry later or use a different model group; inspect `/metrics` and the UI. |
| `500` | `internal_error` | TokenScavenger hit an internal error. | Retry cautiously and inspect logs. |

## Rate Limits and Backoff

When an upstream provider returns a rate-limit response, TokenScavenger normalizes it into `ProviderError::RateLimited`. If another eligible provider or model group target succeeds, the caller still receives `200`. If every eligible route fails because of rate limiting or quota exhaustion, TokenScavenger returns `429 Too Many Requests`.

When TokenScavenger can derive a backoff window from upstream headers, it includes:

```http
Retry-After: 7
```

Clients should apply exponential backoff with jitter even when `Retry-After` is absent, because not every upstream provider returns reliable reset metadata. Failed rate-limited attempts may still count against upstream limits, so aggressive immediate retries can make recovery slower.

## Route Exhaustion

`503 route_exhausted` means the proxy could not build or complete a viable non-rate-limited route. Common causes include:

- the requested model is not present in the catalog or no provider supports it
- all matching providers are disabled, unhealthy, or blocked by an open circuit breaker
- upstream providers returned non-rate-limit failures
- a requested feature, such as tools, JSON mode, streaming, or embeddings, filtered out every candidate
- a project-scoped API key is restricted by model group, provider, privacy profile, paid-fallback, budget, or quota policy

For operational triage, check:

```bash
curl http://localhost:8000/v1/models
curl http://localhost:8000/readyz
curl http://localhost:8000/metrics
```

Useful metrics include:

- `tokenscavenger_requests_total{status=...}`
- `tokenscavenger_route_attempts_total`
- `tokenscavenger_provider_health_state`
- `tokenscavenger_provider_breaker_state`
- `tokenscavenger_quota_remaining`

## Observability And Incident Workflow

Every routed request receives a durable trace timeline in SQLite. Trace events
record the model group resolution, planned candidates, skip reasons, attempts,
retry/fallback decisions, upstream response class, latency, and usage linkage.
Request and diagnostic endpoints never include request bodies and apply the same
secret redaction used by the admin config API.

Useful triage endpoints:

```bash
curl http://localhost:8000/admin/observability/summary?period=24h
curl http://localhost:8000/admin/request-traces?limit=25
curl http://localhost:8000/admin/request-traces/<request-id>
curl http://localhost:8000/admin/incidents?limit=25
curl http://localhost:8000/admin/diagnostics/bundle
```

The embedded UI exposes the same workflow at `/ui/observability`.

## Model Intelligence

`GET /v1/models` remains OpenAI-compatible but may include additional optional
fields such as `provider_id`, `free_tier`, `context_window`, `task_tags`,
`modalities`, and `freshness`. Clients that only expect OpenAI's base model
shape can ignore these fields.

Chat and streaming route planning also consults model intelligence metadata.
Candidates are skipped before upstream calls when the request needs tools, JSON
mode, vision input, or a known context window that the model cannot satisfy.
`/admin/route-plan` exposes the same compatibility decision with query hints
such as `tools=true`, `json=true`, `vision=true`, `input_tokens=...`, and
`output_tokens=...`.

## Streaming Errors

For streaming chat completions, TokenScavenger may fall back only before any bytes have been sent to the caller. Once streaming output has begun, mid-stream fallback is intentionally disabled so clients do not receive a mixed response from multiple providers.

If a provider fails before the first streamed event, the normal status-code rules apply. If a provider fails after streaming has started, the stream terminates according to the SSE behavior of the route, and the failure is recorded in logs and metrics.

## Tool-Aware Routing

For `POST /v1/chat/completions` requests that include OpenAI `tools`,
TokenScavenger applies an additional routing pass after normal eligibility
filtering. The request still respects model groups, provider health, circuit
breakers, model enablement, and paid-fallback policy, but the remaining
attempts are reprioritized toward catalog entries marked tool-capable and
providers with stronger observed tool-call behavior.

This is designed for agent clients that need real streamed `tool_calls` and
correct tool-result continuation turns. Ordinary chat requests without `tools`
keep the configured model-group and provider order.

## Auth and Compatibility Notes

When `server.master_api_key` is configured, clients can send the master key:

```http
Authorization: Bearer <master-api-key>
```

Operators can also issue project-scoped OpenAI-compatible bearer keys from
`/ui/projects` or `/admin/projects/{project_id}/keys`. Project keys authorize
only `/v1/*` client endpoints. They do not authorize `/admin/*`, `/ui`, or
`/metrics`; those surfaces continue to require master-key, UI-session, or
external-identity admin auth.

Project keys are generated by TokenScavenger, shown once, stored only as hashes,
and identified afterward by prefix. Project policy can restrict model groups,
providers, paid fallback, local/free-only privacy profiles, per-request cost,
daily cost/request/token caps, sliding-window request/token quotas,
organization/environment caps, and key-level caps. Project policy failures
produce `503 route_exhausted` because no eligible route remains for that client.

The key is for TokenScavenger itself. Provider API keys remain in TokenScavenger configuration and are never returned in API responses, logs, fixtures, or UI payloads.

For admin UI/API access, TokenScavenger can also trust identity headers from an
identity-aware reverse proxy when `[server.external_identity] enabled = true`.
This supports Google, GitHub, Microsoft, and OSS identity providers through
OIDC-capable proxies such as oauth2-proxy, Dex, Authelia, Keycloak, and Zitadel.

External identity headers authorize only `/ui` and `/admin/*`. They do not
authorize OpenAI-compatible client endpoints under `/v1/*`; those continue to
use the `master_api_key` or a project-scoped API key when configured.

Role enforcement:

- `read_only`: view UI and read admin APIs.
- `operator`: read-only plus operational POSTs such as provider tests and discovery refreshes.
- `config_editor`: mutate non-secret runtime config.
- `credential_manager`: update credential-bearing config fields such as provider API keys.
- `admin`: full admin access.
