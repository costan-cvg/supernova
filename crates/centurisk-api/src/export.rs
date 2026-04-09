//! SOV export endpoints — CSV download and preflight readiness check.
//!
//! - GET /api/export/sov?format=csv — download SOV as CSV file
//! - GET /api/export/preflight — readiness check with gap report

use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use centurisk_export::sov::{AssetExportRow, export_sov_csv, compute_preflight};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::AppState;
use crate::assets::{display_field_value, tenant_where};
use crate::auth::Auth;
use centurisk_core::field_value::FieldValue;

#[derive(Deserialize)]
pub struct ExportQuery {
    pub format: Option<String>,
}

#[derive(Serialize)]
pub struct PreflightResponse {
    pub total_assets: usize,
    pub ready_assets: usize,
    pub gap_assets: usize,
    pub readiness_percentage: f64,
    pub gaps: Vec<GapEntry>,
}

#[derive(Serialize)]
pub struct GapEntry {
    pub asset_id: String,
    pub asset_name: String,
    pub missing_fields: Vec<String>,
}

/// Query all assets for the tenant and build export rows.
fn query_asset_rows(
    state: &AppState,
    principal: &centurisk_auth::Principal,
) -> Result<Vec<AssetExportRow>, StatusCode> {
    let conn = state.db.get().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let (tenant_clause, tenant_params) = tenant_where(principal);

    let query = format!(
        "SELECT a.asset_id, a.asset_type, fm.field_name, fm.value_json
         FROM assets a
         LEFT JOIN field_mutations fm ON fm.asset_id = a.asset_id AND fm.approval_state = 'Approved'
         WHERE {tenant_clause}
         ORDER BY a.asset_id, fm.field_name, fm.effective_date DESC, fm.submitted_at DESC"
    );

    let mut stmt = conn.prepare(&query).map_err(|e| {
        tracing::error!(err = %e, "export.query_assets prepare failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    type Row = (String, String, Option<String>, Option<String>);
    let results: Vec<Row> = stmt
        .query_map(rusqlite::params_from_iter(&tenant_params), |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(|e| {
            tracing::error!(err = %e, "export.query_assets query failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .filter_map(|r| r.ok())
        .collect();

    // Group by asset_id, dedup fields (first = latest due to ORDER BY)
    let mut assets: HashMap<String, AssetExportRow> = HashMap::new();
    let mut seen_fields: HashMap<(String, String), bool> = HashMap::new();

    for (asset_id, asset_type, field_name, value_json) in results {
        let entry = assets.entry(asset_id.clone()).or_insert_with(|| AssetExportRow {
            asset_id: asset_id.clone(),
            asset_type,
            fields: HashMap::new(),
        });
        if let (Some(fname), Some(vjson)) = (field_name, value_json) {
            let key = (asset_id, fname.clone());
            if !seen_fields.contains_key(&key) {
                seen_fields.insert(key, true);
                if let Ok(fv) = serde_json::from_str::<FieldValue>(&vjson) {
                    entry.fields.insert(fname, display_field_value(&fv));
                }
            }
        }
    }

    let mut rows: Vec<AssetExportRow> = assets.into_values().collect();
    rows.sort_by(|a, b| a.asset_id.cmp(&b.asset_id));
    Ok(rows)
}

// ── GET /api/export/sov ────────────────────────────────────────────────────

#[tracing::instrument(name = "api.export_sov", skip_all)]
async fn export_sov(
    Auth(principal): Auth,
    State(state): State<AppState>,
    Query(params): Query<ExportQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    // Only CSV format supported for now
    let format = params.format.as_deref().unwrap_or("csv");
    if format != "csv" {
        return Err(StatusCode::BAD_REQUEST);
    }

    let rows = query_asset_rows(&state, &principal)?;
    let csv_content = export_sov_csv(&rows);

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"sov_export.csv\""),
        ],
        csv_content,
    ))
}

// ── GET /api/export/preflight ──────────────────────────────────────────────

#[tracing::instrument(name = "api.export_preflight", skip_all)]
async fn export_preflight(
    Auth(principal): Auth,
    State(state): State<AppState>,
) -> Result<Json<PreflightResponse>, StatusCode> {
    let rows = query_asset_rows(&state, &principal)?;
    let report = compute_preflight(&rows);

    Ok(Json(PreflightResponse {
        total_assets: report.total_assets,
        ready_assets: report.ready_assets,
        gap_assets: report.gap_assets,
        readiness_percentage: report.readiness_percentage,
        gaps: report.gaps.into_iter().map(|g| GapEntry {
            asset_id: g.asset_id,
            asset_name: g.asset_name,
            missing_fields: g.missing_fields,
        }).collect(),
    }))
}

// ── Routes ─────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/export/sov", get(export_sov))
        .route("/api/export/preflight", get(export_preflight))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use centurisk_auth::AllowAllPolicy;
    use centurisk_core::ids::{ActorId, MemberId, MutationId, PoolId};
    use std::sync::Arc;
    use tower::ServiceExt;

    /// The hardcoded pool_id from the Auth fallback principal.
    const FALLBACK_POOL_ID: &str = "00000000-0000-0000-0000-000000000010";

    fn setup_test_state() -> AppState {
        let db = centurisk_db::init_test_db().unwrap();
        AppState {
            db,
            policy: Arc::new(AllowAllPolicy),
        }
    }

    /// Seed a pool, member, and asset under the fallback pool_id
    /// so the default Auth extractor will find them.
    fn seed_asset_for_fallback(state: &AppState) -> (String, String) {
        let conn = state.db.get().unwrap();
        let pool_id = FALLBACK_POOL_ID;
        let member_id = MemberId::new().to_string();
        let actor_id = ActorId::new().to_string();
        let asset_id = uuid::Uuid::now_v7().to_string();

        conn.execute(
            "INSERT OR IGNORE INTO pools (pool_id, name, created_by) VALUES (?1, 'Test Pool', ?2)",
            rusqlite::params![pool_id, actor_id],
        ).unwrap();
        conn.execute(
            "INSERT INTO members (member_id, pool_id, name, created_by) VALUES (?1, ?2, 'Test Member', ?3)",
            rusqlite::params![member_id, pool_id, actor_id],
        ).unwrap();
        conn.execute(
            "INSERT INTO assets (asset_id, pool_id, member_id, path, asset_type, lifecycle, created_by) VALUES (?1, ?2, ?3, ?4, 'Building', 'Active', ?5)",
            rusqlite::params![asset_id, pool_id, member_id, format!("/{}/{}/{}", pool_id, member_id, asset_id), actor_id],
        ).unwrap();

        (member_id, asset_id)
    }

    fn insert_field(state: &AppState, asset_id: &str, field_name: &str, value_json: &str) {
        let conn = state.db.get().unwrap();
        let mutation_id = MutationId::new().to_string();
        conn.execute(
            "INSERT INTO field_mutations (mutation_id, asset_id, field_name, value_json, effective_date, submitted_by, approval_state) VALUES (?1, ?2, ?3, ?4, '2024-01-01', 'system', 'Approved')",
            rusqlite::params![mutation_id, asset_id, field_name, value_json],
        ).unwrap();
    }

    #[tokio::test]
    async fn export_sov_csv_returns_csv_with_correct_headers() {
        let state = setup_test_state();
        let (_member_id, asset_id) = seed_asset_for_fallback(&state);

        insert_field(&state, &asset_id, "building_name", r#"{"type":"Text","value":"City Hall"}"#);
        insert_field(&state, &asset_id, "address", r#"{"type":"Text","value":"100 Main St"}"#);
        insert_field(&state, &asset_id, "city", r#"{"type":"Text","value":"Springfield"}"#);

        let app = crate::app(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/export/sov?format=csv")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);

        let ct = resp.headers().get(header::CONTENT_TYPE).unwrap().to_str().unwrap();
        assert!(ct.contains("text/csv"));

        let cd = resp.headers().get(header::CONTENT_DISPOSITION).unwrap().to_str().unwrap();
        assert!(cd.contains("sov_export.csv"));

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let csv_str = String::from_utf8(body.to_vec()).unwrap();

        // Verify CSV has header and one data row
        let mut reader = csv::Reader::from_reader(csv_str.as_bytes());
        let headers = reader.headers().unwrap();
        assert_eq!(&headers[0], "asset_type");
        assert_eq!(&headers[1], "building_name");

        let records: Vec<_> = reader.records().collect();
        assert_eq!(records.len(), 1);
        let record = records[0].as_ref().unwrap();
        assert_eq!(&record[0], "Building");
        assert_eq!(&record[1], "City Hall");
        assert_eq!(&record[2], "100 Main St");
        assert_eq!(&record[3], "Springfield");
    }

    #[tokio::test]
    async fn export_preflight_shows_gaps() {
        let state = setup_test_state();
        let (_member_id, asset_id) = seed_asset_for_fallback(&state);

        // Only insert a few fields — asset should show gaps
        insert_field(&state, &asset_id, "building_name", r#"{"type":"Text","value":"Fire Station"}"#);
        insert_field(&state, &asset_id, "address", r#"{"type":"Text","value":"200 Oak Ave"}"#);

        let app = crate::app(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/export/preflight")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let report: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(report["total_assets"], 1);
        assert_eq!(report["ready_assets"], 0);
        assert_eq!(report["gap_assets"], 1);
        assert!(report["readiness_percentage"].as_f64().unwrap() < 1.0);

        let gaps = report["gaps"].as_array().unwrap();
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0]["asset_name"], "Fire Station");

        let missing = gaps[0]["missing_fields"].as_array().unwrap();
        let missing_names: Vec<&str> = missing.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(missing_names.contains(&"city"));
        assert!(missing_names.contains(&"year_built"));
        assert!(missing_names.contains(&"sq_footage"));
        // building_name and address are present, should NOT be missing
        assert!(!missing_names.contains(&"building_name"));
        assert!(!missing_names.contains(&"address"));
    }

    #[tokio::test]
    async fn export_tenant_scoping() {
        let state = setup_test_state();
        let conn = state.db.get().unwrap();

        // Create two pools with assets
        let pool_a = PoolId::new().to_string();
        let pool_b = PoolId::new().to_string();
        let member_a = MemberId::new().to_string();
        let member_b = MemberId::new().to_string();
        let actor = ActorId::new().to_string();
        let asset_a = uuid::Uuid::now_v7().to_string();
        let asset_b = uuid::Uuid::now_v7().to_string();

        conn.execute("INSERT INTO pools (pool_id, name, created_by) VALUES (?1, 'Pool A', ?2)", rusqlite::params![pool_a, actor]).unwrap();
        conn.execute("INSERT INTO pools (pool_id, name, created_by) VALUES (?1, 'Pool B', ?2)", rusqlite::params![pool_b, actor]).unwrap();
        conn.execute("INSERT INTO members (member_id, pool_id, name, created_by) VALUES (?1, ?2, 'Member A', ?3)", rusqlite::params![member_a, pool_a, actor]).unwrap();
        conn.execute("INSERT INTO members (member_id, pool_id, name, created_by) VALUES (?1, ?2, 'Member B', ?3)", rusqlite::params![member_b, pool_b, actor]).unwrap();
        conn.execute(
            "INSERT INTO assets (asset_id, pool_id, member_id, path, asset_type, lifecycle, created_by) VALUES (?1, ?2, ?3, '/a', 'Building', 'Active', ?4)",
            rusqlite::params![asset_a, pool_a, member_a, actor],
        ).unwrap();
        conn.execute(
            "INSERT INTO assets (asset_id, pool_id, member_id, path, asset_type, lifecycle, created_by) VALUES (?1, ?2, ?3, '/b', 'Building', 'Active', ?4)",
            rusqlite::params![asset_b, pool_b, member_b, actor],
        ).unwrap();

        // Drop the conn so query_asset_rows can borrow from the single-connection pool
        drop(conn);

        insert_field(&state, &asset_a, "building_name", r#"{"type":"Text","value":"Pool A Building"}"#);
        insert_field(&state, &asset_b, "building_name", r#"{"type":"Text","value":"Pool B Building"}"#);

        // Test with Pool A admin principal — should only see Pool A's asset
        let pool_a_uuid = uuid::Uuid::parse_str(&pool_a).unwrap();
        let principal_a = centurisk_auth::Principal {
            actor_id: centurisk_core::ids::ActorId::new(),
            category: centurisk_auth::principal::UserCategory::PoolAdministrator,
            pool_id: Some(centurisk_core::ids::PoolId::from_uuid(pool_a_uuid)),
            member_id: None,
            profile_ids: vec!["PoolAdministrator".into()],
        };

        let rows = query_asset_rows(&state, &principal_a).unwrap();
        assert_eq!(rows.len(), 1, "Pool A admin should only see Pool A's asset");
        assert_eq!(rows[0].fields.get("building_name").unwrap(), "Pool A Building");

        // Test with Pool B admin principal — should only see Pool B's asset
        let pool_b_uuid = uuid::Uuid::parse_str(&pool_b).unwrap();
        let principal_b = centurisk_auth::Principal {
            actor_id: centurisk_core::ids::ActorId::new(),
            category: centurisk_auth::principal::UserCategory::PoolAdministrator,
            pool_id: Some(centurisk_core::ids::PoolId::from_uuid(pool_b_uuid)),
            member_id: None,
            profile_ids: vec!["PoolAdministrator".into()],
        };

        let rows = query_asset_rows(&state, &principal_b).unwrap();
        assert_eq!(rows.len(), 1, "Pool B admin should only see Pool B's asset");
        assert_eq!(rows[0].fields.get("building_name").unwrap(), "Pool B Building");
    }

    #[tokio::test]
    async fn export_bad_format_returns_400() {
        let state = setup_test_state();
        let app = crate::app(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/export/sov?format=xlsx")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
