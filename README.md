<p align="center">
  <img src="resources/TokenScavengerLogo.png" alt="TokenScavenger Logo" width="200">
</p>

<h1 align="center">TokenScavenger</h1>

<p align="center">
  <a href="https://github.com/kabudu/token-scavenger/actions/workflows/ci.yml"><img src="https://github.com/kabudu/token-scavenger/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT License"></a>
</p>

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
# Optional: require Authorization: Bearer <key>
# master_api_key = "${TOKENSAVENGER_KEY}"
# Optional browser origins allowed by CORS
allowed_cors_origins = []

[database]
path = "tokenscavenger.db"
max_connections = 8

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

On first run, `tokenscavenger` detects the absence of a config file and offers to
run the interactive setup wizard:

```bash
./tokenscavenger
```

Follow the prompts to configure your server, providers, and API keys. The wizard
writes a configuration to `~/.config/tokenscavenger/tokenscavenger.toml`.

To use an existing config file:

```bash
./tokenscavenger -c tokenscavenger.toml
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

## CLI Commands

The `tokenscavenger` binary provides three modes of operation:

| Command | Description |
|---------|-------------|
| `tokenscavenger` (no args) | Starts the server. On first run, prompts to run the setup wizard if no config is found. |
| `tokenscavenger setup` | Run the interactive first-time setup wizard. |
| `tokenscavenger config` | Edit an existing configuration file interactively. |

### `tokenscavenger setup`

Walks you through creating a configuration file from scratch — server bind address,
master API key, routing preferences, and provider credentials. The wizard stores
the resulting config at `~/.config/tokenscavenger/tokenscavenger.toml`.

### `tokenscavenger config`

Loads an existing configuration file and presents an interactive menu where you
can edit each section: server settings, database, routing, resilience, and
providers. Changes are saved back to the file.

Config search order:
1. `./tokenscavenger.toml` (current directory)
2. `~/.config/tokenscavenger/tokenscavenger.toml`
3. `~/.tokenscavenger.toml`

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
- Config — view and edit current effective configuration
- Audit — configuration change history

Config changes made through the web UI take effect immediately without restarting
the application. Server bind address, routing policy, resilience settings, and
provider credentials can all be modified at runtime. Changes are persisted to a
sidecar overrides file so they survive restarts.

## Releases

New releases are created from the GitHub Actions workflow dispatch menu:

1. Navigate to **Actions → Release** in the GitHub repository
2. Click **Run workflow**
3. Choose the version bump type: `patch` (1.0.0 → 1.0.1), `minor` (1.0.0 → 1.1.0),
   or `major` (1.0.0 → 2.0.0)
4. Click **Run workflow**

The workflow:

- Bumps the version in `Cargo.toml` and creates a git tag (`vX.Y.Z`)
- Cross-compiles binaries for Linux (x86\_64), macOS (ARM64), and Windows (x86\_64)
- Creates a GitHub release with all binaries and checksums attached
- Generates release notes from commit history

Each release binary is self-contained — download the one for your platform and run
it. On first execution the built-in setup wizard guides you through configuration.

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
