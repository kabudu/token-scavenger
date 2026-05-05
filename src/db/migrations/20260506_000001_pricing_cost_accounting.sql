-- Pricing metadata and richer cost accounting.

CREATE TABLE IF NOT EXISTS pricing_sources (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider_id TEXT NOT NULL,
    source_kind TEXT NOT NULL,
    source_url TEXT,
    etag TEXT,
    last_modified TEXT,
    last_checked_at TEXT,
    last_success_at TEXT,
    last_error_at TEXT,
    last_error_summary TEXT,
    status TEXT NOT NULL DEFAULT 'never_checked',
    UNIQUE(provider_id, source_kind, source_url)
);

CREATE TABLE IF NOT EXISTS model_pricing (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider_id TEXT NOT NULL,
    model_id TEXT NOT NULL,
    currency TEXT NOT NULL DEFAULT 'USD',
    input_per_1m REAL,
    cached_input_per_1m REAL,
    output_per_1m REAL,
    reasoning_per_1m REAL,
    request_per_1k REAL,
    effective_from TEXT,
    effective_until TEXT,
    source_kind TEXT NOT NULL,
    source_url TEXT,
    confidence TEXT NOT NULL,
    fetched_at TEXT,
    metadata_json TEXT
);

CREATE INDEX IF NOT EXISTS idx_pricing_sources_provider ON pricing_sources(provider_id);
CREATE INDEX IF NOT EXISTS idx_model_pricing_lookup ON model_pricing(provider_id, model_id, effective_until);
CREATE INDEX IF NOT EXISTS idx_model_pricing_source ON model_pricing(provider_id, source_kind);

ALTER TABLE usage_events ADD COLUMN cached_input_tokens INTEGER;
ALTER TABLE usage_events ADD COLUMN cache_miss_input_tokens INTEGER;
ALTER TABLE usage_events ADD COLUMN reasoning_tokens INTEGER;
ALTER TABLE usage_events ADD COLUMN pricing_model_id INTEGER REFERENCES model_pricing(id);
ALTER TABLE usage_events ADD COLUMN cost_formula_json TEXT;
ALTER TABLE usage_events ADD COLUMN cost_calculated_at TEXT;
