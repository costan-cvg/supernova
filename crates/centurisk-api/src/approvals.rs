//! Approval queue — pool admins review and approve/reject pending mutations.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use centurisk_auth::principal::UserCategory;
use serde::{Deserialize, Serialize};

use crate::assets::{display_field_value, tenant_where};
use crate::auth::Auth;
use crate::AppState;

#[derive(Serialize)]
pub struct PendingMutation {
    pub mutation_id: String,
    pub asset_id: String,
    pub asset_name: String,
    pub asset_type: String,
    pub field_name: String,
    pub proposed_value: String,
    pub proposed_value_raw: serde_json::Value,
    pub previous_value: Option<String>,
    pub effective_date: String,
    pub submitted_at: String,
    pub submitted_by: String,
    pub is_valuation_field: bool,
}

#[derive(Deserialize)]
pub struct ApprovalAction {
    pub decision: String, // "approve" or "reject"
}

#[derive(Serialize)]
struct ActionResult {
    mutation_id: String,
    new_state: String,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

/// GET /api/approvals — list all pending mutations for the user's pool.
async fn list_pending(
    Auth(principal): Auth,
    State(state): State<AppState>,
) -> Result<Json<Vec<PendingMutation>>, StatusCode> {
    // Only pool admins and CentuRisk admins can see the approval queue
    match principal.category {
        UserCategory::CentuRiskAdmin | UserCategory::PoolAdministrator => {}
        _ => return Err(StatusCode::FORBIDDEN),
    }

    let conn = state.db.get().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let (tenant_clause, tenant_params) = tenant_where(&principal);

    let query = format!(
        "SELECT fm.mutation_id, fm.asset_id, fm.field_name, fm.value_json,
                fm.effective_date, fm.submitted_at, fm.submitted_by,
                a.asset_type,
                (SELECT fm2.value_json FROM field_mutations fm2
                 WHERE fm2.asset_id = fm.asset_id AND fm2.field_name = fm.field_name
                   AND fm2.approval_state = 'Approved'
                 ORDER BY fm2.effective_date DESC LIMIT 1) as prev_value
         FROM field_mutations fm
         JOIN assets a ON a.asset_id = fm.asset_id
         WHERE fm.approval_state = 'Pending' AND {tenant_clause}
         ORDER BY fm.submitted_at DESC"
    );

    let mut stmt = conn.prepare(&query).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    type Row = (String, String, String, String, String, String, String, String, Option<String>);
    let results: Vec<Row> = stmt
        .query_map(rusqlite::params_from_iter(&tenant_params), |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?,
                row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?))
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .filter_map(|r| r.ok())
        .collect();

    // Get asset names (building_name field)
    let mut asset_names: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for (_, asset_id, _, _, _, _, _, asset_type, _) in &results {
        if !asset_names.contains_key(asset_id) {
            let name: Option<String> = conn.query_row(
                "SELECT value_json FROM field_mutations WHERE asset_id = ?1 AND field_name = 'building_name' AND approval_state = 'Approved' ORDER BY effective_date DESC LIMIT 1",
                rusqlite::params![asset_id],
                |row| row.get(0),
            ).ok();
            let display_name = name
                .and_then(|n| serde_json::from_str::<centurisk_core::field_value::FieldValue>(&n).ok())
                .map(|fv| display_field_value(&fv))
                .unwrap_or_else(|| format!("{} {}", asset_type, &asset_id[..8]));
            asset_names.insert(asset_id.clone(), display_name);
        }
    }

    let pending: Vec<PendingMutation> = results.into_iter().map(
        |(mid, aid, fname, vjson, edate, sat, sby, atype, prev)| {
            let proposed_display = serde_json::from_str::<centurisk_core::field_value::FieldValue>(&vjson)
                .map(|fv| display_field_value(&fv))
                .unwrap_or_else(|_| vjson.clone());
            let proposed_raw = serde_json::from_str(&vjson).unwrap_or(serde_json::Value::Null);
            let prev_display = prev.and_then(|p| {
                serde_json::from_str::<centurisk_core::field_value::FieldValue>(&p)
                    .map(|fv| display_field_value(&fv)).ok()
            });

            PendingMutation {
                mutation_id: mid,
                asset_id: aid.clone(),
                asset_name: asset_names.get(&aid).cloned().unwrap_or_default(),
                asset_type: atype,
                field_name: fname.clone(),
                proposed_value: proposed_display,
                proposed_value_raw: proposed_raw,
                previous_value: prev_display,
                effective_date: edate,
                submitted_at: sat,
                submitted_by: sby,
                is_valuation_field: centurisk_core::sov::is_valuation_field(&fname),
            }
        }
    ).collect();

    Ok(Json(pending))
}

/// POST /api/approvals/:mutation_id — approve or reject a pending mutation.
async fn act_on_mutation(
    Auth(principal): Auth,
    State(state): State<AppState>,
    Path(mutation_id): Path<String>,
    Json(action): Json<ApprovalAction>,
) -> Result<Json<ActionResult>, (StatusCode, Json<ErrorResponse>)> {
    match principal.category {
        UserCategory::CentuRiskAdmin | UserCategory::PoolAdministrator => {}
        _ => return Err((StatusCode::FORBIDDEN, Json(ErrorResponse { error: "Not authorized".into() }))),
    }

    let new_state = match action.decision.as_str() {
        "approve" => "Approved",
        "reject" => "Rejected",
        _ => return Err((StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "Decision must be 'approve' or 'reject'".into() }))),
    };

    let conn = state.db.get()
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "DB error".into() })))?;

    // Verify the mutation is pending and belongs to the user's pool
    let (tenant_clause, tenant_params) = tenant_where(&principal);
    let mut params = tenant_params;
    params.push(mutation_id.clone());
    let mid_idx = params.len();

    let check_query = format!(
        "SELECT 1 FROM field_mutations fm JOIN assets a ON a.asset_id = fm.asset_id
         WHERE {tenant_clause} AND fm.mutation_id = ?{mid_idx} AND fm.approval_state = 'Pending'"
    );
    conn.query_row(&check_query, rusqlite::params_from_iter(&params), |_| Ok(()))
        .map_err(|_| (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "Pending mutation not found".into() })))?;

    let now = time::OffsetDateTime::now_utc();
    let approved_at = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        now.year(), now.month() as u8, now.day(),
        now.hour(), now.minute(), now.second()
    );

    conn.execute(
        "UPDATE field_mutations SET approval_state = ?1, approved_at = ?2, approved_by = ?3 WHERE mutation_id = ?4",
        rusqlite::params![new_state, approved_at, principal.actor_id.to_string(), mutation_id],
    ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("Update failed: {e}") })))?;

    Ok(Json(ActionResult {
        mutation_id,
        new_state: new_state.into(),
    }))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/approvals", get(list_pending))
        .route("/api/approvals/{mutation_id}", post(act_on_mutation))
}
