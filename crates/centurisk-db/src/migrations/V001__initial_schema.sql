-- V001: Initial schema for CentuRisk RMIS
-- Tables: pools, members, users, access_grants, audit_entries

CREATE TABLE pools (
    pool_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    created_by TEXT NOT NULL
);

CREATE TABLE members (
    member_id TEXT PRIMARY KEY,
    pool_id TEXT NOT NULL REFERENCES pools(pool_id),
    name TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    created_by TEXT NOT NULL
);
CREATE INDEX idx_members_pool ON members(pool_id);

CREATE TABLE users (
    user_id TEXT PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    category TEXT NOT NULL CHECK (category IN (
        'CentuRiskAdmin', 'CentuRiskAnalyst', 'CentuRiskAuditor', 'CentuRiskSupport',
        'PoolAdministrator', 'PoolAnalyst',
        'MemberAdmin', 'MemberUser', 'MemberReadOnly',
        'PoolReadOnly'
    )),
    pool_id TEXT REFERENCES pools(pool_id),
    member_id TEXT REFERENCES members(member_id),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE access_grants (
    grant_id TEXT PRIMARY KEY,
    member_id TEXT NOT NULL REFERENCES members(member_id),
    pool_id TEXT NOT NULL REFERENCES pools(pool_id),
    granted_by TEXT NOT NULL REFERENCES users(user_id),
    granted_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    effective_from TEXT NOT NULL,
    effective_to TEXT,
    revoked_at TEXT,
    revoke_reason TEXT
);
CREATE INDEX idx_grants_member ON access_grants(member_id);
CREATE INDEX idx_grants_pool ON access_grants(pool_id);

CREATE TABLE audit_entries (
    entry_id TEXT PRIMARY KEY,
    entity_id TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    field_name TEXT,
    old_value TEXT,
    new_value TEXT,
    effective_date TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    actor_role TEXT NOT NULL,
    pool_id TEXT NOT NULL,
    timestamp TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    operation TEXT NOT NULL CHECK (operation IN ('Create', 'Update', 'Archive', 'Restore'))
);
CREATE INDEX idx_audit_entity ON audit_entries(entity_id, entity_type);
CREATE INDEX idx_audit_pool ON audit_entries(pool_id);
CREATE INDEX idx_audit_timestamp ON audit_entries(timestamp);
