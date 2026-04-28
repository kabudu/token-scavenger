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

### From source

```bash
git clone https://github.com/your-org/token-scavenger.git
cd token-scavenger
cargo build --release
```

The binary will be at `./target/release/tokenscavenger`.

### Using Docker (optional)

```bash
docker build -t tokenscavenger .
docker run -p 8000:8000 -v $(pwd)/config:/config tokenscavenger -c /config/tokenscavenger.toml
```

## Step 2: Get API Keys

Sign up for free accounts at these providers and generate an API key:

| Provider | URL | Free Tier Limit |
|----------|-----|----------------|
| **Groq** | https://console.groq.com/ | Rate-limited free tier |
| **Google Gemini** | https://aistudio.google.com/ | 60 requests/minute |
| **OpenRouter** | https://openrouter.ai/keys | Free models via `:free` suffix |
| **Cerebras** | https://cloud.cerebras.ai/ | 30 RPM per model |
| **Mistral AI** | https://console.mistral.ai/ | Experiment plan, reduced rates |
| **NVIDIA NIM** | https://build.nvidia.com/ | Rate-limited free tier |
| **GitHub Models** | https://github.com/settings/tokens | 15 req/min, 150/day |
| **HuggingFace** | https://huggingface.co/settings/tokens | 1,000 requests/day |
| **SiliconFlow** | https://cloud.siliconflow.cn/ | 1,000 RPM free models |
| **Zhipu AI** | https://open.bigmodel.cn/ | Free flash models |
| **Cohere** | https://dashboard.cohere.com/ | 1,000 calls/month trial |
| **Cloudflare** | https://dash.cloudflare.com/ | 10,000 neurons/day |

Set them as environment variables:

```bash
export GROQ_API_KEY="gsk_..."
export GEMINI_API_KEY="AIza..."
export OPENROUTER_API_KEY="sk-or-..."
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
                  "huggingface", "cohere", "zai"]

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
```

## Step 4: Start

```bash
./target/release/tokenscavenger -c tokenscavenger.toml
```

You should see:

```
 INFO tokenscavenger: Config loaded from tokenscavenger.toml: server.bind=0.0.0.0:8000, providers=3
 INFO tokenscavenger: Database initialized at tokenscavenger.db
 INFO tokenscavenger: AppState created
 INFO tokenscavenger: TokenScavenger v0.1.0 starting on 0.0.0.0:8000
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
import OpenAI from 'openai';

const client = new OpenAI({
  baseURL: 'http://localhost:8000/v1',
  apiKey: 'optional-master-key'
});

const stream = await client.chat.completions.create({
  model: 'llama3-70b-8192',
  messages: [{ role: 'user', content: 'Hello!' }],
  stream: true,
});
for await (const chunk of stream) {
  process.stdout.write(chunk.choices[0]?.delta?.content || '');
}
```

## Model Aliases

TokenScavenger supports configurable model aliases for simplified routing:

```toml
[[aliases]]
alias = "free:llama-70b"
target = ["groq/llama3-70b-8192", "google/gemini-2.0-flash"]

[[aliases]]
alias = "fast-chat"
target = ["cerebras/llama3.1-8b"]
```

Then use the alias in your request:

```json
{"model": "free:llama-70b", "messages": [...]}
```

## Next Steps

- [Configuration Reference](configuration.md) — full config schema documentation
- [Provider Matrix](provider-matrix.md) — detailed provider capabilities and limits
- [Deployment Guide](deployment.md) — production deployment, Docker, systemd
- [API Reference](https://platform.openai.com/docs/api-reference/chat) — TokenScavenger follows the OpenAI API specification
