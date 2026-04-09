//! Quality scoring API endpoints.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use centurisk_core::field_value::FieldValue;
use centurisk_core::quality::{self, QualityScore};
use std::collections::HashMap;

use crate::AppState;
use crate::auth::Auth;
use crate::assets::display_field_value;

/// GET /api/assets/:id/quality — compute quality scores for an asset.
async fn asset_quality(
    Auth(principal): Auth,
    State(state): State<AppState>,
    Path(asset_id): Path<String>,
) -> Result<Json<QualityScore>, StatusCode> {
    let conn = state.db.get().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Get asset type + verify tenant access
    let (tenant_clause, tenant_params) = crate::assets::tenant_where(&principal);
    let mut check_params = tenant_params;
    check_params.push(asset_id.clone());
    let aid_idx = check_params.len();

    let asset_type: String = conn
        .query_row(
            &format!("SELECT asset_type FROM assets a WHERE {tenant_clause} AND a.asset_id = ?{aid_idx}"),
            rusqlite::params_from_iter(&check_params),
            |row| row.get(0),
        )
        .map_err(|_| StatusCode::NOT_FOUND)?;

    // Get current approved fields
    let mut stmt = conn.prepare(
        "SELECT field_name, value_json FROM field_mutations
         WHERE asset_id = ?1 AND approval_state = 'Approved'
         ORDER BY field_name, effective_date DESC"
    ).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows: Vec<(String, String)> = stmt
        .query_map(rusqlite::params![asset_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .filter_map(|r| r.ok())
        .collect();

    // Deduplicate to latest per field
    let mut fields: HashMap<String, FieldValue> = HashMap::new();
    for (fname, vjson) in &rows {
        if !fields.contains_key(fname) {
            if let Ok(fv) = serde_json::from_str(vjson) {
                fields.insert(fname.clone(), fv);
            }
        }
    }

    // Completeness: merge built-in + custom field definitions
    let mut comp_config = match asset_type.as_str() {
        "Building" => quality::building_completeness_config(),
        "LicensedVehicle" => quality::vehicle_completeness_config(),
        "PropertyInTheOpen" => quality::pito_completeness_config(),
        "MovableEquipment" => quality::equipment_completeness_config(),
        _ => quality::default_completeness_config(),
    };

    // Resolve pool_id for loading custom fields
    let pool_id_str: String = conn
        .query_row(
            &format!("SELECT a.pool_id FROM assets a WHERE {tenant_clause} AND a.asset_id = ?{aid_idx}"),
            rusqlite::params_from_iter(&check_params),
            |row| row.get(0),
        )
        .unwrap_or_default();

    if !pool_id_str.is_empty() {
        let custom_defs = crate::custom_fields::load_custom_fields_for_pool(&conn, &pool_id_str);
        for cf in &custom_defs {
            if cf.asset_types.contains(&asset_type) {
                if cf.required {
                    comp_config.required.push(cf.field_name.clone());
                } else if cf.recommended {
                    comp_config.recommended.push(cf.field_name.clone());
                }
            }
        }
    }

    let completeness = quality::score_completeness(&fields, &comp_config);

    // Accuracy
    let accuracy = quality::score_accuracy(&fields, &quality::default_accuracy_rules());

    // Recency: compute days since last mutation per tracked field
    let mut field_ages: HashMap<String, u32> = HashMap::new();
    let today = time::OffsetDateTime::now_utc().date();

    let mut age_stmt = conn.prepare(
        "SELECT field_name, MAX(effective_date) FROM field_mutations
         WHERE asset_id = ?1 AND approval_state = 'Approved'
         GROUP BY field_name"
    ).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let age_rows: Vec<(String, String)> = age_stmt
        .query_map(rusqlite::params![asset_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .filter_map(|r| r.ok())
        .collect();

    for (fname, date_str) in &age_rows {
        if let Some(date) = parse_date(date_str) {
            let duration = today - date;
            field_ages.insert(fname.clone(), duration.whole_days().max(0) as u32);
        }
    }

    let recency = quality::score_recency(&field_ages, &quality::default_recency_config());

    // Composite: weighted average (completeness 40%, accuracy 30%, recency 30%)
    let composite = completeness.score * 0.4 + accuracy.score * 0.3 + recency.score * 0.3;

    Ok(Json(QualityScore { completeness, accuracy, recency, composite }))
}

/// GET /api/quality/summary — quality scores for all assets in the tenant.
async fn quality_summary(
    Auth(principal): Auth,
    State(state): State<AppState>,
) -> Result<Json<Vec<AssetQualitySummary>>, StatusCode> {
    let conn = state.db.get().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let (tenant_clause, tenant_params) = crate::assets::tenant_where(&principal);
    let query = format!(
        "SELECT a.asset_id, a.asset_type, fm.field_name, fm.value_json
         FROM assets a
         LEFT JOIN field_mutations fm ON fm.asset_id = a.asset_id AND fm.approval_state = 'Approved'
         WHERE {tenant_clause}
         ORDER BY a.asset_id, fm.field_name, fm.effective_date DESC"
    );

    let mut stmt = conn.prepare(&query).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    type Row = (String, String, Option<String>, Option<String>);
    let rows: Vec<Row> = stmt
        .query_map(rusqlite::params_from_iter(&tenant_params), |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .filter_map(|r| r.ok())
        .collect();

    // Group fields by asset
    let mut assets: HashMap<String, (String, HashMap<String, FieldValue>)> = HashMap::new();
    for (aid, atype, fname, vjson) in &rows {
        let entry = assets.entry(aid.clone()).or_insert_with(|| (atype.clone(), HashMap::new()));
        if let (Some(f), Some(v)) = (fname, vjson) {
            if !entry.1.contains_key(f) {
                if let Ok(fv) = serde_json::from_str(v) {
                    entry.1.insert(f.clone(), fv);
                }
            }
        }
    }

    // Load custom field definitions for this pool (once, outside the loop)
    let pool_id_str = crate::auth::tenant_from_principal(&principal)
        .map(|t| t.pool_id.to_string())
        .unwrap_or_default();
    let custom_defs = if !pool_id_str.is_empty() {
        crate::custom_fields::load_custom_fields_for_pool(&conn, &pool_id_str)
    } else {
        Vec::new()
    };

    let mut results: Vec<AssetQualitySummary> = assets.into_iter().map(|(asset_id, (asset_type, fields))| {
        let mut comp_config = match asset_type.as_str() {
            "Building" => quality::building_completeness_config(),
            "LicensedVehicle" => quality::vehicle_completeness_config(),
            "PropertyInTheOpen" => quality::pito_completeness_config(),
            "MovableEquipment" => quality::equipment_completeness_config(),
            _ => quality::default_completeness_config(),
        };

        // Extend with custom field definitions applicable to this asset type
        for cf in &custom_defs {
            if cf.asset_types.contains(&asset_type) {
                if cf.required {
                    comp_config.required.push(cf.field_name.clone());
                } else if cf.recommended {
                    comp_config.recommended.push(cf.field_name.clone());
                }
            }
        }

        let completeness = quality::score_completeness(&fields, &comp_config);
        let accuracy = quality::score_accuracy(&fields, &quality::default_accuracy_rules());
        let composite = completeness.score * 0.5 + accuracy.score * 0.5;
        let name = fields.get("building_name").map(display_field_value)
            .unwrap_or_else(|| format!("{} {}", asset_type, &asset_id[..8]));

        AssetQualitySummary {
            asset_id,
            asset_type,
            name,
            completeness: completeness.score,
            accuracy: accuracy.score,
            composite,
            missing_required: completeness.missing_required,
        }
    }).collect();

    results.sort_by(|a, b| a.composite.partial_cmp(&b.composite).unwrap());
    Ok(Json(results))
}

#[derive(serde::Serialize)]
struct AssetQualitySummary {
    asset_id: String,
    asset_type: String,
    name: String,
    completeness: f64,
    accuracy: f64,
    composite: f64,
    missing_required: Vec<String>,
}

fn parse_date(s: &str) -> Option<time::Date> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() >= 3 {
        let y: i32 = parts[0].parse().ok()?;
        let m: u8 = parts[1].parse().ok()?;
        let d: u8 = parts[2].parse().ok()?;
        time::Date::from_calendar_date(y, time::Month::try_from(m).ok()?, d).ok()
    } else {
        None
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/quality/asset/:id", get(asset_quality))
        .route("/api/quality/summary", get(quality_summary))
}
