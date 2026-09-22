-- Additive accounting for subtask routing. Existing rows stay readable.
-- No persistent session-state table: affinity is process-local.

ALTER TABLE request_log ADD COLUMN session_digest TEXT;
ALTER TABLE request_log ADD COLUMN subtask_digest TEXT;
ALTER TABLE request_log ADD COLUMN task_phase TEXT;
ALTER TABLE request_log ADD COLUMN tier TEXT;
ALTER TABLE request_log ADD COLUMN selection_source TEXT;
ALTER TABLE request_log ADD COLUMN profile_revision TEXT;
ALTER TABLE request_log ADD COLUMN classifier_status TEXT;

ALTER TABLE usage_events ADD COLUMN purpose TEXT NOT NULL DEFAULT 'inference';
ALTER TABLE usage_events ADD COLUMN parent_request_id TEXT;
ALTER TABLE usage_events ADD COLUMN attempt_id TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_usage_events_attempt_id
    ON usage_events(attempt_id)
    WHERE attempt_id IS NOT NULL;

-- Crash-safe reservations for paid adaptive and classifier admission.
-- Unsettled retained rows are reloaded after restart and count against budgets.
CREATE TABLE IF NOT EXISTS spend_reservations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    request_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    purpose TEXT NOT NULL,
    scope_key TEXT NOT NULL,
    amount_micros INTEGER NOT NULL,
    state TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_spend_reservations_state ON spend_reservations(state);
CREATE INDEX IF NOT EXISTS idx_spend_reservations_request ON spend_reservations(request_id);
