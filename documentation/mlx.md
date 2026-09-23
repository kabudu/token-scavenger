# Running Local Models with MLX (Apple Silicon)

TokenScavenger routes to Apple Silicon MLX models through the built-in `mlx`
provider, which talks to an `mlx_lm.server` instance over its OpenAI-compatible
HTTP API. TokenScavenger itself never installs, downloads, starts, or stops
that server — you manage it, and TokenScavenger only detects its status
(`tokenscavenger mlx status`, the Providers view, `GET /admin/mlx/status`).

## Requirements

- An Apple Silicon Mac (MLX does not run on Intel Macs or other platforms).
- Python 3.10 or newer.
- Disk space for the model (the 27B distribution below downloads roughly 8 GB)
  and enough free unified memory to hold it plus context overhead.

## 1. Install `mlx-lm` into a virtual environment

Never install into your system Python. Create a project-local venv and install
there:

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install -U pip mlx-lm
```

Verify the install:

```bash
source .venv/bin/activate
python -c "import mlx_lm; print('mlx-lm ok')"
```

## 2. Serve the model

Serve PrismML's ternary 27B 2-bit distribution (first start downloads the
weights from Hugging Face, roughly 8 GB, so allow time):

```bash
source .venv/bin/activate
python -m mlx_lm server \
  --model prism-ml/Ternary-Bonsai-27B-mlx-2bit \
  --host 127.0.0.1 --port 8080
```

Confirm it answers:

```bash
curl http://127.0.0.1:8080/v1/models
```

Notes:

- Default port is `8080`, shared with the llama.cpp server preset. If you run
  both, give one a different `--port` and mirror it in TokenScavenger's
  `base_url` below.
- The server needs no API key. Leave TokenScavenger's `api_key` empty unless
  you front the server with your own auth.
- Memory guidance: the 2-bit weights occupy roughly 8 GB of unified memory,
  plus KV-cache headroom that grows with context length. A 16 GB Mac can run
  it with modest context; 24 GB or more is more comfortable for long chats.

## 3. Point TokenScavenger at it

```toml
[routing]
free_first = true
provider_order = ["mlx", "ollama", "local", "groq"]

[[providers]]
id = "mlx"
enabled = true
# base_url = "http://127.0.0.1:8080/v1"  # default; set only if the server uses another host/port
```

Restart TokenScavenger (or reload config). Discovery will pick up whatever
model id the server reports; the curated seed covers
`prism-ml/Ternary-Bonsai-27B-mlx-2bit` out of the box.

## 4. Check status

```bash
tokenscavenger mlx status
tokenscavenger mlx status --json
```

This reports platform support, whether `mlx_lm` is importable, and whether the
server answers at the configured `base_url` (plus the exact serve command to
run when it does not). The same information appears as a card in the
operator dashboard under Providers. All of it is read-only.

## Troubleshooting

| Symptom | Likely cause and fix |
|---------|----------------------|
| `mlx status` says runtime not installed | Activate the venv or fix `PATH`; re-run the install step. `mlx status` probes `python3` on `PATH`. |
| Server not reachable | `mlx_lm.server` is not running, or listens on another port — check `base_url` / `--port`. |
| `GET /v1/models` through TokenScavenger lacks the model | Discovery runs on a refresh interval; use Refresh Discovery in the UI, and confirm the server's own `/v1/models` lists it. |
| Port already in use on 8080 | Another local server (e.g. llama.cpp) owns the port. Move one of them and set `base_url` accordingly. |
| Slow first request | First start downloads ~8 GB of weights; subsequent starts load from the Hugging Face cache. |
