use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use centurisk_core::asset::{AssetType, LifecycleState};
use centurisk_core::field_value::FieldValue;
use centurisk_core::ids::{AssetId, MemberId, MutationId, PoolId};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;

use crate::AppState;
use crate::auth::Auth;

// ── Response types ──────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct AssetResponse {
    pub asset_id: String,
    pub pool_id: String,
    pub member_id: String,
    pub asset_type: String,
    pub lifecycle: String,
    pub fields: HashMap<String, String>,
}

#[derive(Serialize)]
pub struct MutationResponse {
    pub mutation_id: String,
    pub field_name: String,
    pub value: String,
    pub value_raw: serde_json::Value,
    pub effective_date: String,
    pub submitted_at: String,
    pub submitted_by: String,
    pub approval_state: String,
}

#[derive(Deserialize)]
pub struct CreateAssetRequest {
    pub asset_type: String,
    pub fields: HashMap<String, String>,
}

#[derive(Deserialize)]
pub struct ListAssetsQuery {
    pub asset_type: Option<String>,
    pub lifecycle: Option<String>,
    pub search: Option<String>,
}

#[derive(Deserialize)]
pub struct AssetDetailQuery {
    pub as_of: Option<String>,
    pub include_pending: Option<bool>,
}

#[derive(Deserialize)]
pub struct EditFieldsRequest {
    pub fields: HashMap<String, String>,
    pub effective_date: Option<String>,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Build a WHERE clause from the principal's tenant scope.
pub fn tenant_where(principal: &centurisk_auth::Principal) -> (String, Vec<String>) {
    let tenant = crate::auth::tenant_from_principal(principal);
    match tenant {
        Some(t) => {
            if let Some(mid) = t.member_id {
                ("a.pool_id = ?1 AND a.member_id = ?2".into(), vec![t.pool_id.to_string(), mid.to_string()])
            } else {
                ("a.pool_id = ?1".into(), vec![t.pool_id.to_string()])
            }
        }
        None => ("1=1".into(), vec![]), // CentuRisk admin sees all
    }
}

// ── GET /api/assets ─────────────────────────────────────────────────────────

async fn list_assets(
    Auth(principal): Auth,
    State(state): State<AppState>,
    Query(params): Query<ListAssetsQuery>,
) -> Result<Json<Vec<AssetResponse>>, StatusCode> {
    let conn = state.db.get().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let (tenant_clause, tenant_params) = tenant_where(&principal);

    // Build dynamic filters
    let mut where_parts = vec![tenant_clause];
    let mut all_params: Vec<String> = tenant_params;

    if let Some(at) = &params.asset_type {
        all_params.push(at.clone());
        where_parts.push(format!("a.asset_type = ?{}", all_params.len()));
    }
    if let Some(lc) = &params.lifecycle {
        all_params.push(lc.clone());
        where_parts.push(format!("a.lifecycle = ?{}", all_params.len()));
    }

    let where_sql = where_parts.join(" AND ");
    let query = format!(
        "SELECT a.asset_id, a.pool_id, a.member_id, a.asset_type, a.lifecycle, fm.field_name, fm.value_json
         FROM assets a
         LEFT JOIN field_mutations fm ON fm.asset_id = a.asset_id AND fm.approval_state = 'Approved'
         WHERE {where_sql}
         ORDER BY a.created_at DESC, fm.field_name"
    );

    let mut stmt = conn.prepare(&query).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    type Row = (String, String, String, String, String, Option<String>, Option<String>);
    let results: Vec<Row> = stmt
        .query_map(rusqlite::params_from_iter(&all_params), |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?))
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .filter_map(|r| r.ok())
        .collect();

    let mut assets: HashMap<String, AssetResponse> = HashMap::new();
    for (asset_id, pool_id, member_id, asset_type, lifecycle, field_name, value_json) in results {
        let entry = assets.entry(asset_id.clone()).or_insert_with(|| AssetResponse {
            asset_id, pool_id, member_id, asset_type, lifecycle,
            fields: HashMap::new(),
        });
        if let (Some(fname), Some(vjson)) = (field_name, value_json) {
            if let Ok(fv) = serde_json::from_str::<FieldValue>(&vjson) {
                entry.fields.insert(fname, display_field_value(&fv));
            }
        }
    }

    // Optional text search across field values
    let mut result: Vec<AssetResponse> = if let Some(search) = &params.search {
        let s = search.to_lowercase();
        assets.into_values().filter(|a| {
            a.fields.values().any(|v| v.to_lowercase().contains(&s))
                || a.asset_type.to_lowercase().contains(&s)
        }).collect()
    } else {
        assets.into_values().collect()
    };

    result.sort_by(|a, b| a.asset_id.cmp(&b.asset_id));
    Ok(Json(result))
}

// ── GET /api/assets/:id ─────────────────────────────────────────────────────

async fn get_asset(
    Auth(principal): Auth,
    State(state): State<AppState>,
    Path(asset_id): Path<String>,
    Query(params): Query<AssetDetailQuery>,
) -> Result<Json<AssetResponse>, StatusCode> {
    let conn = state.db.get().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let (tenant_clause, tenant_params) = tenant_where(&principal);

    // Verify asset belongs to tenant and get identity
    let mut check_params = tenant_params.clone();
    check_params.push(asset_id.clone());
    let aid_idx = check_params.len();

    let identity_query = format!(
        "SELECT asset_id, pool_id, member_id, asset_type, lifecycle FROM assets a WHERE {tenant_clause} AND a.asset_id = ?{aid_idx}"
    );
    let identity: (String, String, String, String, String) = conn
        .query_row(&identity_query, rusqlite::params_from_iter(&check_params), |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
        })
        .map_err(|_| StatusCode::NOT_FOUND)?;

    // Resolve fields: temporal resolution via as_of date
    // For each field, pick the latest approved mutation with effective_date <= as_of
    let approval_filter = if params.include_pending.unwrap_or(false) {
        "fm.approval_state IN ('Approved', 'Pending')"
    } else {
        "fm.approval_state = 'Approved'"
    };

    let fields = if let Some(as_of) = &params.as_of {
        // Temporal query: resolve state as of a specific date
        let mut stmt = conn.prepare(&format!(
            "SELECT fm.field_name, fm.value_json
             FROM field_mutations fm
             WHERE fm.asset_id = ?1 AND {approval_filter} AND fm.effective_date <= ?2
             ORDER BY fm.field_name, fm.effective_date DESC, fm.submitted_at DESC"
        )).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let rows: Vec<(String, String)> = stmt
            .query_map(rusqlite::params![asset_id, as_of], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .filter_map(|r| r.ok())
            .collect();

        // Deduplicate: keep only the first (latest) value per field_name
        let mut fields = HashMap::new();
        for (fname, vjson) in rows {
            if !fields.contains_key(&fname) {
                if let Ok(fv) = serde_json::from_str::<FieldValue>(&vjson) {
                    fields.insert(fname, display_field_value(&fv));
                }
            }
        }
        fields
    } else {
        // Current state: latest approved mutation per field (no date filter)
        let mut stmt = conn.prepare(&format!(
            "SELECT fm.field_name, fm.value_json
             FROM field_mutations fm
             WHERE fm.asset_id = ?1 AND {approval_filter}
             ORDER BY fm.field_name, fm.effective_date DESC, fm.submitted_at DESC"
        )).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let rows: Vec<(String, String)> = stmt
            .query_map(rusqlite::params![asset_id], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .filter_map(|r| r.ok())
            .collect();

        let mut fields = HashMap::new();
        for (fname, vjson) in rows {
            if !fields.contains_key(&fname) {
                if let Ok(fv) = serde_json::from_str::<FieldValue>(&vjson) {
                    fields.insert(fname, display_field_value(&fv));
                }
            }
        }
        fields
    };

    Ok(Json(AssetResponse {
        asset_id: identity.0, pool_id: identity.1, member_id: identity.2,
        asset_type: identity.3, lifecycle: identity.4,
        fields,
    }))
}

// ── PUT /api/assets/:id/fields ──────────────────────────────────────────────

async fn edit_fields(
    Auth(principal): Auth,
    State(state): State<AppState>,
    Path(asset_id): Path<String>,
    Json(req): Json<EditFieldsRequest>,
) -> Result<Json<Vec<MutationResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let conn = state.db.get()
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "DB error".into() })))?;

    // Verify asset exists and belongs to tenant
    let (tenant_clause, tenant_params) = tenant_where(&principal);
    let mut check_params = tenant_params;
    check_params.push(asset_id.clone());
    let aid_idx = check_params.len();

    conn.query_row(
        &format!("SELECT 1 FROM assets a WHERE {tenant_clause} AND a.asset_id = ?{aid_idx}"),
        rusqlite::params_from_iter(&check_params),
        |_| Ok(()),
    ).map_err(|_| (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "Asset not found".into() })))?;

    let effective_date = req.effective_date.unwrap_or_else(|| {
        time::OffsetDateTime::now_utc().date().to_string()
    });
    let now = time::OffsetDateTime::now_utc();
    let submitted_at = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        now.year(), now.month() as u8, now.day(),
        now.hour(), now.minute(), now.second()
    );

    // Determine if this user has auto_approve (CentuRisk admins and pool admins do)
    let auto_approve = matches!(
        principal.category,
        centurisk_auth::principal::UserCategory::CentuRiskAdmin
        | centurisk_auth::principal::UserCategory::PoolAdministrator
    );

    // Validate proposed values
    let mut parsed_fields = HashMap::new();
    for (field_name, raw_value) in &req.fields {
        parsed_fields.insert(field_name.clone(), parse_field_value(field_name, raw_value));
    }
    let validation_errors = centurisk_core::sov::validate_fields(&parsed_fields);
    if !validation_errors.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(ErrorResponse {
            error: validation_errors.join("; "),
        })));
    }

    let mut created_mutations = Vec::new();

    for (field_name, field_value) in &parsed_fields {
        let value_json = serde_json::to_string(field_value).unwrap();
        let mutation_id = MutationId::new();

        // Route through approval decision
        let decision = centurisk_core::sov::decide_approval(
            centurisk_core::sov::ChangeType::Modified,
            field_name,
            auto_approve,
        );
        let approval_state = match decision {
            centurisk_core::sov::ApprovalDecision::AutoApprove => "Approved",
            centurisk_core::sov::ApprovalDecision::Pending => "Pending",
        };

        conn.execute(
            "INSERT INTO field_mutations (mutation_id, asset_id, field_name, value_json, effective_date, submitted_at, submitted_by, approval_state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                mutation_id.to_string(), asset_id, field_name, value_json,
                effective_date, submitted_at, principal.actor_id.to_string(), approval_state,
            ],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("Mutation failed: {e}") })))?;

        let raw = serde_json::from_str(&value_json).unwrap_or(serde_json::Value::Null);
        created_mutations.push(MutationResponse {
            mutation_id: mutation_id.to_string(),
            field_name: field_name.clone(),
            value: display_field_value(field_value),
            value_raw: raw,
            effective_date: effective_date.clone(),
            submitted_at: submitted_at.clone(),
            submitted_by: principal.actor_id.to_string(),
            approval_state: approval_state.into(),
        });
    }

    Ok(Json(created_mutations))
}

// ── GET /api/assets/:id/mutations ───────────────────────────────────────────

async fn get_mutations(
    Auth(principal): Auth,
    State(state): State<AppState>,
    Path(asset_id): Path<String>,
) -> Result<Json<Vec<MutationResponse>>, StatusCode> {
    let conn = state.db.get().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let (tenant_clause, tenant_params) = tenant_where(&principal);

    let mut all_params = tenant_params;
    all_params.push(asset_id.clone());
    let aid_idx = all_params.len();

    // Verify asset belongs to tenant
    let check_query = format!(
        "SELECT 1 FROM assets a WHERE {tenant_clause} AND a.asset_id = ?{aid_idx}"
    );
    conn.query_row(&check_query, rusqlite::params_from_iter(&all_params), |_| Ok(()))
        .map_err(|_| StatusCode::NOT_FOUND)?;

    // Get all mutations ordered by field + date
    let mut stmt = conn.prepare(
        "SELECT mutation_id, field_name, value_json, effective_date, submitted_at, submitted_by, approval_state
         FROM field_mutations
         WHERE asset_id = ?1
         ORDER BY field_name, effective_date DESC, submitted_at DESC"
    ).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mutations: Vec<MutationResponse> = stmt
        .query_map(rusqlite::params![asset_id], |row| {
            let value_json_str: String = row.get(2)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                value_json_str,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .filter_map(|r| r.ok())
        .map(|(mid, fname, vjson, edate, sat, sby, astate)| {
            let display = serde_json::from_str::<FieldValue>(&vjson)
                .map(|fv| display_field_value(&fv))
                .unwrap_or_else(|_| vjson.clone());
            let raw = serde_json::from_str(&vjson).unwrap_or(serde_json::Value::Null);
            MutationResponse {
                mutation_id: mid, field_name: fname, value: display, value_raw: raw,
                effective_date: edate, submitted_at: sat, submitted_by: sby, approval_state: astate,
            }
        })
        .collect();

    Ok(Json(mutations))
}

// ── POST /api/assets ────────────────────────────────────────────────────────

async fn create_asset(
    Auth(principal): Auth,
    State(state): State<AppState>,
    Json(req): Json<CreateAssetRequest>,
) -> Result<(StatusCode, Json<AssetResponse>), (StatusCode, Json<ErrorResponse>)> {
    let asset_type = match req.asset_type.as_str() {
        "Building" => AssetType::Building,
        "Contents" => AssetType::Contents,
        "Vehicle" => AssetType::Vehicle,
        "FineArts" => AssetType::FineArts,
        _ => return Err((StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "Invalid asset_type".into() }))),
    };

    let asset_id = AssetId::new();
    let conn = state.db.get()
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "DB error".into() })))?;

    let pool_id = if let Some(pid) = principal.pool_id {
        pid
    } else {
        let pid_str: String = conn
            .query_row("SELECT pool_id FROM pools LIMIT 1", [], |row| row.get(0))
            .map_err(|_| (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "No pools exist".into() })))?;
        PoolId::from_uuid(uuid::Uuid::parse_str(&pid_str).unwrap())
    };

    let member_id = if let Some(mid) = principal.member_id {
        mid
    } else {
        let mid_str: String = conn
            .query_row("SELECT member_id FROM members WHERE pool_id = ?1 LIMIT 1",
                rusqlite::params![pool_id.to_string()], |row| row.get(0))
            .map_err(|_| (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "No member found in pool".into() })))?;
        MemberId::from_uuid(uuid::Uuid::parse_str(&mid_str).unwrap())
    };

    let path = format!("/{}/{}/{}", pool_id, member_id, asset_id);

    conn.execute(
        "INSERT INTO assets (asset_id, pool_id, member_id, path, asset_type, lifecycle, created_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            asset_id.to_string(), pool_id.to_string(), member_id.to_string(),
            path, format!("{:?}", asset_type), format!("{:?}", LifecycleState::Draft),
            principal.actor_id.to_string(),
        ],
    ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("Insert failed: {e}") })))?;

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
            rusqlite::params![mutation_id.to_string(), asset_id.to_string(), field_name, value_json, effective_date, principal.actor_id.to_string()],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("Mutation failed: {e}") })))?;
        display_fields.insert(field_name.clone(), display_field_value(&field_value));
    }

    Ok((StatusCode::CREATED, Json(AssetResponse {
        asset_id: asset_id.to_string(), pool_id: pool_id.to_string(), member_id: member_id.to_string(),
        asset_type: format!("{:?}", asset_type), lifecycle: "Draft".into(), fields: display_fields,
    })))
}

// ── Field value parsing / display ───────────────────────────────────────────

pub fn parse_field_value(field_name: &str, raw: &str) -> FieldValue {
    match field_name {
        "replacement_cost" | "contents_value" => {
            Decimal::from_str(raw).map(|a| FieldValue::Money { amount: a, currency: "USD".into() })
                .unwrap_or_else(|_| FieldValue::Text(raw.into()))
        }
        "year_built" | "sq_footage" | "stories" | "elevator_count" | "parking_spaces"
        | "electrical_update_year" | "plumbing_update_year" => {
            Decimal::from_str(raw).map(FieldValue::Number).unwrap_or_else(|_| FieldValue::Text(raw.into()))
        }
        "sprinkler" | "basement" | "fire_alarm" | "security_system" => {
            FieldValue::Bool(matches!(raw, "true" | "yes" | "1"))
        }
        "construction_class" | "occupancy" | "roof_type" | "foundation_type"
        | "heating_type" | "cooling_type" | "flood_zone" | "ais_zone" => FieldValue::Enum(raw.into()),
        _ => FieldValue::Text(raw.into()),
    }
}

pub fn display_field_value(fv: &FieldValue) -> String {
    match fv {
        FieldValue::Text(s) => s.clone(),
        FieldValue::Number(n) => n.to_string(),
        FieldValue::Date(d) => d.to_string(),
        FieldValue::Bool(b) => if *b { "Yes" } else { "No" }.into(),
        FieldValue::Enum(s) => s.clone(),
        FieldValue::Money { amount, currency } => format!("${} {}", amount, currency),
        FieldValue::Null => "\u{2014}".into(),
    }
}

// ── Routes ──────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/assets", get(list_assets).post(create_asset))
        .route("/api/assets/{id}", get(get_asset))
        .route("/api/assets/{id}/fields", axum::routing::put(edit_fields))
        .route("/api/assets/{id}/mutations", get(get_mutations))
}
