-- Initial schema for TokenScavenger
-- Creates all core tables for configuration, providers, models, usage, and health tracking.

-- Config snapshots for versioned configuration history
CREATE TABLE IF NOT EXISTS config_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    version TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_by TEXT NOT NULL DEFAULT 'system',
    source TEXT NOT NULL DEFAULT 'file',
    config_json TEXT NOT NULL
);

-- Audit log for configuration changes
CREATE TABLE IF NOT EXISTS config_audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    actor TEXT NOT NULL DEFAULT 'system',
    action TEXT NOT NULL,
    target_type TEXT,
    target_id TEXT,
    before_json TEXT,
    after_json TEXT
);

-- Provider configuration and runtime state
CREATE TABLE IF NOT EXISTS providers (
    provider_id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    priority INTEGER NOT NULL DEFAULT 100,
    base_url TEXT,
    auth_kind TEXT NOT NULL DEFAULT 'header',
    free_only INTEGER NOT NULL DEFAULT 1,
    discovery_state TEXT NOT NULL DEFAULT 'never_discovered',
    last_discovery_at TEXT,
    last_success_at TEXT,
    last_error_at TEXT,
    last_error_summary TEXT
);

-- Model catalog (curated + discovered)
CREATE TABLE IF NOT EXISTS models (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider_id TEXT NOT NULL REFERENCES providers(provider_id),
    upstream_model_id TEXT NOT NULL,
    public_model_id TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    free_tier INTEGER NOT NULL DEFAULT 1,
    paid_fallback INTEGER NOT NULL DEFAULT 0,
    supports_chat INTEGER NOT NULL DEFAULT 1,
    supports_embeddings INTEGER NOT NULL DEFAULT 0,
    supports_streaming INTEGER NOT NULL DEFAULT 1,
    supports_tools INTEGER NOT NULL DEFAULT 1,
    supports_vision INTEGER NOT NULL DEFAULT 0,
    supports_json_mode INTEGER NOT NULL DEFAULT 1,
    priority INTEGER NOT NULL DEFAULT 100,
    metadata_json TEXT,
    discovered_at TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(provider_id, upstream_model_id)
);

-- Model aliases for routing
CREATE TABLE IF NOT EXISTS aliases (
    alias TEXT PRIMARY KEY,
    target_json TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Request log entries
CREATE TABLE IF NOT EXISTS request_log (
    request_id TEXT PRIMARY KEY,
    received_at TEXT NOT NULL DEFAULT (datetime('now')),
    endpoint_kind TEXT NOT NULL,
    caller_key_hash TEXT,
    requested_model TEXT,
    resolved_alias TEXT,
    selected_provider_id TEXT,
    selected_model_id TEXT,
    status TEXT NOT NULL,
    http_status INTEGER,
    latency_ms INTEGER,
    streaming INTEGER NOT NULL DEFAULT 0,
    retry_count INTEGER NOT NULL DEFAULT 0,
    fallback_count INTEGER NOT NULL DEFAULT 0,
    error_code TEXT,
    error_summary TEXT
);

-- Usage event accounting
CREATE TABLE IF NOT EXISTS usage_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    request_id TEXT NOT NULL REFERENCES request_log(request_id),
    provider_id TEXT NOT NULL,
    model_id TEXT,
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    estimated_cost_usd REAL NOT NULL DEFAULT 0.0,
    cost_confidence TEXT NOT NULL DEFAULT 'inferred',
    free_tier INTEGER NOT NULL DEFAULT 1
);

-- Provider health event history
CREATE TABLE IF NOT EXISTS provider_health_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider_id TEXT NOT NULL,
    recorded_at TEXT NOT NULL DEFAULT (datetime('now')),
    health_state TEXT NOT NULL,
    breaker_state TEXT NOT NULL DEFAULT 'closed',
    latency_ms INTEGER,
    status_code INTEGER,
    event_type TEXT NOT NULL,
    details_json TEXT
);

-- Discovery run tracking
CREATE TABLE IF NOT EXISTS discovery_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider_id TEXT NOT NULL,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    finished_at TEXT,
    status TEXT NOT NULL DEFAULT 'in_progress',
    models_found INTEGER NOT NULL DEFAULT 0,
    error_summary TEXT
);

-- Indexes for common query patterns
CREATE INDEX IF NOT EXISTS idx_models_provider ON models(provider_id);
CREATE INDEX IF NOT EXISTS idx_models_public_id ON models(public_model_id);
CREATE INDEX IF NOT EXISTS idx_models_enabled ON models(enabled);
CREATE INDEX IF NOT EXISTS idx_request_log_received ON request_log(received_at);
CREATE INDEX IF NOT EXISTS idx_request_log_provider ON request_log(selected_provider_id);
CREATE INDEX IF NOT EXISTS idx_request_log_status ON request_log(status);
CREATE INDEX IF NOT EXISTS idx_usage_events_timestamp ON usage_events(timestamp);
CREATE INDEX IF NOT EXISTS idx_usage_events_provider ON usage_events(provider_id);
CREATE INDEX IF NOT EXISTS idx_health_events_provider ON provider_health_events(provider_id);
CREATE INDEX IF NOT EXISTS idx_health_events_recorded ON provider_health_events(recorded_at);
CREATE INDEX IF NOT EXISTS idx_discovery_runs_provider ON discovery_runs(provider_id);
CREATE INDEX IF NOT EXISTS idx_audit_log_created ON config_audit_log(created_at);
