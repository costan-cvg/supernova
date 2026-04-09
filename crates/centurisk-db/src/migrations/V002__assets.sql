-- V002: Asset registry and field mutations

CREATE TABLE assets (
    asset_id TEXT PRIMARY KEY,
    pool_id TEXT NOT NULL REFERENCES pools(pool_id),
    member_id TEXT NOT NULL REFERENCES members(member_id),
    path TEXT NOT NULL,
    asset_type TEXT NOT NULL,
    lifecycle TEXT NOT NULL DEFAULT 'Draft' CHECK (lifecycle IN ('Draft', 'Active', 'PendingChange', 'Archived')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    created_by TEXT NOT NULL
);
CREATE INDEX idx_assets_pool ON assets(pool_id);
CREATE INDEX idx_assets_member ON assets(pool_id, member_id);
CREATE INDEX idx_assets_path ON assets(path);

CREATE TABLE field_mutations (
    mutation_id TEXT PRIMARY KEY,
    asset_id TEXT NOT NULL REFERENCES assets(asset_id),
    field_name TEXT NOT NULL,
    value_json TEXT NOT NULL,
    effective_date TEXT NOT NULL,
    submitted_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    submitted_by TEXT NOT NULL,
    approved_at TEXT,
    approved_by TEXT,
    approval_state TEXT NOT NULL DEFAULT 'Pending' CHECK (approval_state IN ('Pending', 'Approved', 'Rejected'))
);
CREATE INDEX idx_mutations_asset ON field_mutations(asset_id, field_name, effective_date DESC);
