//! Search API — NL query translation + FTS5 full-text search.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use centurisk_search::{SearchIndex, translate_query};
use serde::{Deserialize, Serialize};

use crate::auth::Auth;
use crate::AppState;

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
    pub limit: Option<usize>,
}

#[derive(Serialize)]
pub struct SearchResponse {
    pub query: centurisk_search::NlQuery,
    pub results: Vec<centurisk_search::fts::SearchResult>,
    pub total: usize,
}

/// GET /api/search?q=buildings+over+$5M — natural language search.
#[tracing::instrument(name = "api.search", skip_all)]
async fn search(
    Auth(principal): Auth,
    State(state): State<AppState>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<SearchResponse>, StatusCode> {
    let conn = state.db.get().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Ensure FTS table exists (cheap no-op if already created)
    if let Err(e) = SearchIndex::ensure_table(&conn) {
        tracing::warn!(error = %e, "FTS5 table creation failed — search unavailable");
        return Ok(Json(SearchResponse {
            query: translate_query(&params.q),
            results: vec![],
            total: 0,
        }));
    }

    // Only rebuild if the index is empty (first query after startup or after onboarding)
    let index_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM asset_search", [], |r| r.get(0))
        .unwrap_or(0);
    if index_count == 0 {
        let _ = SearchIndex::rebuild(&conn);
    }

    // Translate natural language to structured query
    let nl_query = translate_query(&params.q);
    let limit = params.limit.unwrap_or(50);

    // Determine pool scope
    let pool_id = principal.pool_id.map(|p| p.to_string());

    // FTS search
    let mut results = if let Some(ref text) = nl_query.search_text {
        SearchIndex::search(&conn, text, pool_id.as_deref(), limit)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    } else if nl_query.asset_type.is_some() || !nl_query.numeric_filters.is_empty() {
        // No text but have type/numeric filters — query all assets from the DB directly
        let pool_clause = pool_id.as_ref()
            .map(|_| "WHERE a.pool_id = ?1".to_string())
            .unwrap_or_default();
        let mut stmt = conn.prepare(&format!(
            "SELECT a.asset_id, a.asset_type FROM assets a {pool_clause} LIMIT ?{}",
            if pool_id.is_some() { "2" } else { "1" }
        )).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let all: Vec<centurisk_search::fts::SearchResult> = if let Some(ref pid) = pool_id {
            stmt.query_map(rusqlite::params![pid, (limit * 5) as i64], |row| {
                Ok(centurisk_search::fts::SearchResult {
                    asset_id: row.get(0)?, asset_type: row.get(1)?,
                    rank: 0.0, snippet: String::new(),
                    fields: std::collections::HashMap::new(),
                })
            }).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .filter_map(|r| r.ok()).collect()
        } else {
            stmt.query_map(rusqlite::params![(limit * 5) as i64], |row| {
                Ok(centurisk_search::fts::SearchResult {
                    asset_id: row.get(0)?, asset_type: row.get(1)?,
                    rank: 0.0, snippet: String::new(),
                    fields: std::collections::HashMap::new(),
                })
            }).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .filter_map(|r| r.ok()).collect()
        };
        all
    } else {
        vec![]
    };

    // Apply asset type filter
    if let Some(ref atype) = nl_query.asset_type {
        results.retain(|r| r.asset_type == *atype);
    }

    // Apply numeric filters (need to load field values for matching assets)
    if !nl_query.numeric_filters.is_empty() {
        let mut filtered = Vec::new();
        for result in results {
            let mut passes = true;
            for filter in &nl_query.numeric_filters {
                let val: Option<f64> = conn.query_row(
                    "SELECT value_json FROM field_mutations WHERE asset_id = ?1 AND field_name = ?2 AND approval_state = 'Approved' ORDER BY effective_date DESC LIMIT 1",
                    rusqlite::params![result.asset_id, filter.field],
                    |row| row.get::<_, String>(0),
                ).ok().and_then(|json| extract_numeric(&json));

                if let Some(v) = val {
                    let ok = match filter.op.as_str() {
                        ">" => v > filter.value,
                        "<" => v < filter.value,
                        ">=" => v >= filter.value,
                        "<=" => v <= filter.value,
                        "=" => (v - filter.value).abs() < 0.01,
                        _ => true,
                    };
                    if !ok { passes = false; break; }
                } else {
                    passes = false;
                    break;
                }
            }
            if passes { filtered.push(result); }
        }
        results = filtered;
    }

    results.truncate(limit);
    let total = results.len();

    Ok(Json(SearchResponse {
        query: nl_query,
        results,
        total,
    }))
}

fn extract_numeric(json: &str) -> Option<f64> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    match v.get("type")?.as_str()? {
        "Money" => v.get("value")?.get("amount")?.as_str()?.parse().ok(),
        "Number" => v.get("value")?.as_str().and_then(|s| s.parse().ok())
            .or_else(|| v.get("value")?.as_f64()),
        _ => None,
    }
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/search", get(search))
}
