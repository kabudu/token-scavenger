-- Project-scoped OpenAI-compatible client keys, budgets, and attribution.

CREATE TABLE IF NOT EXISTS projects (
    project_id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    description TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    organization_id TEXT,
    environment TEXT,
    owner_subject TEXT,
    owner_email TEXT,
    allowed_model_groups_json TEXT NOT NULL DEFAULT '[]',
    allow_paid_fallback INTEGER NOT NULL DEFAULT 0,
    provider_allowlist_json TEXT NOT NULL DEFAULT '[]',
    provider_denylist_json TEXT NOT NULL DEFAULT '[]',
    privacy_profile TEXT NOT NULL DEFAULT 'default',
    max_cost_per_request_usd REAL,
    max_cost_per_org_per_day_usd REAL,
    max_cost_per_environment_per_day_usd REAL,
    max_cost_per_day_usd REAL,
    max_requests_per_day INTEGER,
    max_input_tokens_per_day INTEGER,
    max_output_tokens_per_day INTEGER,
    sliding_window_seconds INTEGER,
    max_requests_per_window INTEGER,
    max_tokens_per_window INTEGER,
    webhook_url TEXT,
    webhook_events_json TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS project_api_keys (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id TEXT NOT NULL REFERENCES projects(project_id) ON DELETE CASCADE,
    label TEXT NOT NULL,
    owner_subject TEXT,
    key_prefix TEXT NOT NULL UNIQUE,
    key_hash TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT,
    rotation_grace_until TEXT,
    max_requests_per_day INTEGER,
    max_tokens_per_day INTEGER,
    max_cost_per_day_usd REAL,
    revoked_at TEXT,
    last_used_at TEXT
);

INSERT OR IGNORE INTO projects (
    project_id,
    display_name,
    description,
    enabled,
    allow_paid_fallback,
    privacy_profile
) VALUES (
    'default',
    'Default project',
    'Backfilled project for historical usage and master-key client traffic.',
    1,
    1,
    'default'
);

ALTER TABLE request_log ADD COLUMN project_id TEXT REFERENCES projects(project_id);
ALTER TABLE request_log ADD COLUMN api_key_prefix TEXT;

ALTER TABLE usage_events ADD COLUMN project_id TEXT REFERENCES projects(project_id);
ALTER TABLE usage_events ADD COLUMN api_key_prefix TEXT;

UPDATE request_log SET project_id = 'default', api_key_prefix = 'master' WHERE project_id IS NULL;
UPDATE usage_events SET project_id = 'default', api_key_prefix = 'master' WHERE project_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_projects_enabled ON projects(enabled);
CREATE INDEX IF NOT EXISTS idx_project_keys_project ON project_api_keys(project_id);
CREATE INDEX IF NOT EXISTS idx_project_keys_hash ON project_api_keys(key_hash);
CREATE INDEX IF NOT EXISTS idx_project_keys_prefix ON project_api_keys(key_prefix);
CREATE INDEX IF NOT EXISTS idx_request_log_project_received ON request_log(project_id, received_at);
CREATE INDEX IF NOT EXISTS idx_usage_events_project_timestamp ON usage_events(project_id, timestamp);
