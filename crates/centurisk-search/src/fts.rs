//! FTS5-based full-text search index over asset fields.

use rusqlite::{params, Connection};
use std::collections::HashMap;

/// A search result with asset_id and matched snippets.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchResult {
    pub asset_id: String,
    pub asset_type: String,
    pub rank: f64,
    pub snippet: String,
    pub fields: HashMap<String, String>,
}

/// Manages the FTS5 search index alongside the main database.
pub struct SearchIndex;

impl SearchIndex {
    /// Create the FTS5 virtual table if it doesn't exist.
    pub fn ensure_table(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "CREATE VIRTUAL TABLE IF NOT EXISTS asset_search USING fts5(
                asset_id UNINDEXED,
                asset_type UNINDEXED,
                pool_id UNINDEXED,
                content,
                tokenize='porter unicode61'
            );"
        )
    }

    /// Rebuild the search index from current approved field values.
    pub fn rebuild(conn: &Connection) -> Result<usize, rusqlite::Error> {
        conn.execute("DELETE FROM asset_search", [])?;

        // Concatenate all approved field values per asset into a single content string
        let mut stmt = conn.prepare(
            "SELECT a.asset_id, a.asset_type, a.pool_id, fm.field_name, fm.value_json
             FROM assets a
             LEFT JOIN field_mutations fm ON fm.asset_id = a.asset_id AND fm.approval_state = 'Approved'
             ORDER BY a.asset_id, fm.field_name, fm.effective_date DESC"
        )?;

        let rows: Vec<(String, String, String, Option<String>, Option<String>)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)))?
            .filter_map(|r| r.ok())
            .collect();

        // Group by asset, deduplicate fields (latest wins)
        let mut assets: HashMap<String, (String, String, HashMap<String, String>)> = HashMap::new();
        for (aid, atype, pool_id, fname, vjson) in &rows {
            let entry = assets.entry(aid.clone()).or_insert_with(|| (atype.clone(), pool_id.clone(), HashMap::new()));
            if let (Some(f), Some(v)) = (fname, vjson) {
                if !entry.2.contains_key(f) {
                    if let Some(text) = extract_text(v) {
                        entry.2.insert(f.clone(), text);
                    }
                }
            }
        }

        let mut count = 0;
        for (asset_id, (asset_type, pool_id, fields)) in &assets {
            let content = fields.values().cloned().collect::<Vec<_>>().join(" ");
            if content.is_empty() { continue; }

            conn.execute(
                "INSERT INTO asset_search (asset_id, asset_type, pool_id, content) VALUES (?1, ?2, ?3, ?4)",
                params![asset_id, asset_type, pool_id, content],
            )?;
            count += 1;
        }

        Ok(count)
    }

    /// Search for assets matching a text query, optionally filtered by pool.
    pub fn search(
        conn: &Connection,
        query: &str,
        pool_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>, rusqlite::Error> {
        let fts_query = sanitize_fts_query(query);
        if fts_query.is_empty() {
            return Ok(vec![]);
        }

        let (sql, use_pool) = if pool_id.is_some() {
            (
                "SELECT asset_id, asset_type, rank, snippet(asset_search, 3, '<b>', '</b>', '...', 32)
                 FROM asset_search
                 WHERE asset_search MATCH ?1 AND pool_id = ?2
                 ORDER BY rank
                 LIMIT ?3",
                true,
            )
        } else {
            (
                "SELECT asset_id, asset_type, rank, snippet(asset_search, 3, '<b>', '</b>', '...', 32)
                 FROM asset_search
                 WHERE asset_search MATCH ?1
                 ORDER BY rank
                 LIMIT ?2",
                false,
            )
        };

        let mut stmt = conn.prepare(sql)?;

        let results = if use_pool {
            stmt.query_map(params![fts_query, pool_id.unwrap(), limit as i64], |row| {
                Ok(SearchResult {
                    asset_id: row.get(0)?,
                    asset_type: row.get(1)?,
                    rank: row.get(2)?,
                    snippet: row.get(3)?,
                    fields: HashMap::new(),
                })
            })?.filter_map(|r| r.ok()).collect()
        } else {
            stmt.query_map(params![fts_query, limit as i64], |row| {
                Ok(SearchResult {
                    asset_id: row.get(0)?,
                    asset_type: row.get(1)?,
                    rank: row.get(2)?,
                    snippet: row.get(3)?,
                    fields: HashMap::new(),
                })
            })?.filter_map(|r| r.ok()).collect()
        };

        Ok(results)
    }
}

/// Extract displayable text from a FieldValue JSON string.
fn extract_text(json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    match v.get("type")?.as_str()? {
        "Text" | "Enum" => v.get("value")?.as_str().map(|s| s.to_string()),
        "Number" => Some(v.get("value")?.to_string()),
        "Bool" => Some(if v.get("value")?.as_bool()? { "yes" } else { "no" }.into()),
        "Money" => {
            let amount = v.get("value")?.get("amount")?.as_str()?;
            Some(format!("${}", amount))
        }
        _ => None,
    }
}

/// Sanitize a user query for FTS5 (escape special characters).
fn sanitize_fts_query(query: &str) -> String {
    // FTS5 special chars: AND OR NOT ( ) * " ^
    // For simple queries, wrap each word in double quotes for exact matching
    query
        .split_whitespace()
        .filter(|w| !w.is_empty())
        .map(|w| {
            let clean: String = w.chars().filter(|c| c.is_alphanumeric() || *c == '.' || *c == '-' || *c == '_' || *c == '$').collect();
            if clean.is_empty() { String::new() } else { format!("\"{}\"", clean) }
        })
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();

        // Create minimal asset tables
        conn.execute_batch(
            "CREATE TABLE pools (pool_id TEXT PRIMARY KEY, name TEXT NOT NULL, created_at TEXT DEFAULT '', created_by TEXT DEFAULT '');
             CREATE TABLE members (member_id TEXT PRIMARY KEY, pool_id TEXT NOT NULL, name TEXT NOT NULL, created_at TEXT DEFAULT '', created_by TEXT DEFAULT '');
             CREATE TABLE assets (asset_id TEXT PRIMARY KEY, pool_id TEXT NOT NULL, member_id TEXT NOT NULL, path TEXT NOT NULL, asset_type TEXT NOT NULL, lifecycle TEXT DEFAULT 'Active', created_at TEXT DEFAULT '', created_by TEXT DEFAULT '');
             CREATE TABLE field_mutations (mutation_id TEXT PRIMARY KEY, asset_id TEXT NOT NULL, field_name TEXT NOT NULL, value_json TEXT NOT NULL, effective_date TEXT NOT NULL, submitted_at TEXT DEFAULT '', submitted_by TEXT DEFAULT '', approval_state TEXT DEFAULT 'Approved');
             INSERT INTO pools VALUES ('p1', 'Pool A', '', '');
             INSERT INTO members VALUES ('m1', 'p1', 'Member 1', '', '');
             INSERT INTO assets VALUES ('a1', 'p1', 'm1', '/p1/m1/a1', 'Building', 'Active', '', '');
             INSERT INTO assets VALUES ('a2', 'p1', 'm1', '/p1/m1/a2', 'LicensedVehicle', 'Active', '', '');
             INSERT INTO field_mutations VALUES ('fm1', 'a1', 'building_name', '{\"type\":\"Text\",\"value\":\"Fire Station #7\"}', '2024-01-01', '', '', 'Approved');
             INSERT INTO field_mutations VALUES ('fm2', 'a1', 'address', '{\"type\":\"Text\",\"value\":\"123 Main St Springfield\"}', '2024-01-01', '', '', 'Approved');
             INSERT INTO field_mutations VALUES ('fm3', 'a1', 'replacement_cost', '{\"type\":\"Money\",\"value\":{\"amount\":\"1500000\",\"currency\":\"USD\"}}', '2024-01-01', '', '', 'Approved');
             INSERT INTO field_mutations VALUES ('fm4', 'a2', 'building_name', '{\"type\":\"Text\",\"value\":\"Engine 42\"}', '2024-01-01', '', '', 'Approved');"
        ).unwrap();

        SearchIndex::ensure_table(&conn).unwrap();
        SearchIndex::rebuild(&conn).unwrap();
        conn
    }

    #[test]
    fn search_finds_by_name() {
        let conn = setup_test_db();
        let results = SearchIndex::search(&conn, "fire station", None, 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].asset_id, "a1");
    }

    #[test]
    fn search_finds_by_address() {
        let conn = setup_test_db();
        let results = SearchIndex::search(&conn, "Springfield", None, 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].asset_id, "a1");
    }

    #[test]
    fn search_finds_vehicle() {
        let conn = setup_test_db();
        let results = SearchIndex::search(&conn, "Engine", None, 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].asset_type, "LicensedVehicle");
    }

    #[test]
    fn search_returns_empty_for_no_match() {
        let conn = setup_test_db();
        let results = SearchIndex::search(&conn, "nonexistent", None, 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn search_filtered_by_pool() {
        let conn = setup_test_db();
        let results = SearchIndex::search(&conn, "fire", Some("p1"), 10).unwrap();
        assert_eq!(results.len(), 1);

        let results = SearchIndex::search(&conn, "fire", Some("p_other"), 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn rebuild_indexes_correct_count() {
        let conn = setup_test_db();
        // Already rebuilt in setup, verify count
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM asset_search", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 2); // a1 and a2
    }

    #[test]
    fn sanitize_fts_handles_special_chars() {
        assert_eq!(sanitize_fts_query("hello world"), "\"hello\" \"world\"");
        assert_eq!(sanitize_fts_query("fire AND station"), "\"fire\" \"AND\" \"station\"");
        assert_eq!(sanitize_fts_query(""), "");
        assert_eq!(sanitize_fts_query("$5M"), "\"$5M\"");
    }
}
