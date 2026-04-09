-- V005: Loss events table for recording incidents associated with assets

CREATE TABLE loss_events (
    event_id TEXT PRIMARY KEY,
    asset_id TEXT NOT NULL REFERENCES assets(asset_id),
    event_type TEXT NOT NULL CHECK (event_type IN ('fire', 'flood', 'wind', 'theft', 'other')),
    event_date TEXT NOT NULL,
    severity TEXT NOT NULL CHECK (severity IN ('minor', 'moderate', 'major', 'catastrophic')),
    description TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    created_by TEXT NOT NULL
);
CREATE INDEX idx_loss_events_asset ON loss_events(asset_id, event_date DESC);
