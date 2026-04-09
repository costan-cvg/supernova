-- V006: In-app notifications

CREATE TABLE notifications (
    notification_id TEXT PRIMARY KEY,
    recipient_user_id TEXT NOT NULL,
    pool_id TEXT NOT NULL,
    source_type TEXT NOT NULL,
    source_id TEXT,
    priority TEXT NOT NULL DEFAULT 'Normal' CHECK (priority IN ('Low', 'Normal', 'High', 'Urgent')),
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'Created' CHECK (state IN ('Created', 'Delivered', 'Acknowledged')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    acknowledged_at TEXT
);
CREATE INDEX idx_notifications_user ON notifications(recipient_user_id, state);
