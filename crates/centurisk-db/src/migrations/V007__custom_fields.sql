-- V007: Custom field definitions per pool
-- Allows pool administrators to define additional fields beyond built-in ones.
-- Custom field values are stored as regular field_mutations (same as built-in fields).
-- Definitions tell the UI what to render and quality scoring what to check.

CREATE TABLE custom_field_definitions (
    field_id TEXT PRIMARY KEY,
    pool_id TEXT NOT NULL REFERENCES pools(pool_id),
    field_name TEXT NOT NULL,
    field_type TEXT NOT NULL CHECK (field_type IN ('Text', 'Number', 'Date', 'Boolean', 'Enum')),
    required BOOLEAN NOT NULL DEFAULT 0,
    recommended BOOLEAN NOT NULL DEFAULT 0,
    asset_types TEXT NOT NULL DEFAULT 'Building,Contents,Vehicle,FineArts',
    enum_options TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    created_by TEXT NOT NULL,
    UNIQUE(pool_id, field_name)
);
CREATE INDEX idx_custom_fields_pool ON custom_field_definitions(pool_id);
