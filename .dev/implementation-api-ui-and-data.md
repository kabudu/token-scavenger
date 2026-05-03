# TokenScavenger API, UI, and Data Design

## 1. External HTTP Surface

### 1.1 Public API Routes

- `POST /v1/chat/completions`
- `POST /v1/embeddings`
- `GET /v1/models`
- `GET /healthz`
- `GET /readyz`
- `GET /metrics`
- `GET /ui`

### 1.2 Admin/Internal Routes

Keep internal control routes under `/admin` or `/internal`, disabled or API-key-protected by default:

- `POST /admin/providers/discovery/refresh`
- `POST /admin/providers/:provider_id/test`
- `GET /admin/providers`
- `GET /admin/models`
- `GET /admin/config`
- `PUT /admin/config`
- `GET /admin/audit`
- `GET /admin/logs/stream`
- `GET /admin/usage/series`
- `GET /admin/health/events`

The UI may call these routes directly.

## 2. OpenAI-Compatible Request Handling

### 2.1 Chat Completions

Support these request fields in MVP:

- `model`
- `messages`
- `temperature`
- `top_p`
- `max_tokens`
- `stream`
- `stop`
- `presence_penalty`
- `frequency_penalty`
- `user`
- `response_format` where feasible
- `tools` and `tool_choice` when the chosen provider supports them

Behavior rules:

- If the caller requests unsupported fields and no candidate provider can satisfy them, return a compatibility error.
- Preserve OpenAI-style error envelope shape.
- Generate request IDs and include them in response headers and logs.

### 2.2 Embeddings

Support:

- `model`
- `input`
- `encoding_format`
- `user`

Normalize output vectors and usage fields into OpenAI-compatible response objects.

### 2.3 Models

Return an OpenAI-style list object with TokenScavenger metadata extensions added under a namespaced field if needed. Do not break compatibility for clients that only expect standard fields.

## 3. Error Contract

All API errors must conform to an OpenAI-like shape:

```json
{
  "error": {
    "message": "No healthy free-tier provider available for alias free:llama-3.3-70b",
    "type": "provider_unavailable",
    "param": null,
    "code": "route_exhausted"
  }
}
```

Internal error taxonomy:

- `invalid_request`
- `auth_error`
- `provider_unavailable`
- `route_exhausted`
- `rate_limited`
- `quota_exhausted`
- `unsupported_feature`
- `internal_error`

Map internal errors to appropriate HTTP statuses consistently.

## 4. Configuration Model

Support file-based boot configuration plus database-persisted effective configuration.

### 4.1 File Config Responsibilities

- bootstrap server settings
- bootstrap auth settings
- provider credentials and default enablement
- fallback defaults
- database path
- metrics and logging configuration

### 4.2 Database-Persisted Config Responsibilities

- provider enable/disable state
- provider ordering
- model enable/disable state
- alias definitions
- UI preferences
- rate limits
- audit entries

Boot rule:

- File config loads first.
- DB overrides apply next for mutable operator state.
- Environment variable expansion is allowed only in file config and secret fields.

### 4.3 Suggested TOML Schema

```toml
[server]
bind = "0.0.0.0:8000"
master_api_key = ""
ui_enabled = true
ui_path = "/ui"
request_timeout_ms = 120000

[database]
path = "tokenscavenger.db"

[logging]
format = "json"
level = "info"

[metrics]
enabled = true
path = "/metrics"

[routing]
free_first = true
allow_paid_fallback = false
default_alias_strategy = "provider-priority"
provider_order = ["groq", "cerebras", "google", "openrouter-free", "cloudflare", "nvidia", "mistral", "github-models", "zai", "deepseek", "xai"]

[resilience]
max_retries_per_provider = 2
breaker_failure_threshold = 3
breaker_cooldown_secs = 60
health_probe_interval_secs = 30

[[providers]]
id = "groq"
enabled = true
base_url = "https://api.groq.com/openai/v1"
api_key = "${GROQ_API_KEY}"
free_only = true
discover_models = true
```

The actual Rust config structs should be versioned for future migrations.

## 5. Database Schema

Use SQL migrations. Suggested tables:

### 5.1 `config_snapshots`

- `id`
- `version`
- `created_at`
- `created_by`
- `source`
- `config_json`

### 5.2 `config_audit_log`

- `id`
- `created_at`
- `actor`
- `action`
- `target_type`
- `target_id`
- `before_json`
- `after_json`

### 5.3 `providers`

- `provider_id`
- `display_name`
- `enabled`
- `priority`
- `base_url`
- `auth_kind`
- `free_only`
- `discovery_state`
- `last_discovery_at`
- `last_success_at`
- `last_error_at`
- `last_error_summary`

### 5.4 `models`

- `id`
- `provider_id`
- `upstream_model_id`
- `public_model_id`
- `enabled`
- `free_tier`
- `paid_fallback`
- `supports_chat`
- `supports_embeddings`
- `supports_streaming`
- `supports_tools`
- `supports_vision`
- `supports_json_mode`
- `priority`
- `metadata_json`
- `discovered_at`
- `updated_at`

### 5.5 `aliases`

- `alias`
- `target_json`
- `enabled`
- `created_at`
- `updated_at`

### 5.6 `request_log`

- `request_id`
- `received_at`
- `endpoint_kind`
- `caller_key_hash`
- `requested_model`
- `resolved_alias`
- `selected_provider_id`
- `selected_model_id`
- `status`
- `http_status`
- `latency_ms`
- `streaming`
- `retry_count`
- `fallback_count`
- `error_code`
- `error_summary`

### 5.7 `usage_events`

- `id`
- `request_id`
- `provider_id`
- `model_id`
- `timestamp`
- `input_tokens`
- `output_tokens`
- `estimated_cost_usd`
- `cost_confidence`
- `free_tier`

### 5.8 `provider_health_events`

- `id`
- `provider_id`
- `recorded_at`
- `health_state`
- `breaker_state`
- `latency_ms`
- `status_code`
- `event_type`
- `details_json`

### 5.9 `discovery_runs`

- `id`
- `provider_id`
- `started_at`
- `finished_at`
- `status`
- `models_found`
- `error_summary`

## 6. UI Requirements

The UI must be operational, not decorative. It is part of the core product requirement.

### 6.1 Main Views

- Dashboard
- Providers
- Models
- Routing/Aliases
- Usage Analytics
- Health
- Logs
- Configuration
- Audit History

### 6.2 Dashboard

Must show:

- total requests today
- input/output tokens today
- free versus paid request split
- estimated spend today
- active healthy providers
- error rate
- p50/p95 latency
- recent discovery status

### 6.3 Providers View

Must support:

- enable/disable provider
- reorder providers
- inspect provider health and breaker state
- test connection
- view last discovery result
- inspect configured base URL and auth mode without exposing secrets

### 6.4 Models View

Must support:

- view discovered and curated models
- enable/disable model
- filter by provider, modality, free/paid, healthy/unhealthy
- inspect support flags like streaming and tools
- manual refresh discovery

### 6.5 Routing/Aliases View

Must support:

- alias creation and editing
- explicit fallback-chain editing
- view effective route plan for a sample request
- explain why a provider is skipped

### 6.6 Usage Analytics View

Must support:

- time-series charts by provider and model
- free versus paid usage
- token totals
- error counts
- latency trends

### 6.7 Health View

Must support:

- per-provider health state
- current breaker state
- recent health events
- recent quota/rate-limit observations

### 6.8 Logs View

Must support:

- real-time stream of structured logs
- filtering by request ID, provider, endpoint, status
- redacted rendering of secrets

### 6.9 Configuration View

Must support:

- operator-editable settings with validation
- preview of effective configuration
- save with audit log entry
- rollback to previous config snapshot

## 7. UI Technology Guidance

The original spec permits HTMX + Tailwind and mentions Leptos as optional for richer interactivity. Implementation guidance:

- Use server-rendered Axum pages with HTMX for MVP to preserve binary simplicity.
- Use Chart.js or a small embedded chart library for analytics views if Leptos is deferred.
- Keep pages fully functional without a SPA runtime.
- Embed all assets into the binary.

## 8. Security Controls

Required controls:

- optional master API key for proxy access
- UI auth if exposed beyond localhost
- redact secrets from logs and UI
- configurable CORS
- configurable rate limiting per caller key
- audit log for config changes
- no plaintext provider key exposure after save

Recommended controls for near-term follow-up:

- role-based admin separation
- per-user virtual keys
- CSRF protection on UI forms if cookie auth is used

## 9. Metrics and Logging Requirements

### 9.1 Prometheus Metrics

Required metrics include, at minimum:

- `tokenscavenger_requests_total{provider,model,endpoint,status}`
- `tokenscavenger_request_latency_seconds_bucket{provider,endpoint}`
- `tokenscavenger_tokens_total{provider,model,type}`
- `tokenscavenger_route_attempts_total{provider,model,outcome}`
- `tokenscavenger_provider_health_state{provider,state}`
- `tokenscavenger_provider_breaker_state{provider,state}`
- `tokenscavenger_quota_remaining{provider}`
- `tokenscavenger_discovery_runs_total{provider,status}`

### 9.2 Structured Log Fields

Every request-path log should include:

- timestamp
- level
- request_id
- endpoint_kind
- requested_model
- resolved_model
- provider_id
- upstream_model_id
- attempt_index
- retry_count
- fallback_count
- latency_ms
- streaming
- http_status
- error_code

## 10. Retention and Data Management

Support retention configuration for:

- request log rows
- usage event rows
- health events
- audit history

Implement cleanup jobs and make retention visible in config/UI.

## 11. Acceptance Criteria

API/UI/data implementation is complete when:

- OpenAI-compatible endpoints behave correctly for supported fields
- database migrations initialize a clean instance and upgrade an old one
- UI allows an operator to manage providers and diagnose routing behavior
- logs and metrics expose the decision-making path clearly
- config changes are persisted, validated, audited, and reversible

## 12. API, UI, And Data Checklist

Mark each item as the API, UI, and persistence surfaces are implemented.

- [x] Implement public routes for chat completions, embeddings, models, health, readiness, metrics, and UI.
- [x] Implement protected admin/internal routes for providers, models, config, audit, logs, usage, health events, and discovery refresh.
- [x] Parse OpenAI-compatible requests into typed internal structs while preserving safe extra fields.
- [x] Return OpenAI-shaped success and error responses with request IDs.
- [x] Normalize embeddings responses and usage accounting.
- [x] Create SQL migrations for config, audit, providers, models, aliases, request logs, usage events, health events, and discovery runs.
- [x] Implement file config loading plus database-persisted mutable operator state.
- [x] Implement dashboard, providers, models, routing/aliases, usage, health, logs, configuration, and audit views.
- [x] Add UI actions for provider enablement, provider ordering, model enablement, alias editing, discovery refresh, config save, and rollback.
- [x] Add auth, CORS, rate limiting hooks, secret masking, and audit logging for protected actions.
- [x] Emit required Prometheus metrics and structured request-path log fields.
- [x] Implement retention configuration and cleanup jobs.
- [x] Cover API behavior, migrations, UI smoke flows, and security controls with tests.
- [x] Package reproducible binary artifacts for supported platforms.
