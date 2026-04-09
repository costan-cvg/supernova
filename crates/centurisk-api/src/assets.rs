use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use centurisk_core::asset::{AssetType, LifecycleState};
use centurisk_core::field_value::FieldValue;
use centurisk_core::ids::{AssetId, MemberId, MutationId};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;

use crate::AppState;
use crate::auth::Auth;

#[derive(Serialize)]
pub struct AssetResponse {
    pub asset_id: String,
    pub asset_type: String,
    pub lifecycle: String,
    pub fields: HashMap<String, String>,
}

#[derive(Deserialize)]
pub struct CreateAssetRequest {
    pub asset_type: String,
    pub fields: HashMap<String, String>,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

async fn list_assets(
    Auth(principal): Auth,
    State(state): State<AppState>,
) -> Result<Json<Vec<AssetResponse>>, StatusCode> {
    // Derive tenant context from principal
    let tenant = crate::auth::tenant_from_principal(&principal);

    let conn = state.db.get().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut stmt = conn
        .prepare(
            "SELECT a.asset_id, a.asset_type, a.lifecycle, fm.field_name, fm.value_json
             FROM assets a
             LEFT JOIN field_mutations fm ON fm.asset_id = a.asset_id AND fm.approval_state = 'Approved'
             WHERE a.pool_id = ?1
             ORDER BY a.created_at DESC, fm.field_name",
        )
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows: Vec<(String, String, String, Option<String>, Option<String>)> = stmt
        .query_map(rusqlite::params![tenant.pool_id.to_string()], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .filter_map(|r| r.ok())
        .collect();

    // Group by asset_id
    let mut assets: HashMap<String, AssetResponse> = HashMap::new();
    for (asset_id, asset_type, lifecycle, field_name, value_json) in rows {
        let entry = assets.entry(asset_id.clone()).or_insert_with(|| AssetResponse {
            asset_id,
            asset_type,
            lifecycle,
            fields: HashMap::new(),
        });

        if let (Some(fname), Some(vjson)) = (field_name, value_json) {
            // Parse FieldValue and extract display string
            if let Ok(fv) = serde_json::from_str::<FieldValue>(&vjson) {
                entry.fields.insert(fname, display_field_value(&fv));
            }
        }
    }

    let mut result: Vec<AssetResponse> = assets.into_values().collect();
    result.sort_by(|a, b| a.asset_id.cmp(&b.asset_id));
    Ok(Json(result))
}

async fn create_asset(
    Auth(principal): Auth,
    State(state): State<AppState>,
    Json(req): Json<CreateAssetRequest>,
) -> Result<(StatusCode, Json<AssetResponse>), (StatusCode, Json<ErrorResponse>)> {
    let tenant = crate::auth::tenant_from_principal(&principal);

    let asset_type = match req.asset_type.as_str() {
        "Building" => AssetType::Building,
        "Contents" => AssetType::Contents,
        "Vehicle" => AssetType::Vehicle,
        "FineArts" => AssetType::FineArts,
        _ => return Err((StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "Invalid asset_type".into() }))),
    };

    let asset_id = AssetId::new();
    // For now, use the principal's member_id or create a default path
    let member_id = principal.member_id.unwrap_or_else(MemberId::new);
    let path = format!("/{}/{}/{}", tenant.pool_id, member_id, asset_id);

    let conn = state.db.get()
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "DB error".into() })))?;

    // Insert asset
    conn.execute(
        "INSERT INTO assets (asset_id, pool_id, member_id, path, asset_type, lifecycle, created_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            asset_id.to_string(),
            tenant.pool_id.to_string(),
            member_id.to_string(),
            path,
            format!("{:?}", asset_type),
            format!("{:?}", LifecycleState::Draft),
            principal.actor_id.to_string(),
        ],
    ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("Insert failed: {e}") })))?;

    // Insert field mutations (auto-approved for admin in Inc 1-4)
    let today = time::OffsetDateTime::now_utc().date();
    let effective_date = format!("{}", today);
    let mut display_fields = HashMap::new();

    for (field_name, raw_value) in &req.fields {
        let field_value = parse_field_value(field_name, raw_value);
        let value_json = serde_json::to_string(&field_value).unwrap();

        let mutation_id = MutationId::new();
        conn.execute(
            "INSERT INTO field_mutations (mutation_id, asset_id, field_name, value_json, effective_date, submitted_by, approval_state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'Approved')",
            rusqlite::params![
                mutation_id.to_string(),
                asset_id.to_string(),
                field_name,
                value_json,
                effective_date,
                principal.actor_id.to_string(),
            ],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("Mutation failed: {e}") })))?;

        display_fields.insert(field_name.clone(), display_field_value(&field_value));
    }

    Ok((
        StatusCode::CREATED,
        Json(AssetResponse {
            asset_id: asset_id.to_string(),
            asset_type: format!("{:?}", asset_type),
            lifecycle: "Draft".into(),
            fields: display_fields,
        }),
    ))
}

/// Parse a raw string value into a typed FieldValue based on field name conventions.
fn parse_field_value(field_name: &str, raw: &str) -> FieldValue {
    match field_name {
        "replacement_cost" | "contents_value" => {
            if let Ok(amount) = Decimal::from_str(raw) {
                FieldValue::Money { amount, currency: "USD".into() }
            } else {
                FieldValue::Text(raw.into())
            }
        }
        "year_built" | "sq_footage" | "stories" | "elevator_count" | "parking_spaces"
        | "electrical_update_year" | "plumbing_update_year" => {
            if let Ok(n) = Decimal::from_str(raw) {
                FieldValue::Number(n)
            } else {
                FieldValue::Text(raw.into())
            }
        }
        "sprinkler" | "basement" | "fire_alarm" | "security_system" => {
            FieldValue::Bool(raw == "true" || raw == "yes" || raw == "1")
        }
        "construction_class" | "occupancy" | "roof_type" | "foundation_type"
        | "heating_type" | "cooling_type" | "flood_zone" | "ais_zone" => {
            FieldValue::Enum(raw.into())
        }
        "appraisal_date" => {
            // Try to parse as date
            FieldValue::Text(raw.into()) // Simplified — full date parsing in Inc 2
        }
        _ => FieldValue::Text(raw.into()),
    }
}

/// Convert a FieldValue to a display string for the API response.
fn display_field_value(fv: &FieldValue) -> String {
    match fv {
        FieldValue::Text(s) => s.clone(),
        FieldValue::Number(n) => n.to_string(),
        FieldValue::Date(d) => d.to_string(),
        FieldValue::Bool(b) => if *b { "Yes" } else { "No" }.into(),
        FieldValue::Enum(s) => s.clone(),
        FieldValue::Money { amount, currency } => format!("${} {}", amount, currency),
        FieldValue::Null => "—".into(),
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/assets", get(list_assets).post(create_asset))
}
