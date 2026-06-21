-- Durable request traces for operator observability and incident diagnosis.
CREATE TABLE IF NOT EXISTS request_trace_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    request_id TEXT NOT NULL,
    recorded_at TEXT NOT NULL DEFAULT (datetime('now')),
    event_type TEXT NOT NULL,
    provider_id TEXT,
    model_id TEXT,
    outcome TEXT,
    latency_ms INTEGER,
    details_json TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_request_trace_events_request ON request_trace_events(request_id, id);
CREATE INDEX IF NOT EXISTS idx_request_trace_events_recorded ON request_trace_events(recorded_at);
CREATE INDEX IF NOT EXISTS idx_request_trace_events_provider ON request_trace_events(provider_id);
CREATE INDEX IF NOT EXISTS idx_request_trace_events_outcome ON request_trace_events(outcome);
