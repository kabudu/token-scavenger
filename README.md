<p align="center">
  <img src="resources/TokenScavengerLogo.png" alt="TokenScavenger Logo" width="200">
</p>

<h1 align="center">TokenScavenger</h1>

<p align="center">
  <a href="https://github.com/kabudu/token-scavenger/actions/workflows/ci.yml"><img src="https://github.com/kabudu/token-scavenger/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT License"></a>
</p>

**A lightweight, single-binary, self-hosted OpenAI-compatible LLM router that intelligently scavenges free-tier tokens first - with smart fallback to paid providers when you allow it.**

**Just change the `base_url` in your OpenAI SDK** and TokenScavenger handles the rest: provider credentials, routing logic, model discovery, circuit breakers, usage tracking, and a beautiful operator dashboard - all in one Rust binary backed by SQLite.

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

## Why TokenScavenger?

- **Maximize free inference** across 14 permanent free-tier providers without managing dozens of API endpoints.
- **Zero runtime overhead** — one static binary, no Docker or Python/Node required for basic use.
- **Production-grade resilience** — circuit breakers, health checks, retries, and full observability.
- **Beautiful built-in UI** — monitor usage, routing decisions, and provider health in real time.
- **Drop-in replacement** for OpenAI, LangChain, Vercel AI SDK, LlamaIndex, etc.

## Features

- **Free-tier-first routing** with configurable fallback chains
- **Full OpenAI-compatible API** (chat completions + streaming SSE, embeddings, `/v1/models`)
- **14 built-in providers** with automatic model discovery
- **Circuit breakers, retries & health monitoring**
- **Prometheus metrics** + per-provider token usage tracking
- **Embedded web UI** with live dashboard, logs, and config editor
- **SQLite persistence** (WAL mode) for usage accounting and audit log
- **Interactive setup wizard** and CLI tools
- **Single static binary** (~15–25 MB)

## Supported API Surface

| Endpoint                      | Purpose                                                     |
| ----------------------------- | ----------------------------------------------------------- |
| `POST /v1/chat/completions`   | OpenAI-compatible chat completions, including streaming SSE |
| `POST /v1/embeddings`         | OpenAI-compatible embeddings where supported upstream       |
| `GET /v1/models`              | Merged public model catalog                                 |
| `GET /healthz`, `GET /readyz` | Health and readiness probes                                 |
| `GET /metrics`                | Prometheus metrics                                          |
| `GET /ui`                     | Embedded operator dashboard                                 |

Error responses use the OpenAI-style `{"error": ...}` envelope. Upstream rate-limit exhaustion returns `429` with `rate_limit_exceeded` and `Retry-After` when known; non-rate-limit route exhaustion remains `503 route_exhausted`. See [API behavior](documentation/api-behavior.md) for the full status-code contract.

## Quick Start

### 1. Download the latest release

The simplest way to run TokenScavenger is to download the prebuilt binary for
your platform from the [latest GitHub release](https://github.com/kabudu/token-scavenger/releases/latest).

Each release includes self-contained binaries and SHA256 checksums. Download the
matching artifact, make it executable on Linux/macOS, and start it:

```bash
chmod +x tokenscavenger-*
./tokenscavenger-*
```

On first run, TokenScavenger detects the absence of a config file and offers to
run the interactive setup wizard. Follow the prompts to configure your server,
providers, and API keys. The wizard writes a configuration to
`~/.config/tokenscavenger/tokenscavenger.toml`.

To use an existing config file:

```bash
./tokenscavenger-* -c tokenscavenger.toml
```

### 2. Get API keys

Sign up for API keys from your preferred providers:

| Provider      | Sign Up                                       |
| ------------- | --------------------------------------------- |
| Groq          | https://console.groq.com/                     |
| Google Gemini | https://aistudio.google.com/                  |
| OpenRouter    | https://openrouter.ai/                        |
| Cerebras      | https://inference-docs.cerebras.ai/           |
| Mistral       | https://console.mistral.ai/                   |
| NVIDIA NIM    | https://build.nvidia.com/                     |
| Cloudflare    | https://developers.cloudflare.com/workers-ai/ |
| DeepSeek      | https://platform.deepseek.com/                |
| xAI Grok      | https://console.x.ai/                         |

### 3. Configure manually, if preferred

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

### 4. Other installation methods

You can also build from source:

```bash
cargo build --release
./target/release/tokenscavenger -c tokenscavenger.toml
```

See [documentation/deployment.md](documentation/deployment.md) for Docker,
systemd, reverse proxy, and cross-compilation options.

### 5. Use it

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://localhost:8000/v1",
    api_key="optional-master-key"
)

response = client.chat.completions.create(
    model="llama3-70b-8192",  # or any model group
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
  providers/    14 provider adapter implementations
  resilience/   Circuit breakers, health tracking, retry/backoff
  router/       Route planning engine, policy, model groups, fallback
  ui/           Embedded operator web UI (9 views)
  usage/        Usage accounting, aggregation, pricing
  util/         Secret redaction, time utilities
```

## CLI Commands

The `tokenscavenger` binary provides server, setup, configuration, and service-management modes:

| Command                            | Description                                                                             |
| ---------------------------------- | --------------------------------------------------------------------------------------- |
| `tokenscavenger` (no args)         | Starts the server. On first run, prompts to run the setup wizard if no config is found. |
| `tokenscavenger setup`             | Run the interactive first-time setup wizard.                                            |
| `tokenscavenger config`            | Edit an existing configuration file interactively.                                      |
| `tokenscavenger service install`   | Install TokenScavenger as a background service on supported platforms.                  |
| `tokenscavenger service uninstall` | Remove the installed background service on supported platforms.                         |

### `tokenscavenger setup`

Walks you through creating a configuration file from scratch — server bind address,
master API key, routing preferences, and provider credentials. The wizard stores
the resulting config at `~/.config/tokenscavenger/tokenscavenger.toml`.

### `tokenscavenger config`

Loads an existing configuration file and presents an interactive menu where you
can edit each section: server settings, database, routing, resilience, and
providers. Changes are saved back to the file.

### `tokenscavenger service install`

Installs TokenScavenger as a background service after a config file exists. Run
`tokenscavenger setup` first if this is a fresh machine.

On macOS, this creates and loads:

```text
~/Library/LaunchAgents/com.tokenscavenger.server.plist
```

On Linux, this prints the `systemd` commands needed to create, enable, and start
`/etc/systemd/system/tokenscavenger.service` with `sudo`.

```bash
tokenscavenger service install
```

### `tokenscavenger service uninstall`

Removes the macOS LaunchAgent when running on macOS. On Linux, it prints the
`systemd` commands needed to stop, disable, remove, and reload the service.

```bash
tokenscavenger service uninstall
```

Config search order:

1. `./tokenscavenger.toml` (current directory)
2. `~/.config/tokenscavenger/tokenscavenger.toml`
3. `~/.tokenscavenger.toml`

## Configuration Reference

See [documentation/configuration.md](documentation/configuration.md) for the full configuration schema.

## API Behavior

See [documentation/api-behavior.md](documentation/api-behavior.md) for endpoint coverage, error response semantics, `429` backoff behavior, `503 route_exhausted`, and streaming fallback rules.

## Provider Support Matrix

See [documentation/provider-matrix.md](documentation/provider-matrix.md) for details on each provider's API format, free tier limits, and known quirks.

## Roadmap

See [ROADMAP.md](ROADMAP.md) for five high-value future enhancements that can push TokenScavenger toward an operator-grade LLM traffic control plane.

## Deployment

See [documentation/deployment.md](documentation/deployment.md) for deployment options including Docker, systemd, and cross-compilation.

## Operator UI

Open `http://localhost:8000/ui` in your browser for the operator dashboard with views for:

- Dashboard — system status, uptime, provider count
- Providers — enable/disable, inspect health and breaker state
- Models — view discovered and curated models
- Routing — view fallback order and model group configuration
- Usage — token counts and estimated costs
- Health — per-provider health states
- Logs — real-time log stream via SSE
- Config — view and edit current effective configuration
- Audit — configuration change history

![Management Console](resources/ConsoleScreenshot.png)

Config changes made through the web UI take effect immediately without restarting
the application. Server bind address, routing policy, resilience settings, and
provider credentials can all be modified at runtime. Changes are persisted to a
sidecar overrides file so they survive restarts.

## Releases

New releases are created from the GitHub Actions workflow dispatch menu:

1. Navigate to **Actions → Release** in the GitHub repository
2. Click **Run workflow**
3. Choose `current` to release the version already in `Cargo.toml`, or choose
   `patch` (1.0.0 → 1.0.1), `minor` (1.0.0 → 1.1.0), or `major` (1.0.0 → 2.0.0)
   to bump before releasing.
4. Click **Run workflow**

The workflow:

- Uses the current `Cargo.toml` version or bumps it, then creates a git tag (`vX.Y.Z`)
- Cross-compiles binaries for Linux (x86_64), macOS (ARM64), and Windows (x86_64)
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
