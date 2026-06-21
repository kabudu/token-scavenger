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

[server.external_identity]
enabled = false                     # Trust identity headers from a reverse proxy
user_header = "x-auth-request-user"
email_header = "x-auth-request-email"
name_header = "x-auth-request-preferred-username"
groups_header = "x-auth-request-groups"
group_delimiter = ","
read_only_groups = []
operator_groups = []
config_editor_groups = []
credential_manager_groups = []
admin_groups = []

[database]
path = "tokenscavenger.db"          # SQLite database file path
max_connections = 8                 # SQLite pool size

[logging]
format = "json"                     # "json" or "text"
level = "info"                      # trace | debug | info | warn | error

[metrics]
enabled = true                      # Enable Prometheus metrics endpoint
path = "/metrics"                   # Metrics endpoint path

[security.credential_encryption]
enabled = false                     # Encrypt credential-bearing runtime overrides
key_env = "TOKENSCAVENGER_CREDENTIAL_KEY"

[retention]
usage_days = 30
health_event_days = 30
audit_days = 90
request_trace_days = 30

[updates]
enabled = true                      # Enable admin UI/API self-update checks
github_repo = "kabudu/token-scavenger"
check_interval_secs = 21600

[routing]
free_first = true                   # Always prefer free tier first
allow_paid_fallback = false         # Allow fallback to paid providers
objective = "balanced"              # min_cost | min_latency | balanced | quality_first | local_only
default_model_group_strategy = "provider-priority"
provider_order = [                  # Fallback ordering
    "ollama", "local", "groq", "cerebras", "google", "openrouter", "cloudflare",
    "nvidia", "mistral", "github-models", "siliconflow",
    "huggingface", "cohere", "zai", "deepseek", "xai"
]

[routing.model_group_objectives]
"agentic" = "quality_first"
"cheap:code" = "min_cost"

[routing.budgets]
max_cost_per_request_usd = 0.01
max_cost_per_day_usd = 2.00

[routing.budgets.max_cost_per_provider_per_day_usd]
deepseek = 1.00
xai = 0.50

[routing.budgets.max_cost_per_model_group_per_day_usd]
"agentic" = 1.50

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
embedding_support = "auto"          # auto | enabled | disabled for local embeddings

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

#### `[server.external_identity]`

External identity support lets TokenScavenger sit behind an identity-aware
reverse proxy such as oauth2-proxy, Dex, Authelia, Keycloak, Zitadel, or a cloud
load balancer. Those systems can authenticate Google, GitHub, Microsoft, or any
OIDC/SAML provider and forward trusted identity headers to TokenScavenger.

TokenScavenger does not trust these headers unless `enabled = true`. When
enabled, external identity headers authorize only `/ui` and `/admin/*`; OpenAI
client endpoints such as `/v1/chat/completions` still require the
`master_api_key` when one is configured.

| Field | Default | Description |
|-------|---------|-------------|
| `enabled` | `false` | Enables trusted reverse-proxy identity headers for admin UI/API access. |
| `user_header` | `"x-auth-request-user"` | Header containing the stable subject/user id. |
| `email_header` | `"x-auth-request-email"` | Optional email header used for display and audit actor strings. |
| `name_header` | `"x-auth-request-preferred-username"` | Optional display-name header. |
| `groups_header` | `"x-auth-request-groups"` | Header containing group names. |
| `group_delimiter` | `","` | Delimiter used to split the groups header. |
| `read_only_groups` | `[]` | Groups allowed to view the UI and read admin APIs. |
| `operator_groups` | `[]` | Groups allowed to run operational actions such as provider tests and discovery refreshes. |
| `config_editor_groups` | `[]` | Groups allowed to edit non-secret runtime configuration. |
| `credential_manager_groups` | `[]` | Groups allowed to update credential-bearing fields such as provider API keys. |
| `admin_groups` | `[]` | Groups with full admin access. |

Example:

```toml
[server.external_identity]
enabled = true
read_only_groups = ["tokenscavenger-viewers"]
operator_groups = ["tokenscavenger-operators"]
config_editor_groups = ["tokenscavenger-editors"]
credential_manager_groups = ["tokenscavenger-credential-managers"]
admin_groups = ["tokenscavenger-admins"]
```

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

### `[security.credential_encryption]`

When enabled, TokenScavenger encrypts credential-bearing values before writing
runtime override files created by the admin UI or CLI hot-reload flow. Runtime
memory still holds decrypted values so provider adapters can authenticate
normally.

The key is supplied by the environment variable named in `key_env`. TokenScavenger
derives a 256-bit AES-GCM key from that value. Keep this key stable across
restarts; encrypted override files cannot be decrypted without it.

| Field | Default | Description |
|-------|---------|-------------|
| `enabled` | `false` | Encrypts persisted runtime override secrets when true. |
| `key_env` | `"TOKENSCAVENGER_CREDENTIAL_KEY"` | Environment variable containing the operator-supplied encryption key. |

Encrypted values are stored with the `tsenc:v1:` prefix. Base config files may
still contain plaintext secrets if operators write them manually; use environment
variable references there for best hygiene.

### `[retention]`

| Field | Default | Description |
|-------|---------|-------------|
| `usage_days` | `30` | Retention window for usage events. |
| `health_event_days` | `30` | Retention window for provider health events. |
| `audit_days` | `90` | Retention window for config audit entries. |
| `request_trace_days` | `30` | Retention window for request trace events. |

### `[updates]`

| Field | Default | Description |
|-------|---------|-------------|
| `enabled` | `true` | Enables `/admin/update/check`, `/admin/update/apply`, and the admin UI update banner. |
| `github_repo` | `"kabudu/token-scavenger"` | GitHub repository used for release discovery. |
| `check_interval_secs` | `21600` | Intended operator polling interval for update checks. |

Update checks are passive and resilient: if GitHub is unreachable or the host
has no internet access, `/admin/update/check` still returns `200 OK` with
`update_available = false` and a `check_error` message for diagnostics.

When applying an update, TokenScavenger downloads the platform release asset,
verifies it against `checksums.txt`, replaces the current executable, and
restarts with the same executable arguments.

### `[routing]`

| Field | Default | Description |
|-------|---------|-------------|
| `free_first` | `true` | When true, free-tier providers are always preferred over paid. |
| `allow_paid_fallback` | `false` | If true, providers marked `free_only = false` may be used after earlier routes are exhausted. |
| `objective` | `"balanced"` | Scoring objective for eligible attempts: `min_cost`, `min_latency`, `balanced`, `quality_first`, or `local_only`. |
| `model_group_objectives` | `{}` | Optional per-model-group objective overrides keyed by the requested model/group name. |
| `budgets.max_cost_per_request_usd` | unset | Hard estimated USD cap for a single request. Paid routes with unknown pricing are blocked when any matching hard budget exists. |
| `budgets.max_cost_per_day_usd` | unset | Hard estimated USD cap across all usage recorded today. |
| `budgets.max_cost_per_provider_per_day_usd` | `{}` | Hard estimated USD cap per provider for usage recorded today. |
| `budgets.max_cost_per_model_group_per_day_usd` | `{}` | Hard estimated USD cap per requested model/model group for usage recorded today. |
| `default_model_group_strategy` | `"provider-priority"` | Strategy for resolving model groups with multiple targets. |
| `provider_order` | `[default list]` | Ordered list defining the fallback chain. First match wins. |

TokenScavenger first applies endpoint capability, model enablement, provider
health, circuit breaker, quota, and paid-fallback filters. It then applies hard
budgets and scores the remaining candidates using estimated cost, observed
latency, recent failure rate, context window, model/provider capability, and
operator order.
Route-plan explanations include the selected objective, score components,
estimated cost, observed latency, failure rate, and skip reasons.

The `local_only` objective keeps only local attempts. Built-in local provider
IDs (`local`, `ollama`, `llama-cpp`, and `lmstudio`) always qualify, and any
provider whose configured `base_url` uses `localhost`, `127.0.0.1`, or `::1`
also qualifies. Local providers still use the normal adapter, health, breaker,
fallback, usage, and metrics paths.

For chat requests that include OpenAI `tools`, TokenScavenger applies a
tool-aware reprioritization pass before policy scoring so agentic workloads and
Hermes-style coding harnesses prefer providers with stronger tool-call behavior
while still respecting enabled models, provider health, circuit breakers,
budgets, and paid-fallback policy.

Model intelligence metadata is also applied before policy scoring. TokenScavenger
normalizes model family, task tags, modalities, context window, tool/JSON/vision
signals, reasoning hints, embeddings support, and discovery freshness from the
curated and discovered catalog. Requests that need tools, JSON mode, vision
input, or a context length larger than a model's known window are rerouted before
an upstream call is made.

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
| `embedding_support` | `"auto"` | For local OpenAI-compatible providers, controls whether discovered models are marked embedding-capable: `"auto"` probes `/embeddings`, `"enabled"` trusts the operator override, and `"disabled"` suppresses embeddings. Remote providers ignore this field. |

Supported provider IDs:
`groq`, `google`, `openrouter`, `cloudflare`, `cerebras`, `nvidia`, `cohere`, `mistral`, `github-models`, `huggingface`, `zai` (or `zhipu`), `siliconflow`, `deepseek`, `xai` (or `grok`), `local`, `ollama`, `llama-cpp` (or `llamacpp`), `lmstudio` (or `lm-studio`)

Local provider defaults:

| Provider | Default base URL | Notes |
|----------|------------------|-------|
| `local` | `http://127.0.0.1:1234/v1` | Generic OpenAI-compatible local or LAN upstream. Override `base_url` for your server. |
| `ollama` | `http://127.0.0.1:11434/v1` | Uses Ollama's OpenAI-compatible endpoints. |
| `llama-cpp` | `http://127.0.0.1:8080/v1` | Uses the llama.cpp server OpenAI-compatible API. |
| `lmstudio` | `http://127.0.0.1:1234/v1` | Uses LM Studio's local OpenAI-compatible server. |

Local embeddings are intentionally model-aware. With the default
`embedding_support = "auto"`, TokenScavenger probes each discovered local model
against `/embeddings` before advertising embeddings support in `/v1/models` or
using that model for embedding routes. Use `"enabled"` when a local server blocks
or dislikes probes but you know the model supports embeddings, and `"disabled"`
when the local server is chat-only.

Discovery refresh runs in the normal background discovery loop. Local embedding
probes are bounded to four concurrent probe requests with a short per-model
timeout, so a large local catalog or unsupported embedding endpoint does not
serially stall the refresh cycle.

### `[[model_groups]]`

| Field | Default | Description |
|-------|---------|-------------|
| `name` | *(required)* | Public model group name used in the `model` field of requests |
| `target` | *(required)* | Single target or ordered target array. Strings are model IDs that can route through any eligible provider. Objects pin a model to one provider: `{ provider = "nvidia", model = "google/gemma-4-31b-it" }`. |

TokenScavenger seeds editable smart groups on first run:

- `fast:chat`
- `cheap:code`
- `reasoning:deep`
- `vision:balanced`

They use the same `model_groups` table and are inserted only when absent, so
operator-edited groups are never overwritten by startup seeding.

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
target = ["llama3.1-8b", { provider = "groq", model = "llama3-8b-8192" }]

[[model_groups]]
name = "powerful"
target = [
  { provider = "groq", model = "llama3-70b-8192" },
  "gemini-2.0-flash",
  { provider = "openrouter", model = "meta-llama/llama-3.3-70b-instruct:free" },
]
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
