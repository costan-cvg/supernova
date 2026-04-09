-- V004: Renewal workflow tables

CREATE TABLE renewals (
    renewal_id TEXT PRIMARY KEY,
    pool_id TEXT NOT NULL REFERENCES pools(pool_id),
    name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'Open' CHECK (status IN ('Open', 'InProgress', 'Completed')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    created_by TEXT NOT NULL
);
CREATE INDEX idx_renewals_pool ON renewals(pool_id);

-- Proposed values for each asset in a renewal
CREATE TABLE renewal_proposals (
    proposal_id TEXT PRIMARY KEY,
    renewal_id TEXT NOT NULL REFERENCES renewals(renewal_id),
    asset_id TEXT NOT NULL REFERENCES assets(asset_id),
    field_name TEXT NOT NULL,
    proposed_value TEXT NOT NULL,
    current_value TEXT,
    member_decision TEXT CHECK (member_decision IN ('Approved', 'Modified', 'Flagged')),
    decided_at TEXT,
    UNIQUE(renewal_id, asset_id, field_name)
);
CREATE INDEX idx_proposals_renewal ON renewal_proposals(renewal_id);
CREATE INDEX idx_proposals_asset ON renewal_proposals(asset_id);

-- Flags raised by members for discussion
CREATE TABLE renewal_flags (
    flag_id TEXT PRIMARY KEY,
    renewal_id TEXT NOT NULL REFERENCES renewals(renewal_id),
    asset_id TEXT NOT NULL REFERENCES assets(asset_id),
    field_name TEXT,
    member_note TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'Open' CHECK (state IN ('Open', 'Resolved')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    created_by TEXT NOT NULL,
    resolved_at TEXT,
    resolved_by TEXT
);
CREATE INDEX idx_flags_renewal ON renewal_flags(renewal_id);
