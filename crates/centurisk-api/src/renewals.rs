//! Renewal workflow — proposed valuations, member review, flags, bulk approval.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use centurisk_core::ids::MutationId;
use serde::{Deserialize, Serialize};

use crate::assets::{display_field_value, parse_field_value, tenant_where};
use crate::auth::Auth;
use crate::AppState;

// ── Types ───────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct RenewalSummary {
    pub renewal_id: String,
    pub name: String,
    pub status: String,
    pub total_proposals: usize,
    pub approved: usize,
    pub flagged: usize,
    pub pending: usize,
}

#[derive(Serialize)]
pub struct ProposalView {
    pub proposal_id: String,
    pub asset_id: String,
    pub asset_name: String,
    pub field_name: String,
    pub current_value: Option<String>,
    pub proposed_value: String,
    pub member_decision: Option<String>,
}

#[derive(Serialize)]
pub struct FlagView {
    pub flag_id: String,
    pub asset_id: String,
    pub asset_name: String,
    pub field_name: Option<String>,
    pub member_note: String,
    pub state: String,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct CreateRenewalRequest {
    pub name: String,
    /// Proposed values: list of { asset_id, field_name, proposed_value }
    pub proposals: Vec<ProposalInput>,
}

#[derive(Deserialize)]
pub struct ProposalInput {
    pub asset_id: String,
    pub field_name: String,
    pub proposed_value: String,
}

#[derive(Deserialize)]
pub struct MemberDecisionRequest {
    /// "approve", "flag"
    pub decision: String,
    /// Note required when flagging
    pub note: Option<String>,
}

#[derive(Deserialize)]
pub struct ResolveFlagRequest {
    // Empty for now — just marks resolved
}

#[derive(Deserialize)]
pub struct BulkApproveRequest {
    // Approves all unflagged proposals in this renewal
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Serialize)]
struct ActionResult {
    success: bool,
    message: String,
}

// ── Helper ──────────────────────────────────────────────────────────────────

fn asset_name(conn: &rusqlite::Connection, asset_id: &str) -> String {
    conn.query_row(
        "SELECT value_json FROM field_mutations WHERE asset_id = ?1 AND field_name = 'building_name' AND approval_state = 'Approved' ORDER BY effective_date DESC LIMIT 1",
        rusqlite::params![asset_id],
        |row| row.get::<_, String>(0),
    ).ok()
    .and_then(|j| serde_json::from_str::<centurisk_core::field_value::FieldValue>(&j).ok())
    .map(|fv| display_field_value(&fv))
    .unwrap_or_else(|| asset_id[..8].to_string())
}

// ── POST /api/renewals ──────────────────────────────────────────────────────

async fn create_renewal(
    Auth(principal): Auth,
    State(state): State<AppState>,
    Json(req): Json<CreateRenewalRequest>,
) -> Result<(StatusCode, Json<RenewalSummary>), (StatusCode, Json<ErrorResponse>)> {
    let conn = state.db.get()
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "DB error".into() })))?;

    let pool_id = principal.pool_id
        .ok_or_else(|| (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "No pool context".into() })))?;

    let renewal_id = uuid::Uuid::now_v7().to_string();

    conn.execute(
        "INSERT INTO renewals (renewal_id, pool_id, name, created_by) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![renewal_id, pool_id.to_string(), req.name, principal.actor_id.to_string()],
    ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("Create failed: {e}") })))?;

    for p in &req.proposals {
        let proposal_id = uuid::Uuid::now_v7().to_string();

        // Get current value for comparison
        let current: Option<String> = conn.query_row(
            "SELECT value_json FROM field_mutations WHERE asset_id = ?1 AND field_name = ?2 AND approval_state = 'Approved' ORDER BY effective_date DESC LIMIT 1",
            rusqlite::params![p.asset_id, p.field_name],
            |row| row.get(0),
        ).ok();

        let fv = parse_field_value(&p.field_name, &p.proposed_value);
        let proposed_json = serde_json::to_string(&fv).unwrap();

        conn.execute(
            "INSERT INTO renewal_proposals (proposal_id, renewal_id, asset_id, field_name, proposed_value, current_value)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![proposal_id, renewal_id, p.asset_id, p.field_name, proposed_json, current],
        ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("Proposal failed: {e}") })))?;
    }

    Ok((StatusCode::CREATED, Json(RenewalSummary {
        renewal_id,
        name: req.name,
        status: "Open".into(),
        total_proposals: req.proposals.len(),
        approved: 0,
        flagged: 0,
        pending: req.proposals.len(),
    })))
}

// ── GET /api/renewals ───────────────────────────────────────────────────────

async fn list_renewals(
    Auth(principal): Auth,
    State(state): State<AppState>,
) -> Result<Json<Vec<RenewalSummary>>, StatusCode> {
    let conn = state.db.get().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let (tenant_clause, tenant_params) = tenant_where(&principal);

    // Map 'a.' prefix to 'r.' for the renewals table
    let renewal_clause = tenant_clause.replace("a.pool_id", "r.pool_id").replace("a.member_id", "r.member_id");

    let query = format!(
        "SELECT r.renewal_id, r.name, r.status FROM renewals r WHERE {renewal_clause} ORDER BY r.created_at DESC"
    );
    let mut stmt = conn.prepare(&query).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let renewals: Vec<(String, String, String)> = stmt
        .query_map(rusqlite::params_from_iter(&tenant_params), |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .filter_map(|r| r.ok())
        .collect();

    let mut results = Vec::new();
    for (rid, name, status) in renewals {
        let total: usize = conn.query_row(
            "SELECT COUNT(*) FROM renewal_proposals WHERE renewal_id = ?1", rusqlite::params![rid], |r| r.get(0)
        ).unwrap_or(0);
        let approved: usize = conn.query_row(
            "SELECT COUNT(*) FROM renewal_proposals WHERE renewal_id = ?1 AND member_decision = 'Approved'", rusqlite::params![rid], |r| r.get(0)
        ).unwrap_or(0);
        let flagged: usize = conn.query_row(
            "SELECT COUNT(*) FROM renewal_flags WHERE renewal_id = ?1 AND state = 'Open'", rusqlite::params![rid], |r| r.get(0)
        ).unwrap_or(0);

        results.push(RenewalSummary {
            renewal_id: rid, name, status,
            total_proposals: total, approved, flagged, pending: total - approved,
        });
    }

    Ok(Json(results))
}

// ── GET /api/renewals/:id/proposals ─────────────────────────────────────────

async fn get_proposals(
    Auth(_principal): Auth,
    State(state): State<AppState>,
    Path(renewal_id): Path<String>,
) -> Result<Json<Vec<ProposalView>>, StatusCode> {
    let conn = state.db.get().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut stmt = conn.prepare(
        "SELECT p.proposal_id, p.asset_id, p.field_name, p.current_value, p.proposed_value, p.member_decision
         FROM renewal_proposals p WHERE p.renewal_id = ?1 ORDER BY p.asset_id, p.field_name"
    ).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let proposals: Vec<ProposalView> = stmt
        .query_map(rusqlite::params![renewal_id], |row| {
            Ok((row.get::<_,String>(0)?, row.get::<_,String>(1)?, row.get::<_,String>(2)?,
                row.get::<_,Option<String>>(3)?, row.get::<_,String>(4)?, row.get::<_,Option<String>>(5)?))
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .filter_map(|r| r.ok())
        .map(|(pid, aid, fname, curr, prop, dec)| {
            let name = asset_name(&conn, &aid);
            let current_display = curr.and_then(|c| serde_json::from_str::<centurisk_core::field_value::FieldValue>(&c).ok()).map(|fv| display_field_value(&fv));
            let proposed_display = serde_json::from_str::<centurisk_core::field_value::FieldValue>(&prop).map(|fv| display_field_value(&fv)).unwrap_or(prop);
            ProposalView { proposal_id: pid, asset_id: aid, asset_name: name, field_name: fname, current_value: current_display, proposed_value: proposed_display, member_decision: dec }
        })
        .collect();

    Ok(Json(proposals))
}

// ── POST /api/renewals/:renewal_id/proposals/:proposal_id/decide ────────────

async fn decide_proposal(
    Auth(principal): Auth,
    State(state): State<AppState>,
    Path((renewal_id, proposal_id)): Path<(String, String)>,
    Json(req): Json<MemberDecisionRequest>,
) -> Result<Json<ActionResult>, (StatusCode, Json<ErrorResponse>)> {
    let conn = state.db.get()
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "DB error".into() })))?;

    let now = time::OffsetDateTime::now_utc();
    let now_str = format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", now.year(), now.month() as u8, now.day(), now.hour(), now.minute(), now.second());

    match req.decision.as_str() {
        "approve" => {
            // Get the proposal details
            let (asset_id, field_name, proposed_value): (String, String, String) = conn.query_row(
                "SELECT asset_id, field_name, proposed_value FROM renewal_proposals WHERE proposal_id = ?1 AND renewal_id = ?2",
                rusqlite::params![proposal_id, renewal_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            ).map_err(|_| (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "Proposal not found".into() })))?;

            // Create a field mutation through the SOV pipeline (source: renewal)
            let mutation_id = MutationId::new();
            let today = now.date().to_string();

            // Renewal approvals go through as Pending (valuation changes always pend per SOV rules)
            let approval_state = if centurisk_core::sov::is_valuation_field(&field_name) { "Pending" } else { "Approved" };

            conn.execute(
                "INSERT INTO field_mutations (mutation_id, asset_id, field_name, value_json, effective_date, submitted_at, submitted_by, approval_state)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![mutation_id.to_string(), asset_id, field_name, proposed_value, today, now_str, principal.actor_id.to_string(), approval_state],
            ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("Mutation failed: {e}") })))?;

            conn.execute(
                "UPDATE renewal_proposals SET member_decision = 'Approved', decided_at = ?1 WHERE proposal_id = ?2",
                rusqlite::params![now_str, proposal_id],
            ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("Update failed: {e}") })))?;

            Ok(Json(ActionResult { success: true, message: format!("Approved. Mutation {approval_state}.") }))
        }
        "flag" => {
            let note = req.note.unwrap_or_else(|| "Flagged for discussion".into());

            // Get asset_id from proposal
            let asset_id: String = conn.query_row(
                "SELECT asset_id FROM renewal_proposals WHERE proposal_id = ?1 AND renewal_id = ?2",
                rusqlite::params![proposal_id, renewal_id],
                |row| row.get(0),
            ).map_err(|_| (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "Proposal not found".into() })))?;

            let flag_id = uuid::Uuid::now_v7().to_string();
            conn.execute(
                "INSERT INTO renewal_flags (flag_id, renewal_id, asset_id, member_note, created_by) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![flag_id, renewal_id, asset_id, note, principal.actor_id.to_string()],
            ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("Flag failed: {e}") })))?;

            conn.execute(
                "UPDATE renewal_proposals SET member_decision = 'Flagged', decided_at = ?1 WHERE proposal_id = ?2",
                rusqlite::params![now_str, proposal_id],
            ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("Update failed: {e}") })))?;

            Ok(Json(ActionResult { success: true, message: "Flagged for discussion.".into() }))
        }
        _ => Err((StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "Decision must be 'approve' or 'flag'".into() }))),
    }
}

// ── GET /api/renewals/:id/flags ─────────────────────────────────────────────

async fn get_flags(
    Auth(_principal): Auth,
    State(state): State<AppState>,
    Path(renewal_id): Path<String>,
) -> Result<Json<Vec<FlagView>>, StatusCode> {
    let conn = state.db.get().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut stmt = conn.prepare(
        "SELECT flag_id, asset_id, field_name, member_note, state, created_at FROM renewal_flags WHERE renewal_id = ?1 ORDER BY created_at DESC"
    ).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let flags: Vec<FlagView> = stmt
        .query_map(rusqlite::params![renewal_id], |row| {
            Ok((row.get::<_,String>(0)?, row.get::<_,String>(1)?, row.get::<_,Option<String>>(2)?,
                row.get::<_,String>(3)?, row.get::<_,String>(4)?, row.get::<_,String>(5)?))
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .filter_map(|r| r.ok())
        .map(|(fid, aid, fname, note, st, cat)| {
            FlagView { flag_id: fid, asset_id: aid.clone(), asset_name: asset_name(&conn, &aid), field_name: fname, member_note: note, state: st, created_at: cat }
        })
        .collect();

    Ok(Json(flags))
}

// ── POST /api/renewals/:id/flags/:flag_id/resolve ───────────────────────────

async fn resolve_flag(
    Auth(principal): Auth,
    State(state): State<AppState>,
    Path((renewal_id, flag_id)): Path<(String, String)>,
) -> Result<Json<ActionResult>, (StatusCode, Json<ErrorResponse>)> {
    let conn = state.db.get()
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "DB error".into() })))?;

    let now = time::OffsetDateTime::now_utc();
    let now_str = format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", now.year(), now.month() as u8, now.day(), now.hour(), now.minute(), now.second());

    conn.execute(
        "UPDATE renewal_flags SET state = 'Resolved', resolved_at = ?1, resolved_by = ?2 WHERE flag_id = ?3 AND renewal_id = ?4 AND state = 'Open'",
        rusqlite::params![now_str, principal.actor_id.to_string(), flag_id, renewal_id],
    ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: format!("Resolve failed: {e}") })))?;

    Ok(Json(ActionResult { success: true, message: "Flag resolved.".into() }))
}

// ── POST /api/renewals/:id/bulk-approve ─────────────────────────────────────

async fn bulk_approve(
    Auth(principal): Auth,
    State(state): State<AppState>,
    Path(renewal_id): Path<String>,
) -> Result<Json<ActionResult>, (StatusCode, Json<ErrorResponse>)> {
    let conn = state.db.get()
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "DB error".into() })))?;

    // Check for open flags — bulk approve only if none
    let open_flags: usize = conn.query_row(
        "SELECT COUNT(*) FROM renewal_flags WHERE renewal_id = ?1 AND state = 'Open'",
        rusqlite::params![renewal_id], |r| r.get(0),
    ).unwrap_or(0);

    if open_flags > 0 {
        return Err((StatusCode::CONFLICT, Json(ErrorResponse {
            error: format!("{open_flags} unresolved flag(s). Resolve all flags before bulk approval."),
        })));
    }

    // Approve all undecided proposals
    let now = time::OffsetDateTime::now_utc();
    let now_str = format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", now.year(), now.month() as u8, now.day(), now.hour(), now.minute(), now.second());
    let today = now.date().to_string();

    let mut stmt = conn.prepare(
        "SELECT proposal_id, asset_id, field_name, proposed_value FROM renewal_proposals WHERE renewal_id = ?1 AND member_decision IS NULL"
    ).map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Query failed".into() })))?;

    let pending: Vec<(String, String, String, String)> = stmt
        .query_map(rusqlite::params![renewal_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Query failed".into() })))?
        .filter_map(|r| r.ok())
        .collect();

    let mut approved_count = 0;
    for (pid, asset_id, field_name, proposed_value) in &pending {
        let mutation_id = MutationId::new();
        let approval_state = if centurisk_core::sov::is_valuation_field(field_name) { "Pending" } else { "Approved" };

        let _ = conn.execute(
            "INSERT INTO field_mutations (mutation_id, asset_id, field_name, value_json, effective_date, submitted_at, submitted_by, approval_state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![mutation_id.to_string(), asset_id, field_name, proposed_value, today, now_str, principal.actor_id.to_string(), approval_state],
        );

        let _ = conn.execute(
            "UPDATE renewal_proposals SET member_decision = 'Approved', decided_at = ?1 WHERE proposal_id = ?2",
            rusqlite::params![now_str, pid],
        );

        approved_count += 1;
    }

    Ok(Json(ActionResult {
        success: true,
        message: format!("Bulk approved {approved_count} proposals."),
    }))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/renewals", get(list_renewals).post(create_renewal))
        .route("/api/renewals/:id/proposals", get(get_proposals))
        .route("/api/renewals/:renewal_id/proposals/:proposal_id/decide", post(decide_proposal))
        .route("/api/renewals/:id/flags", get(get_flags))
        .route("/api/renewals/:renewal_id/flags/:flag_id/resolve", post(resolve_flag))
        .route("/api/renewals/:id/bulk-approve", post(bulk_approve))
}
