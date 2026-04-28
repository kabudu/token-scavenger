# Changelog

All notable changes to TokenScavenger will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-04-28

### Added

- **Core proxy engine**: OpenAI-compatible HTTP API with Axum + Tokio
- **12 provider adapters**: Groq, Google Gemini, OpenRouter, Cloudflare, Cerebras, NVIDIA NIM, Cohere, Mistral AI, GitHub Models, HuggingFace, Zhipu AI, SiliconFlow
- **Free-tier routing**: Automatic provider fallback with configurable ordering
- **SSE streaming**: Full OpenAI-compatible streaming chat completions
- **Circuit breakers**: Per-provider failure tracking with automatic recovery
- **Health monitoring**: Real-time provider health scoring
- **Retry logic**: Exponential backoff with jitter
- **Model discovery**: Dynamic model list fetching from provider endpoints
- **Curated catalog**: 15+ pre-configured free-tier models
- **Model aliases**: Configurable shorthand names for routing
- **Usage accounting**: Token tracking and cost estimation
- **Prometheus metrics**: Request counts, latency histograms, token usage, health states
- **Structured logging**: JSON-formatted logs with correlation IDs
- **Secret redaction**: Automatic masking of API keys in logs and UI
- **SQLite persistence**: WAL mode, 9-table schema with indexes
- **Database migrations**: Automatic schema upgrades on startup
- **Web UI**: 9-view operator dashboard (Dashboard, Providers, Models, Routing, Usage, Health, Logs, Config, Audit)
- **API auth**: Optional Bearer token authentication
- **CORS support**: Configurable cross-origin access
- **CLI interface**: `-c` for config path, `-d` for database override
- **Graceful shutdown**: Connection draining with configurable timeout
- **30 tests**: 18 unit + 12 integration tests, all passing

### Configuration

- TOML-based configuration with `${ENV_VAR}` expansion
- Full schema: server, database, logging, metrics, routing, resilience, providers, aliases

### Documentation

- README with quick start, architecture overview, SDK examples
- Getting started guide with step-by-step walkthrough
- Configuration reference with all fields documented
- Provider support matrix with 12 providers detailed
- Deployment guide for Docker, systemd, reverse proxy, monitoring
- Contributing guide and code of conduct
