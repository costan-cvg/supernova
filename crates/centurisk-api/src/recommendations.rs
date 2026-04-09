//! Recommendations and loss events API endpoints.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use centurisk_core::field_value::FieldValue;
use centurisk_core::loss_event::{self, LossEvent};
use centurisk_core::quality::{self, QualityScore};
use centurisk_core::recommendation::{self, Recommendation};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::AppState;
use crate::auth::Auth;

// ── Response / Request types ───────────────────────────────────────────────

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Serialize)]
struct RecommendationsResponse {
    asset_id: String,
    recommendations: Vec<Recommendation>,
}

#[derive(Deserialize)]
pub struct CreateLossEventRequest {
    pub event_type: String,
    pub event_date: String,
    pub severity: String,
    pub description: String,
}

#[derive(Serialize)]
pub struct LossEventResponse {
    pub event_id: String,
    pub asset_id: String,
    pub event_type: String,
    pub event_date: String,
    pub severity: String,
    pub description: String,
    pub created_at: String,
    pub created_by: String,
}

// ── GET /api/assets/:id/recommendations ────────────────────────────────────

/// Compute and return recommendations for an asset based on its current field
/// data and quality scores.
async fn asset_recommendations(
    Auth(principal): Auth,
    State(state): State<AppState>,
    Path(asset_id): Path<String>,
) -> Result<Json<RecommendationsResponse>, StatusCode> {
    let conn = state.db.get().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Verify asset exists and get type + tenant access
    let (tenant_clause, tenant_params) = crate::assets::tenant_where(&principal);
    let mut check_params = tenant_params;
    check_params.push(asset_id.clone());
    let aid_idx = check_params.len();

    let asset_type: String = conn
        .query_row(
            &format!(
                "SELECT asset_type FROM assets a WHERE {tenant_clause} AND a.asset_id = ?{aid_idx}"
            ),
            rusqlite::params_from_iter(&check_params),
            |row| row.get(0),
        )
        .map_err(|_| StatusCode::NOT_FOUND)?;

    // Get current approved fields
    let mut stmt = conn
        .prepare(
            "SELECT field_name, value_json FROM field_mutations
             WHERE asset_id = ?1 AND approval_state = 'Approved'
             ORDER BY field_name, effective_date DESC",
        )
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows: Vec<(String, String)> = stmt
        .query_map(rusqlite::params![asset_id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
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

    // Compute quality score (same logic as quality endpoint)
    let comp_config = match asset_type.as_str() {
        "Building" => quality::building_completeness_config(),
        "Vehicle" => quality::vehicle_completeness_config(),
        "Contents" => quality::contents_completeness_config(),
        _ => quality::building_completeness_config(),
    };
    let completeness = quality::score_completeness(&fields, &comp_config);
    let accuracy = quality::score_accuracy(&fields, &quality::default_accuracy_rules());

    // Recency: compute days since last mutation per tracked field
    let mut field_ages: HashMap<String, u32> = HashMap::new();
    let today = time::OffsetDateTime::now_utc().date();

    let mut age_stmt = conn
        .prepare(
            "SELECT field_name, MAX(effective_date) FROM field_mutations
             WHERE asset_id = ?1 AND approval_state = 'Approved'
             GROUP BY field_name",
        )
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let age_rows: Vec<(String, String)> = age_stmt
        .query_map(rusqlite::params![asset_id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
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
    let composite = completeness.score * 0.4 + accuracy.score * 0.3 + recency.score * 0.3;

    let quality_score = QualityScore {
        completeness,
        accuracy,
        recency,
        composite,
    };

    // Generate recommendations
    let recommendations = recommendation::recommend(&fields, &quality_score);

    Ok(Json(RecommendationsResponse {
        asset_id,
        recommendations,
    }))
}

// ── POST /api/assets/:id/loss-events ───────────────────────────────────────

/// Record a loss event for an asset.
async fn create_loss_event(
    Auth(principal): Auth,
    State(state): State<AppState>,
    Path(asset_id): Path<String>,
    Json(req): Json<CreateLossEventRequest>,
) -> Result<(StatusCode, Json<LossEventResponse>), (StatusCode, Json<ErrorResponse>)> {
    let conn = state.db.get().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "DB error".into(),
            }),
        )
    })?;

    // Verify asset exists and belongs to tenant
    let (tenant_clause, tenant_params) = crate::assets::tenant_where(&principal);
    let mut check_params = tenant_params;
    check_params.push(asset_id.clone());
    let aid_idx = check_params.len();

    conn.query_row(
        &format!(
            "SELECT 1 FROM assets a WHERE {tenant_clause} AND a.asset_id = ?{aid_idx}"
        ),
        rusqlite::params_from_iter(&check_params),
        |_| Ok(()),
    )
    .map_err(|_| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Asset not found".into(),
            }),
        )
    })?;

    // Validate the loss event
    let event = LossEvent {
        asset_id: asset_id.clone(),
        event_type: req.event_type.clone(),
        date: req.event_date.clone(),
        severity: req.severity.clone(),
        description: req.description.clone(),
    };

    let errors = loss_event::validate_loss_event(&event);
    if !errors.is_empty() {
        let msg = errors.iter().map(|e| e.0.as_str()).collect::<Vec<_>>().join("; ");
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { error: msg }),
        ));
    }

    let event_id = uuid::Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO loss_events (event_id, asset_id, event_type, event_date, severity, description, created_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            event_id,
            asset_id,
            req.event_type,
            req.event_date,
            req.severity,
            req.description,
            principal.actor_id.to_string(),
        ],
    )
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Insert failed: {e}"),
            }),
        )
    })?;

    // Read back the created row for created_at
    let created_at: String = conn
        .query_row(
            "SELECT created_at FROM loss_events WHERE event_id = ?1",
            rusqlite::params![event_id],
            |row| row.get(0),
        )
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to read back created event".into(),
                }),
            )
        })?;

    Ok((
        StatusCode::CREATED,
        Json(LossEventResponse {
            event_id,
            asset_id,
            event_type: req.event_type,
            event_date: req.event_date,
            severity: req.severity,
            description: req.description,
            created_at,
            created_by: principal.actor_id.to_string(),
        }),
    ))
}

// ── GET /api/assets/:id/loss-events ────────────────────────────────────────

/// List loss events for an asset.
async fn list_loss_events(
    Auth(principal): Auth,
    State(state): State<AppState>,
    Path(asset_id): Path<String>,
) -> Result<Json<Vec<LossEventResponse>>, StatusCode> {
    let conn = state.db.get().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Verify asset exists and belongs to tenant
    let (tenant_clause, tenant_params) = crate::assets::tenant_where(&principal);
    let mut check_params = tenant_params;
    check_params.push(asset_id.clone());
    let aid_idx = check_params.len();

    conn.query_row(
        &format!(
            "SELECT 1 FROM assets a WHERE {tenant_clause} AND a.asset_id = ?{aid_idx}"
        ),
        rusqlite::params_from_iter(&check_params),
        |_| Ok(()),
    )
    .map_err(|_| StatusCode::NOT_FOUND)?;

    let mut stmt = conn
        .prepare(
            "SELECT event_id, asset_id, event_type, event_date, severity, description, created_at, created_by
             FROM loss_events
             WHERE asset_id = ?1
             ORDER BY event_date DESC, created_at DESC",
        )
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let events: Vec<LossEventResponse> = stmt
        .query_map(rusqlite::params![asset_id], |row| {
            Ok(LossEventResponse {
                event_id: row.get(0)?,
                asset_id: row.get(1)?,
                event_type: row.get(2)?,
                event_date: row.get(3)?,
                severity: row.get(4)?,
                description: row.get(5)?,
                created_at: row.get(6)?,
                created_by: row.get(7)?,
            })
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .filter_map(|r| r.ok())
        .collect();

    Ok(Json(events))
}

// ── Helper ─────────────────────────────────────────────────────────────────

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

// ── Routes ─────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/assets/:id/recommendations",
            get(asset_recommendations),
        )
        .route(
            "/api/assets/:id/loss-events",
            get(list_loss_events).post(create_loss_event),
        )
}
