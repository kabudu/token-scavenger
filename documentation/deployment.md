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

The repository includes a `Dockerfile`. A minimal production-oriented version looks like this:

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
docker build -t tokenscavenger:0.3.5 .
docker run -d \
  --name tokenscavenger \
  -p 8000:8000 \
  -v /path/to/config:/tokenscavenger.toml \
  -v /path/to/data:/data \
  -e GROQ_API_KEY=... \
  -e GEMINI_API_KEY=... \
  tokenscavenger:0.3.5
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
allowed_cors_origins = ["https://proxy.example.com"]
allow_query_api_keys = false     # Prefer Authorization: Bearer <key>
```

### External Identity

For teams, put TokenScavenger behind an identity-aware reverse proxy and map
provider groups into TokenScavenger roles. This keeps Google, GitHub, Microsoft,
and other OIDC providers outside the runtime while allowing the admin UI/API to
enforce local permissions.

```toml
[server]
bind = "127.0.0.1:8000"
master_api_key = "${PROXY_KEY}"

[server.external_identity]
enabled = true
read_only_groups = ["tokenscavenger-viewers"]
operator_groups = ["tokenscavenger-operators"]
config_editor_groups = ["tokenscavenger-editors"]
credential_manager_groups = ["tokenscavenger-credential-managers"]
admin_groups = ["tokenscavenger-admins"]
```

The reverse proxy must strip any incoming identity headers from clients before
setting its own trusted values. TokenScavenger expects these default headers:

- `x-auth-request-user`
- `x-auth-request-email`
- `x-auth-request-preferred-username`
- `x-auth-request-groups`

Use `GET /admin/whoami` after deployment to verify the resolved identity source,
subject, role, and credential-management permission.

### Credential Encryption

Enable encrypted runtime override storage when admins will enter provider keys
through the UI or CLI hot-reload flow:

```toml
[security.credential_encryption]
enabled = true
key_env = "TOKENSCAVENGER_CREDENTIAL_KEY"
```

Set `TOKENSCAVENGER_CREDENTIAL_KEY` in the service manager, container secret, or
Kubernetes Secret. Keep it stable and backed up; encrypted `*.overrides.toml`
files cannot be restored without the same key.

### Self-Update

Self-update is opt-in:

```toml
[updates]
enabled = true
github_repo = "kabudu/token-scavenger"
```

The admin UI checks `/admin/update/check` and displays an update CTA when a
newer GitHub release exists. Applying an update downloads the matching platform
asset, verifies it against `checksums.txt`, replaces the current executable, and
restarts TokenScavenger with the same command-line arguments. Use a process
manager such as systemd, launchd, Docker, or Kubernetes for additional restart
supervision.

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
        proxy_set_header X-Auth-Request-User $upstream_http_x_auth_request_user;
        proxy_set_header X-Auth-Request-Email $upstream_http_x_auth_request_email;
        proxy_set_header X-Auth-Request-Preferred-Username $upstream_http_x_auth_request_preferred_username;
        proxy_set_header X-Auth-Request-Groups $upstream_http_x_auth_request_groups;
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

Usage, health, audit, and request trace data are cleaned up by the background
retention task. Configure windows in days:

```toml
[retention]
usage_days = 30
health_event_days = 30
audit_days = 90
request_trace_days = 30
```

### Restore Drill

1. Stop TokenScavenger.
2. Copy the backup database into place.
3. Restore the matching config and overrides files.
4. Ensure `TOKENSCAVENGER_CREDENTIAL_KEY` matches the value used when encrypted overrides were created.
5. Start TokenScavenger and verify `/readyz`, `/admin/whoami`, `/admin/config`, and `/ui/observability`.

Migration rollback is file-based: restore the database backup taken before the
upgrade and run the previous binary with the same config. Runtime config
snapshots can also be rolled back through `/admin/config/rollback` when the
database itself is healthy.

## Monitoring

### Incident Workflow

The embedded UI includes `/ui/observability` for request traces, incident feed
review, and redacted diagnostic bundle export. The backing admin endpoints are:

- `GET /admin/observability/summary?period=24h`
- `GET /admin/request-traces?limit=50`
- `GET /admin/request-traces/{request_id}`
- `GET /admin/incidents?limit=50`
- `GET /admin/diagnostics/bundle`

Diagnostic bundles include runtime version, redacted effective config, recent
request traces, incidents, health states, and 24-hour observability summary.
They do not include provider API keys or full request/response bodies.

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

Ready-to-import monitoring starters live in `monitoring/`:

- `monitoring/grafana-dashboard.json`
- `monitoring/prometheus-alerts.yml`

Release artifacts also include `checksums.txt`, an SPDX SBOM, and GitHub
artifact attestations for provenance verification.

## Packaging

- Homebrew formula: `packaging/homebrew/tokenscavenger.rb`
- Kubernetes manifests: `deploy/kubernetes/`

The Homebrew formula points at the signed/notarized macOS ARM64 and Linux x86_64
release artifacts and pins their SHA256 checksums. The GitHub release workflow
updates `kabudu/homebrew-tap` after publishing a release by reading the generated
`checksums.txt` asset and committing the new formula to the tap's `master`
branch. That cross-repository push requires a repository secret named
`HOMEBREW_TAP_TOKEN` with `contents:write` access to `kabudu/homebrew-tap`.

The Kubernetes deployment references `tokenscavenger:0.3.5`, matching the local
Docker build tag above. For a remote cluster, retag and push that image to your
registry, then update `deploy/kubernetes/deployment.yaml` to the registry image
your cluster can pull.

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
