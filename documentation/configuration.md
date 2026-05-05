# Configuration Reference

TokenScavenger uses a single TOML configuration file, typically `tokenscavenger.toml`. Environment variables in `${VAR_NAME}` syntax are expanded automatically.

## Complete Schema

```toml
[server]
bind = "0.0.0.0:8000"              # Host:port to bind HTTP listener
master_api_key = ""                 # Optional API key for proxy auth
allowed_cors_origins = []           # Explicit browser origins allowed by CORS
allow_query_api_keys = false        # Accept ?api_key=... auth only when true
ui_session_auth = false             # Enable browser cookie session login endpoint
ui_enabled = true                   # Enable/disable operator dashboard
ui_path = "/ui"                     # URL prefix for UI
request_timeout_ms = 120000         # Upstream request timeout

[database]
path = "tokenscavenger.db"          # SQLite database file path
max_connections = 8                 # SQLite pool size

[logging]
format = "json"                     # "json" or "text"
level = "info"                      # trace | debug | info | warn | error

[metrics]
enabled = true                      # Enable Prometheus metrics endpoint
path = "/metrics"                   # Metrics endpoint path

[routing]
free_first = true                   # Always prefer free tier first
allow_paid_fallback = false         # Allow fallback to paid providers
default_model_group_strategy = "provider-priority"
provider_order = [                  # Fallback ordering
    "groq", "cerebras", "google", "openrouter", "cloudflare",
    "nvidia", "mistral", "github-models", "siliconflow",
    "huggingface", "cohere", "zai", "deepseek", "xai"
]

[resilience]
max_retries_per_provider = 2        # Retry attempts per provider
breaker_failure_threshold = 3       # Consecutive failures to open breaker
breaker_cooldown_secs = 60          # Seconds before half-open retry
health_probe_interval_secs = 30     # Health check interval

[[providers]]
id = "groq"                         # Provider identifier
enabled = true                      # Enable/disable without removing config
base_url = ""                       # Override default API endpoint
api_key = "${GROQ_API_KEY}"         # API key (supports env expansion)
free_only = true                    # Mark as free-tier
discover_models = true              # Auto-discover available models

# Model groups for simplified model routing
[[model_groups]]
name = "free:llama-70b"
target = ["groq/llama3-70b-8192", "google/gemini-2.0-flash"]
```

## Section Details

### `[server]`

| Field | Default | Description |
|-------|---------|-------------|
| `bind` | `"0.0.0.0:8000"` | Address and port to bind the HTTP server |
| `master_api_key` | `""` | When set, all API requests must include `Authorization: Bearer <key>`. Empty = no auth. |
| `allowed_cors_origins` | `[]` | Browser origins allowed by CORS, for example `["https://ops.example"]`. Empty uses the non-permissive default. |
| `allow_query_api_keys` | `false` | When `true`, `?api_key=<key>` is accepted as an explicit compatibility path. Prefer bearer auth. |
| `ui_session_auth` | `false` | Enables `POST /admin/session` to exchange the master key for an HttpOnly browser cookie session. |
| `ui_enabled` | `true` | Whether to serve the operator web UI |
| `ui_path` | `"/ui"` | URL path prefix for the UI |
| `request_timeout_ms` | `120000` | Maximum time to wait for an upstream provider response |

### `[database]`

| Field | Default | Description |
|-------|---------|-------------|
| `path` | `"tokenscavenger.db"` | Path to the SQLite database file. Created automatically on first run. |
| `max_connections` | `8` | Maximum SQLite pool connections. Increase carefully for higher write concurrency. |

### `[logging]`

| Field | Default | Description |
|-------|---------|-------------|
| `format` | `"json"` | Log output format. `"json"` for structured JSON logs, `"text"` for human-readable. |
| `level` | `"info"` | Minimum log level. Also controlled by `RUST_LOG` environment variable. |

### `[metrics]`

| Field | Default | Description |
|-------|---------|-------------|
| `enabled` | `true` | Whether to expose Prometheus metrics. |
| `path` | `"/metrics"` | HTTP endpoint path for Prometheus scrape. |

### `[routing]`

| Field | Default | Description |
|-------|---------|-------------|
| `free_first` | `true` | When true, free-tier providers are always preferred over paid. |
| `allow_paid_fallback` | `false` | If true, providers marked `free_only = false` may be used after earlier routes are exhausted. |
| `default_model_group_strategy` | `"provider-priority"` | Strategy for resolving model groups with multiple targets. |
| `provider_order` | `[default list]` | Ordered list defining the fallback chain. First match wins. |

### `[resilience]`

| Field | Default | Description |
|-------|---------|-------------|
| `max_retries_per_provider` | `2` | Maximum retry attempts on the same provider before trying the next. |
| `breaker_failure_threshold` | `3` | Number of consecutive failures before a circuit breaker opens. |
| `breaker_cooldown_secs` | `60` | Seconds to wait before allowing a half-open retry. |
| `health_probe_interval_secs` | `30` | Interval between active health probes. |

### `[[providers]]`

This is an array of provider configurations. Each entry specifies:

| Field | Default | Description |
|-------|---------|-------------|
| `id` | *(required)* | Provider identifier. Must match a supported provider name. |
| `enabled` | `true` | When `false`, the provider is skipped during routing. |
| `base_url` | *(provider default)* | Override the default API base URL for this provider. |
| `api_key` | `""` | API key. Supports `${ENV_VAR}` expansion. |
| `free_only` | `true` | If `false`, this provider can be used for paid fallback. |
| `discover_models` | `true` | Whether to query the provider's model list endpoint. |

Supported provider IDs:
`groq`, `google`, `openrouter`, `cloudflare`, `cerebras`, `nvidia`, `cohere`, `mistral`, `github-models`, `huggingface`, `zai` (or `zhipu`), `siliconflow`, `deepseek`, `xai` (or `grok`)

### `[[model_groups]]`

| Field | Default | Description |
|-------|---------|-------------|
| `name` | *(required)* | Public model group name used in the `model` field of requests |
| `target` | *(required)* | Array of provider/model pairs to try in order |

## Example Configurations

### Minimal (single provider)

```toml
[server]
bind = "127.0.0.1:8000"

[[providers]]
id = "groq"
api_key = "${GROQ_API_KEY}"
```

### Multi-provider with model groups

```toml
[server]
bind = "0.0.0.0:8000"
master_api_key = "${TOKENSAVENGER_KEY}"

[routing]
free_first = true
allow_paid_fallback = false

[[providers]]
id = "groq"
api_key = "${GROQ_API_KEY}"

[[providers]]
id = "google"
api_key = "${GEMINI_API_KEY}"

[[providers]]
id = "openrouter"
api_key = "${OPENROUTER_API_KEY}"

[[providers]]
id = "cerebras"
api_key = "${CEREBRAS_API_KEY}"

[[providers]]
id = "mistral"
api_key = "${MISTRAL_API_KEY}"

[[model_groups]]
name = "fast"
target = ["cerebras/llama3.1-8b", "groq/llama3-8b-8192"]

[[model_groups]]
name = "powerful"
target = ["groq/llama3-70b-8192", "google/gemini-2.0-flash", "openrouter/meta-llama/llama-3.3-70b-instruct:free"]
```

### Free-first with paid fallback

```toml
[routing]
free_first = true
allow_paid_fallback = true
provider_order = ["groq", "cerebras", "openrouter", "deepseek", "xai"]

[[providers]]
id = "groq"
api_key = "${GROQ_API_KEY}"
free_only = true

[[providers]]
id = "deepseek"
api_key = "${DEEPSEEK_API_KEY}"
free_only = false

[[providers]]
id = "xai"
api_key = "${XAI_API_KEY}"
free_only = false
```
