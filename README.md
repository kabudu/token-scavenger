# TokenScavenger

**A lightweight, self-hosted LLM proxy/router that prioritizes free-tier inference providers.**

TokenScavenger exposes an OpenAI-compatible HTTP API so existing clients can switch by changing only their `base_url`. It automatically routes requests across 12+ free-tier providers with automatic fallback, circuit breakers, health monitoring, and a built-in operator web UI.

```text
┌─────────────┐     POST /v1/chat/completions     ┌────────────────┐
│  Your App   │ ──────────────────────────────────→│ TokenScavenger │
│ (OpenAI SDK)│                                     │   :8000        │
│             │←──── OpenAI-shaped responses ──────│                │
└─────────────┘                                     └───────┬────────┘
                            ┌───────────────────────────────┤
                            ↓                               ↓
                    ┌──────────────┐              ┌──────────────────┐
                    │  Groq (free)  │              │  Gemini (free)   │
                    ├──────────────┤              ├──────────────────┤
                    │ Cerebras     │              │ OpenRouter       │
                    │ Mistral      │              │ Cloudflare       │
                    │ NVIDIA NIM   │              │ GitHub Models    │
                    │ HuggingFace  │              │ SiliconFlow      │
                    │ ZAI/Zhipu    │              │ Cohere           │
                    └──────────────┘              └──────────────────┘
```

## Features

- **Free-tier first routing** — automatically prefers free providers, falls back through a configurable chain
- **OpenAI-compatible API** — works with existing OpenAI SDK clients (just change `base_url`)
- **12 built-in providers** — Groq, Google Gemini, OpenRouter, Cloudflare, Cerebras, NVIDIA NIM, Cohere, Mistral AI, GitHub Models, HuggingFace, Zhipu AI, SiliconFlow
- **Streaming SSE** — full support for OpenAI-style streaming chat completions
- **Circuit breakers & retries** — per-provider health tracking with automatic recovery
- **Model discovery** — automatic provider model list discovery plus curated built-in catalog
- **Prometheus metrics** — request counts, latency histograms, token usage, health states
- **Embedded web UI** — operator dashboard for providers, models, routing, usage, health, logs, config
- **SQLite persistence** — WAL mode, usage accounting, audit log, health events
- **Single binary** — no Python, Node, or Docker required for basic operation

## Quick Start

### 1. Get API keys

Sign up for free API keys from your preferred providers:

| Provider | Sign Up |
|----------|---------|
| Groq | https://console.groq.com/ |
| Google Gemini | https://aistudio.google.com/ |
| OpenRouter | https://openrouter.ai/ |
| Cerebras | https://inference-docs.cerebras.ai/ |
| Mistral | https://console.mistral.ai/ |
| NVIDIA NIM | https://build.nvidia.com/ |
| Cloudflare | https://developers.cloudflare.com/workers-ai/ |

### 2. Configure

Create `tokenscavenger.toml`:

```toml
[server]
bind = "0.0.0.0:8000"

[database]
path = "tokenscavenger.db"

[logging]
level = "info"

[routing]
free_first = true
allow_paid_fallback = false

[[providers]]
id = "groq"
enabled = true
api_key = "${GROQ_API_KEY}"
free_only = true
discover_models = true

[[providers]]
id = "google"
enabled = true
api_key = "${GEMINI_API_KEY}"
free_only = true
discover_models = true
```

Environment variables are expanded automatically (`${VAR_NAME}` syntax).

### 3. Run

```bash
cargo run --release -- -c tokenscavenger.toml
```

Or build a static binary:

```bash
cargo build --release
./target/release/tokenscavenger -c tokenscavenger.toml
```

### 4. Use it

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://localhost:8000/v1",
    api_key="optional-master-key"
)

response = client.chat.completions.create(
    model="llama3-70b-8192",  # or any alias
    messages=[{"role": "user", "content": "Hello!"}]
)
print(response.choices[0].message.content)
```

## Architecture

TokenScavenger is a single Rust binary using Axum + Tokio + SQLite with these subsystems:

```
src/
  api/          OpenAI-compatible routes, auth, error taxonomy
  app/          Application state, startup/shutdown lifecycle
  config/       TOML config loading, validation, env var expansion
  db/           SQLite pool, migrations (9 tables), helpers
  discovery/    Model discovery, curated catalog, merge logic
  metrics/      Prometheus counters/histograms, structured tracing
  providers/    12 provider adapter implementations
  resilience/   Circuit breakers, health tracking, retry/backoff
  router/       Route planning engine, policy, aliases, fallback
  ui/           Embedded operator web UI (9 views)
  usage/        Usage accounting, aggregation, pricing
  util/         Secret redaction, time utilities
```

## Configuration Reference

See [docs/configuration.md](docs/configuration.md) for the full configuration schema.

## Provider Support Matrix

See [docs/provider-matrix.md](docs/provider-matrix.md) for details on each provider's API format, free tier limits, and known quirks.

## Deployment

See [docs/deployment.md](docs/deployment.md) for deployment options including Docker, systemd, and cross-compilation.

## Operator UI

Open `http://localhost:8000/ui` in your browser for the operator dashboard with views for:

- Dashboard — system status, uptime, provider count
- Providers — enable/disable, inspect health and breaker state
- Models — view discovered and curated models
- Routing — view fallback order and alias configuration
- Usage — token counts and estimated costs
- Health — per-provider health states
- Logs — real-time log stream via SSE
- Config — view current effective configuration
- Audit — configuration change history

## Development

```bash
# Run tests
cargo test

# Build release binary
cargo build --release

# Check for warnings
cargo clippy --all-targets --all-features

# Format code
cargo fmt --all
```

## License

MIT — see [LICENSE](LICENSE).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).
