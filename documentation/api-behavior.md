# API Behavior

TokenScavenger exposes OpenAI-compatible HTTP endpoints while routing each request through one or more upstream providers. The response body follows the OpenAI error shape, but the HTTP status code is chosen to help downstream clients decide whether to retry immediately, back off, fix the request, or treat the proxy as unavailable.

## Supported Endpoints

| Endpoint | Description |
|----------|-------------|
| `POST /v1/chat/completions` | OpenAI-compatible chat completions, including streaming SSE when `stream: true`. |
| `POST /v1/embeddings` | OpenAI-compatible embeddings for providers that support embeddings. |
| `GET /v1/models` | Merged public model catalog from curated models, discovery, and operator configuration. |
| `GET /healthz` | Lightweight process health check. |
| `GET /readyz` | Readiness check with dependency state. |
| `GET /metrics` | Prometheus metrics. |
| `GET /ui` | Embedded operator dashboard, when enabled. |

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

## Streaming Errors

For streaming chat completions, TokenScavenger may fall back only before any bytes have been sent to the caller. Once streaming output has begun, mid-stream fallback is intentionally disabled so clients do not receive a mixed response from multiple providers.

If a provider fails before the first streamed event, the normal status-code rules apply. If a provider fails after streaming has started, the stream terminates according to the SSE behavior of the route, and the failure is recorded in logs and metrics.

## Auth and Compatibility Notes

When `server.master_api_key` is configured, clients must send:

```http
Authorization: Bearer <master-api-key>
```

The key is for TokenScavenger itself. Provider API keys remain in TokenScavenger configuration and are never returned in API responses, logs, fixtures, or UI payloads.
