-- V003: System admin user — exists independent of any pool.
-- This is the CentuRisk operator account, not pool data.

INSERT OR IGNORE INTO users (user_id, email, display_name, category, pool_id, member_id)
VALUES ('00000000-0000-0000-0000-000000000001', 'admin@centurisk.dev', 'CentuRisk Admin', 'CentuRiskAdmin', NULL, NULL);
