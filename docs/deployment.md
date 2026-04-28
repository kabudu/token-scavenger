# Deployment Guide

This guide covers production deployment scenarios for TokenScavenger.

## System Requirements

| Resource | Minimum | Recommended |
|----------|---------|-------------|
| CPU | 1 core | 2+ cores |
| RAM | 128 MB | 512 MB |
| Disk | 50 MB (binary + DB) | 1 GB (with logs) |
| OS | Linux (any), macOS, Windows | Linux x86_64 or aarch64 |

## Build Options

### Static binary (Linux)

Build a fully static binary for maximum portability:

```bash
# Install musl target
rustup target add x86_64-unknown-linux-musl

# Build
cargo build --release --target x86_64-unknown-linux-musl
```

The binary is at `target/x86_64-unknown-linux-musl/release/tokenscavenger`.

### Cross-compilation for ARM

```bash
# For Raspberry Pi / ARM servers
rustup target add aarch64-unknown-linux-musl
cargo build --release --target aarch64-unknown-linux-musl
```

### macOS

```bash
cargo build --release
# Binary: target/release/tokenscavenger
```

## Docker Deployment

### Using the Dockerfile

Create a `Dockerfile` in the project root:

```dockerfile
FROM rust:1.85 AS builder
WORKDIR /app
COPY . .
RUN cargo build --release --target x86_64-unknown-linux-musl

FROM scratch
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/tokenscavenger /
COPY --from=builder /app/tokenscavenger.toml /
EXPOSE 8000
ENTRYPOINT ["/tokenscavenger"]
CMD ["-c", "/tokenscavenger.toml"]
```

Build and run:

```bash
docker build -t tokenscavenger .
docker run -d \
  --name tokenscavenger \
  -p 8000:8000 \
  -v /path/to/config:/tokenscavenger.toml \
  -v /path/to/data:/data \
  -e GROQ_API_KEY=... \
  -e GEMINI_API_KEY=... \
  tokenscavenger
```

Or use Docker Compose:

```yaml
version: '3.8'
services:
  tokenscavenger:
    build: .
    ports:
      - "8000:8000"
    volumes:
      - ./tokenscavenger.toml:/tokenscavenger.toml
      - ./data:/data
    environment:
      - GROQ_API_KEY=${GROQ_API_KEY}
      - GEMINI_API_KEY=${GEMINI_API_KEY}
    restart: unless-stopped
```

## systemd Service

Create `/etc/systemd/system/tokenscavenger.service`:

```ini
[Unit]
Description=TokenScavenger LLM Proxy
After=network.target

[Service]
Type=simple
User=tokenscavenger
Group=tokenscavenger
WorkingDirectory=/opt/tokenscavenger
ExecStart=/usr/local/bin/tokenscavenger -c /opt/tokenscavenger/tokenscavenger.toml
Restart=on-failure
RestartSec=5

Environment=GROQ_API_KEY=...
Environment=GEMINI_API_KEY=...

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
sudo systemctl daemon-reload
sudo systemctl enable tokenscavenger
sudo systemctl start tokenscavenger
sudo systemctl status tokenscavenger
```

## Production Configuration

### Security

```toml
[server]
bind = "127.0.0.1:8000"          # Bind to localhost behind a reverse proxy
master_api_key = "${PROXY_KEY}"  # Require auth for all API requests
```

### Reverse Proxy (Nginx)

```nginx
server {
    listen 443 ssl;
    server_name proxy.example.com;

    ssl_certificate /etc/ssl/certs/example.crt;
    ssl_certificate_key /etc/ssl/private/example.key;

    location / {
        proxy_pass http://127.0.0.1:8000;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_read_timeout 300s;
        proxy_buffering off;  # Required for streaming
    }

    location /metrics {
        deny all;  # Protect metrics endpoint
    }
}
```

### Prometheus Scraping

Add to your `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: 'tokenscavenger'
    static_configs:
      - targets: ['localhost:8000']
    metrics_path: '/metrics'
```

### Health Checks

Configure your orchestrator to use these endpoints:

- **Liveness**: `GET /healthz` — returns `200 OK` with body `ok`
- **Readiness**: `GET /readyz` — returns `200 OK` with JSON status when providers are configured

## Database

### Backup

The SQLite database is a single file. Backup with:

```bash
sqlite3 tokenscavenger.db ".backup 'backup-$(date +%Y%m%d).db'"
```

### Retention

Usage and health event data accumulates over time. The default schema includes timestamp fields for implementing retention cleanup. Configure retention via config in a future release.

## Monitoring

### Prometheus Metrics

Available at `GET /metrics`:

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `tokenscavenger_requests_total` | Counter | provider, model, endpoint, status | Total proxy requests |
| `tokenscavenger_request_latency_seconds` | Histogram | provider, endpoint | Request latency distribution |
| `tokenscavenger_tokens_total` | Counter | provider, model, type | Token usage (input/output) |
| `tokenscavenger_route_attempts_total` | Counter | provider, model, outcome | Route attempt outcomes |
| `tokenscavenger_provider_health_state` | Gauge | provider, state | Current health state per provider |
| `tokenscavenger_provider_breaker_state` | Gauge | provider, state | Circuit breaker state per provider |

### Logging

Logs are structured JSON by default, suitable for ingestion by Loki, ELK, or Datadog:

```json
{
  "timestamp": "2026-04-28T22:35:00Z",
  "level": "INFO",
  "request_id": "ts-abc123",
  "provider_id": "groq",
  "model": "llama3-70b-8192",
  "latency_ms": 342,
  "http_status": 200,
  "fields": { "message": "Chat completion succeeded" }
}
```

## Upgrading

1. Download the new binary
2. Stop the service: `sudo systemctl stop tokenscavenger`
3. Replace the binary: `cp tokenscavenger /usr/local/bin/tokenscavenger`
4. Start the service: `sudo systemctl start tokenscavenger`
5. Verify: `curl http://localhost:8000/healthz`

Database migrations run automatically on startup. Always back up your database before upgrading.
