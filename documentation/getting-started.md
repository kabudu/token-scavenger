# Getting Started with TokenScavenger

This guide walks you through your first TokenScavenger deployment from scratch.

## Prerequisites

- **Rust 1.85+** (edition 2024). Install via [rustup](https://rustup.rs/):
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  rustup default stable
  ```
- **At least one free-tier API key** from a supported provider.

## Step 1: Install

### Download Binary (Recommended)

1. Download the latest release for your platform:
   - **macOS**: `tokenscavenger-macos-arm64` (M1/M2/M3) or `x86_64` (Intel)
   - **Linux**: `tokenscavenger-linux-x86_64` (Static binary)
   - **Windows**: `tokenscavenger-windows-x64.exe`

2. Make it executable and run the setup wizard:
   ```bash
   chmod +x tokenscavenger
   ./tokenscavenger
   ```

### From source

If you prefer to build it yourself, ensure you have the Rust toolchain installed:

```bash
git clone https://github.com/kabudu/token-scavenger.git
cd token-scavenger
cargo build --release
```

The binary will be available at `./target/release/tokenscavenger`.

### Using Docker (optional)

```bash
docker build -t tokenscavenger .
docker run -p 8000:8000 -v $(pwd)/config:/config tokenscavenger -c /config/tokenscavenger.toml
```

## Step 2: Get API Keys

Sign up for accounts at these providers and generate an API key:

| Provider          | URL                                    | Free Tier Limit                |
| ----------------- | -------------------------------------- | ------------------------------ |
| **Groq**          | https://console.groq.com/              | Rate-limited free tier         |
| **Google Gemini** | https://aistudio.google.com/           | 60 requests/minute             |
| **OpenRouter**    | https://openrouter.ai/keys             | Free models via `:free` suffix |
| **Cerebras**      | https://cloud.cerebras.ai/             | 30 RPM per model               |
| **Mistral AI**    | https://console.mistral.ai/            | Experiment plan, reduced rates |
| **NVIDIA NIM**    | https://build.nvidia.com/              | Rate-limited free tier         |
| **GitHub Models** | https://github.com/settings/tokens     | 15 req/min, 150/day            |
| **HuggingFace**   | https://huggingface.co/settings/tokens | 1,000 requests/day             |
| **SiliconFlow**   | https://cloud.siliconflow.cn/          | 1,000 RPM free models          |
| **Zhipu AI**      | https://open.bigmodel.cn/              | Free flash models              |
| **Cohere**        | https://dashboard.cohere.com/          | 1,000 calls/month trial        |
| **Cloudflare**    | https://dash.cloudflare.com/           | 10,000 neurons/day             |
| **DeepSeek**      | https://platform.deepseek.com/         | Paid fallback                  |
| **xAI Grok**      | https://console.x.ai/                  | Paid fallback                  |

Set them as environment variables:

```bash
export GROQ_API_KEY="gsk_..."
export GEMINI_API_KEY="AIza..."
export OPENROUTER_API_KEY="sk-or-..."
export DEEPSEEK_API_KEY="sk-..."
export XAI_API_KEY="xai-..."
```

## Step 3: Configure

Create `tokenscavenger.toml`:

```toml
[server]
bind = "0.0.0.0:8000"
# master_api_key = "optional-key-to-protect-the-proxy"

[database]
path = "tokenscavenger.db"

[logging]
level = "info"        # trace, debug, info, warn, error
format = "json"       # or "text"

[metrics]
enabled = true

[routing]
free_first = true
allow_paid_fallback = false

# Provider order defines the fallback chain
provider_order = ["groq", "cerebras", "google", "openrouter", "cloudflare",
                  "nvidia", "mistral", "github-models", "siliconflow",
                  "huggingface", "cohere", "zai", "deepseek", "xai"]

[resilience]
max_retries_per_provider = 2
breaker_failure_threshold = 3
breaker_cooldown_secs = 60

[[providers]]
id = "groq"
enabled = true
api_key = "${GROQ_API_KEY}"

[[providers]]
id = "google"
enabled = true
api_key = "${GEMINI_API_KEY}"

[[providers]]
id = "openrouter"
enabled = true
api_key = "${OPENROUTER_API_KEY}"

# Optional paid fallback providers. They are ignored unless
# [routing].allow_paid_fallback is true.
[[providers]]
id = "deepseek"
enabled = true
api_key = "${DEEPSEEK_API_KEY}"
free_only = false

[[providers]]
id = "xai"
enabled = true
api_key = "${XAI_API_KEY}"
free_only = false
```

## Step 4: Start

```bash
# If using a pre-built binary:
./tokenscavenger -c tokenscavenger.toml

# If you built from source:
./target/release/tokenscavenger -c tokenscavenger.toml
```

You should see:

```
 INFO tokenscavenger: Config loaded from tokenscavenger.toml: server.bind=0.0.0.0:8000, providers=3
 INFO tokenscavenger: Database initialized at tokenscavenger.db
 INFO tokenscavenger: AppState created
 INFO tokenscavenger: TokenScavenger v0.1.2 starting on 0.0.0.0:8000
```

## Step 5: Test

### Health check

```bash
curl http://localhost:8000/healthz
# ok
```

### List models

```bash
curl http://localhost:8000/v1/models
```

### Chat completion

```bash
curl http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3-70b-8192",
    "messages": [{"role": "user", "content": "Hello!"}],
    "stream": false
  }'
```

### Response semantics

TokenScavenger returns OpenAI-shaped JSON errors, but preserves status codes that downstream clients can act on:

- `429 rate_limit_exceeded` means every viable route failed because of upstream rate limits or quota. Back off and honor `Retry-After` when present.
- `503 route_exhausted` means no viable non-rate-limited route remained, such as disabled providers, unhealthy providers, an open circuit breaker, unsupported features, or no model match.
- `400` and `401` usually require changing the request or credentials rather than retrying unchanged.

See [API Behavior](api-behavior.md) for the full contract.

### With streaming

```bash
curl -N http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3-70b-8192",
    "messages": [{"role": "user", "content": "Count to 5"}],
    "stream": true
  }'
```

### Operator dashboard

Open http://localhost:8000/ui in your browser.

## Using with OpenAI SDKs

### Python

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://localhost:8000/v1",
    api_key="optional-master-key"   # only if configured
)

# Non-streaming
response = client.chat.completions.create(
    model="llama3-70b-8192",
    messages=[{"role": "user", "content": "Hello!"}]
)
print(response.choices[0].message.content)

# Streaming
stream = client.chat.completions.create(
    model="gemini-2.0-flash",
    messages=[{"role": "user", "content": "Tell me a story"}],
    stream=True
)
for chunk in stream:
    if chunk.choices[0].delta.content:
        print(chunk.choices[0].delta.content, end="")
```

### Node.js

```javascript
import OpenAI from "openai";

const client = new OpenAI({
  baseURL: "http://localhost:8000/v1",
  apiKey: "optional-master-key",
});

const stream = await client.chat.completions.create({
  model: "llama3-70b-8192",
  messages: [{ role: "user", content: "Hello!" }],
  stream: true,
});
for await (const chunk of stream) {
  process.stdout.write(chunk.choices[0]?.delta?.content || "");
}
```

## Model Groups

TokenScavenger supports configurable model groups for simplified routing:

```toml
[[model_groups]]
name = "free:llama-70b"
target = ["groq/llama3-70b-8192", "google/gemini-2.0-flash"]

[[model_groups]]
name = "fast-chat"
target = ["cerebras/llama3.1-8b"]
```

Then use the group name in your request:

```json
{"model": "free:llama-70b", "messages": [...]}
```

### Mastering Model Groups

Model groups are powerful tools for creating stable, provider-agnostic endpoints for your applications. Unlike core provider settings, model groups are stored in the database and are best managed via the **Admin UI (Config > Model Group Editor)**.

#### Scenario: High-Availability Failover

If you want to ensure your "smart chat" always works even if a specific provider is down:

1. Create a model group named `smart-chat`.
2. Add multiple target models in order of preference:
   - `groq/llama3-70b-8192` (Fastest)
   - `cerebras/llama3.1-70b` (Alternative)
   - `google/gemini-1.5-pro` (Fallback)
3. Point your code to `model="smart-chat"`.
4. TokenScavenger will try them in the exact order you defined. If Groq is unhealthy or rate-limited, it automatically fails over to Cerebras, then Gemini.

#### Agentic Tool Calls

When a chat request includes OpenAI `tools`, TokenScavenger keeps the model
group as the eligibility boundary but automatically reprioritizes the remaining
attempts toward providers with stronger tool-call behavior. This helps agent
clients that expect real `tool_calls` and follow-up turns without requiring a
separate agent-only model group.

### The "Default Model" Pattern

You can create a model group literally named `default`. This allows you to point legacy scripts or simple integrations to TokenScavenger without specifying any model at all. If a request comes in with a missing or unrecognized model, TokenScavenger can be configured to use this `default` mapping.

### Hot-Reloading Configuration

Most settings—including model groups, model priority, and provider status—can be changed via the **Admin UI** while the service is running. Changes are applied immediately to new requests without needing a restart.

## Next Steps

- [Configuration Reference](configuration.md) — full config schema documentation
- [API Behavior](api-behavior.md) — endpoint coverage, error semantics, and retry/backoff rules
- [Provider Matrix](provider-matrix.md) — detailed provider capabilities and limits
- [Deployment Guide](deployment.md) — production deployment, Docker, systemd
- [API Reference](https://platform.openai.com/docs/api-reference/chat) — TokenScavenger follows the OpenAI API specification
