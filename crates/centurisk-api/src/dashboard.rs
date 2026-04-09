//! Dashboard endpoints — TIV aggregation, portfolio quality, member overview.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::assets::tenant_where;
use crate::auth::Auth;
use crate::AppState;

// ── TIV Aggregation ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct TivQuery {
    /// Group by: "city", "state", "zip_code", "construction_class", "occupancy", "asset_type"
    pub group_by: Option<String>,
}

#[derive(Serialize)]
pub struct TivBucket {
    pub label: String,
    pub asset_count: usize,
    pub total_tiv: f64,
}

#[derive(Serialize)]
pub struct TivSummary {
    pub group_by: String,
    pub total_tiv: f64,
    pub total_assets: usize,
    pub buckets: Vec<TivBucket>,
}

/// GET /api/dashboard/tiv — TIV accumulation grouped by a dimension.
async fn tiv_aggregation(
    Auth(principal): Auth,
    State(state): State<AppState>,
    Query(params): Query<TivQuery>,
) -> Result<Json<TivSummary>, StatusCode> {
    let conn = state.db.get().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let (tenant_clause, tenant_params) = tenant_where(&principal);
    // Whitelist allowed group_by values to prevent SQL injection
    let group_by = match params.group_by.as_deref().unwrap_or("city") {
        "city" | "state" | "zip_code" | "construction_class" | "occupancy" | "asset_type" => {
            params.group_by.as_deref().unwrap_or("city")
        }
        _ => "city",
    };

    // Inject field name directly (safe — whitelisted above)
    let query = format!(
        "SELECT a.asset_id, a.asset_type,
                (SELECT fm.value_json FROM field_mutations fm
                 WHERE fm.asset_id = a.asset_id AND fm.field_name = 'replacement_cost'
                   AND fm.approval_state = 'Approved'
                 ORDER BY fm.effective_date DESC LIMIT 1) as cost_json,
                (SELECT fm.value_json FROM field_mutations fm
                 WHERE fm.asset_id = a.asset_id AND fm.field_name = '{group_by}'
                   AND fm.approval_state = 'Approved'
                 ORDER BY fm.effective_date DESC LIMIT 1) as group_json
         FROM assets a
         WHERE {tenant_clause}"
    );

    let all_params = tenant_params;

    let mut stmt = conn.prepare(&query).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    type Row = (String, String, Option<String>, Option<String>);
    let rows: Vec<Row> = stmt
        .query_map(rusqlite::params_from_iter(&all_params), |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .filter_map(|r| r.ok())
        .collect();

    let mut buckets_map: HashMap<String, (usize, f64)> = HashMap::new();
    let mut total_tiv = 0.0;

    for (_aid, asset_type, cost_json, group_json) in &rows {
        let cost = cost_json.as_deref()
            .and_then(|j| extract_money_amount(j))
            .unwrap_or(0.0);

        let label = if group_by == "asset_type" {
            asset_type.clone()
        } else {
            group_json.as_deref()
                .and_then(|j| extract_text_value(j))
                .unwrap_or_else(|| "Unknown".into())
        };

        let entry = buckets_map.entry(label).or_insert((0, 0.0));
        entry.0 += 1;
        entry.1 += cost;
        total_tiv += cost;
    }

    let mut buckets: Vec<TivBucket> = buckets_map.into_iter()
        .map(|(label, (count, tiv))| TivBucket { label, asset_count: count, total_tiv: tiv })
        .collect();
    buckets.sort_by(|a, b| b.total_tiv.partial_cmp(&a.total_tiv).unwrap());

    Ok(Json(TivSummary {
        group_by: group_by.into(),
        total_tiv,
        total_assets: rows.len(),
        buckets,
    }))
}

fn extract_money_amount(json: &str) -> Option<f64> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    if v.get("type")?.as_str()? == "Money" {
        v.get("value")?.get("amount")?.as_str()?.parse().ok()
    } else {
        None
    }
}

fn extract_text_value(json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    match v.get("type")?.as_str()? {
        "Text" | "Enum" => v.get("value")?.as_str().map(|s| s.to_string()),
        _ => Some(format!("{}", v.get("value")?)),
    }
}

// ── Portfolio Overview ──────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct PortfolioOverview {
    pub total_assets: usize,
    pub total_tiv: f64,
    pub by_type: Vec<TypeCount>,
    pub by_lifecycle: Vec<TypeCount>,
    pub pending_approvals: usize,
}

#[derive(Serialize)]
pub struct TypeCount {
    pub label: String,
    pub count: usize,
}

/// GET /api/dashboard/overview — portfolio summary stats.
async fn overview(
    Auth(principal): Auth,
    State(state): State<AppState>,
) -> Result<Json<PortfolioOverview>, StatusCode> {
    let conn = state.db.get().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let (tenant_clause, tenant_params) = tenant_where(&principal);

    // Asset counts by type
    let type_query = format!(
        "SELECT asset_type, COUNT(*) FROM assets a WHERE {tenant_clause} GROUP BY asset_type ORDER BY COUNT(*) DESC"
    );
    let mut stmt = conn.prepare(&type_query).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let by_type: Vec<TypeCount> = stmt
        .query_map(rusqlite::params_from_iter(&tenant_params), |row| {
            Ok(TypeCount { label: row.get(0)?, count: row.get(1)? })
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .filter_map(|r| r.ok())
        .collect();

    // Asset counts by lifecycle
    let lc_query = format!(
        "SELECT lifecycle, COUNT(*) FROM assets a WHERE {tenant_clause} GROUP BY lifecycle ORDER BY COUNT(*) DESC"
    );
    let mut stmt = conn.prepare(&lc_query).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let by_lifecycle: Vec<TypeCount> = stmt
        .query_map(rusqlite::params_from_iter(&tenant_params), |row| {
            Ok(TypeCount { label: row.get(0)?, count: row.get(1)? })
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .filter_map(|r| r.ok())
        .collect();

    let total_assets: usize = by_type.iter().map(|t| t.count).sum();

    // Total TIV
    let tiv_query = format!(
        "SELECT COALESCE(SUM(
            CASE WHEN json_extract(fm.value_json, '$.type') = 'Money'
                 THEN CAST(json_extract(fm.value_json, '$.value.amount') AS REAL)
                 ELSE 0 END
        ), 0)
         FROM assets a
         JOIN field_mutations fm ON fm.asset_id = a.asset_id
           AND fm.field_name = 'replacement_cost' AND fm.approval_state = 'Approved'
         WHERE {tenant_clause}"
    );
    let total_tiv: f64 = conn
        .query_row(&tiv_query, rusqlite::params_from_iter(&tenant_params), |row| row.get(0))
        .unwrap_or(0.0);

    // Pending approvals
    let pending_query = format!(
        "SELECT COUNT(*) FROM field_mutations fm JOIN assets a ON a.asset_id = fm.asset_id
         WHERE fm.approval_state = 'Pending' AND {tenant_clause}"
    );
    let pending_approvals: usize = conn
        .query_row(&pending_query, rusqlite::params_from_iter(&tenant_params), |row| row.get(0))
        .unwrap_or(0);

    Ok(Json(PortfolioOverview {
        total_assets,
        total_tiv,
        by_type,
        by_lifecycle,
        pending_approvals,
    }))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/dashboard/overview", get(overview))
        .route("/api/dashboard/tiv", get(tiv_aggregation))
}
