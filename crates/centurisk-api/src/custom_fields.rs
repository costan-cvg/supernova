//! Custom field definitions API — pool-level configuration for additional fields.
//!
//! Custom fields are stored as regular field_mutations (same as built-in fields).
//! The definitions here tell the UI what to render and quality scoring what to check.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::auth::Auth;

// ── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomFieldDefinition {
    pub field_id: String,
    pub pool_id: String,
    pub field_name: String,
    pub field_type: String,
    pub required: bool,
    pub recommended: bool,
    pub asset_types: Vec<String>,
    pub enum_options: Option<Vec<String>>,
    pub created_at: String,
    pub created_by: String,
}

#[derive(Deserialize)]
pub struct CreateCustomFieldRequest {
    pub field_name: String,
    pub field_type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub recommended: bool,
    #[serde(default = "default_asset_types")]
    pub asset_types: Vec<String>,
    pub enum_options: Option<Vec<String>>,
}

fn default_asset_types() -> Vec<String> {
    vec![
        "Building".into(),
        "Contents".into(),
        "Vehicle".into(),
        "FineArts".into(),
    ]
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

// ── Validation ─────────────────────────────────────────────────────────────

const VALID_FIELD_TYPES: &[&str] = &["Text", "Number", "Date", "Boolean", "Enum"];
const VALID_ASSET_TYPES: &[&str] = &["Building", "Contents", "Vehicle", "FineArts"];

fn validate_create_request(req: &CreateCustomFieldRequest) -> Result<(), String> {
    if req.field_name.is_empty() {
        return Err("field_name is required".into());
    }
    // Reject field names with spaces or special chars (must be snake_case-ish)
    if !req
        .field_name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err("field_name must contain only alphanumeric characters and underscores".into());
    }
    if !VALID_FIELD_TYPES.contains(&req.field_type.as_str()) {
        return Err(format!(
            "field_type must be one of: {}",
            VALID_FIELD_TYPES.join(", ")
        ));
    }
    for at in &req.asset_types {
        if !VALID_ASSET_TYPES.contains(&at.as_str()) {
            return Err(format!(
                "invalid asset_type '{}'; must be one of: {}",
                at,
                VALID_ASSET_TYPES.join(", ")
            ));
        }
    }
    if req.field_type == "Enum" {
        match &req.enum_options {
            None => return Err("enum_options required when field_type is Enum".into()),
            Some(opts) if opts.is_empty() => {
                return Err("enum_options must not be empty for Enum fields".into())
            }
            _ => {}
        }
    }
    if req.required && req.recommended {
        return Err("a field cannot be both required and recommended".into());
    }
    Ok(())
}

// ── GET /api/custom-fields ─────────────────────────────────────────────────

/// List custom field definitions for the principal's pool.
#[tracing::instrument(name = "api.list_custom_fields", skip_all)]
async fn list_custom_fields(
    Auth(principal): Auth,
    State(state): State<AppState>,
) -> Result<Json<Vec<CustomFieldDefinition>>, StatusCode> {
    let pool_id = principal
        .pool_id
        .ok_or(StatusCode::BAD_REQUEST)?
        .to_string();
    let conn = state
        .db
        .get()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut stmt = conn
        .prepare(
            "SELECT field_id, pool_id, field_name, field_type, required, recommended, asset_types, enum_options, created_at, created_by
             FROM custom_field_definitions
             WHERE pool_id = ?1
             ORDER BY field_name",
        )
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows: Vec<CustomFieldDefinition> = stmt
        .query_map(rusqlite::params![pool_id], |row| {
            let asset_types_str: String = row.get(6)?;
            let enum_options_str: Option<String> = row.get(7)?;
            Ok(CustomFieldDefinition {
                field_id: row.get(0)?,
                pool_id: row.get(1)?,
                field_name: row.get(2)?,
                field_type: row.get(3)?,
                required: row.get(4)?,
                recommended: row.get(5)?,
                asset_types: asset_types_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect(),
                enum_options: enum_options_str.map(|s| {
                    s.split(',')
                        .map(|o| o.trim().to_string())
                        .filter(|o| !o.is_empty())
                        .collect()
                }),
                created_at: row.get(8)?,
                created_by: row.get(9)?,
            })
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .filter_map(|r| r.ok())
        .collect();

    Ok(Json(rows))
}

// ── POST /api/custom-fields ────────────────────────────────────────────────

/// Create a new custom field definition. Admin only.
#[tracing::instrument(name = "api.create_custom_field", skip_all)]
async fn create_custom_field(
    Auth(principal): Auth,
    State(state): State<AppState>,
    Json(req): Json<CreateCustomFieldRequest>,
) -> Result<(StatusCode, Json<CustomFieldDefinition>), (StatusCode, Json<ErrorResponse>)> {
    // Admin-only check
    if !matches!(
        principal.category,
        centurisk_auth::principal::UserCategory::CentuRiskAdmin
            | centurisk_auth::principal::UserCategory::PoolAdministrator
    ) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "Only administrators can create custom field definitions".into(),
            }),
        ));
    }

    let pool_id = principal
        .pool_id
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "No pool context".into(),
                }),
            )
        })?
        .to_string();

    // Validate request
    if let Err(msg) = validate_create_request(&req) {
        return Err((StatusCode::BAD_REQUEST, Json(ErrorResponse { error: msg })));
    }

    let conn = state.db.get().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "DB error".into(),
            }),
        )
    })?;

    let field_id = uuid::Uuid::now_v7().to_string();
    let asset_types_str = req.asset_types.join(",");
    let enum_options_str = req.enum_options.as_ref().map(|opts| opts.join(","));

    conn.execute(
        "INSERT INTO custom_field_definitions (field_id, pool_id, field_name, field_type, required, recommended, asset_types, enum_options, created_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            field_id,
            pool_id,
            req.field_name,
            req.field_type,
            req.required,
            req.recommended,
            asset_types_str,
            enum_options_str,
            principal.actor_id.to_string(),
        ],
    )
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            (
                StatusCode::CONFLICT,
                Json(ErrorResponse {
                    error: format!(
                        "A custom field named '{}' already exists in this pool",
                        req.field_name
                    ),
                }),
            )
        } else {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Insert failed: {e}"),
                }),
            )
        }
    })?;

    // Read back the created_at timestamp
    let created_at: String = conn
        .query_row(
            "SELECT created_at FROM custom_field_definitions WHERE field_id = ?1",
            rusqlite::params![field_id],
            |row| row.get(0),
        )
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to read created_at".into(),
                }),
            )
        })?;

    Ok((
        StatusCode::CREATED,
        Json(CustomFieldDefinition {
            field_id,
            pool_id,
            field_name: req.field_name,
            field_type: req.field_type,
            required: req.required,
            recommended: req.recommended,
            asset_types: req.asset_types,
            enum_options: req.enum_options,
            created_at,
            created_by: principal.actor_id.to_string(),
        }),
    ))
}

// ── DELETE /api/custom-fields/:id ──────────────────────────────────────────

/// Delete a custom field definition. Admin only.
#[tracing::instrument(name = "api.delete_custom_field", skip_all, fields(field_id = %field_id))]
async fn delete_custom_field(
    Auth(principal): Auth,
    State(state): State<AppState>,
    Path(field_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Admin-only check
    if !matches!(
        principal.category,
        centurisk_auth::principal::UserCategory::CentuRiskAdmin
            | centurisk_auth::principal::UserCategory::PoolAdministrator
    ) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "Only administrators can delete custom field definitions".into(),
            }),
        ));
    }

    let pool_id = principal
        .pool_id
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "No pool context".into(),
                }),
            )
        })?
        .to_string();

    let conn = state.db.get().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "DB error".into(),
            }),
        )
    })?;

    // Delete only if it belongs to this pool
    let rows_affected = conn
        .execute(
            "DELETE FROM custom_field_definitions WHERE field_id = ?1 AND pool_id = ?2",
            rusqlite::params![field_id, pool_id],
        )
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Delete failed: {e}"),
                }),
            )
        })?;

    if rows_affected == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Custom field definition not found".into(),
            }),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

// ── Helper for quality scoring ─────────────────────────────────────────────

/// Load custom field definitions for a given pool from the database.
/// Used by quality scoring to extend completeness configs.
pub fn load_custom_fields_for_pool(
    conn: &rusqlite::Connection,
    pool_id: &str,
) -> Vec<CustomFieldDefinition> {
    let mut stmt = match conn.prepare(
        "SELECT field_id, pool_id, field_name, field_type, required, recommended, asset_types, enum_options, created_at, created_by
         FROM custom_field_definitions
         WHERE pool_id = ?1",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    stmt.query_map(rusqlite::params![pool_id], |row| {
        let asset_types_str: String = row.get(6)?;
        let enum_options_str: Option<String> = row.get(7)?;
        Ok(CustomFieldDefinition {
            field_id: row.get(0)?,
            pool_id: row.get(1)?,
            field_name: row.get(2)?,
            field_type: row.get(3)?,
            required: row.get(4)?,
            recommended: row.get(5)?,
            asset_types: asset_types_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            enum_options: enum_options_str.map(|s| {
                s.split(',')
                    .map(|o| o.trim().to_string())
                    .filter(|o| !o.is_empty())
                    .collect()
            }),
            created_at: row.get(8)?,
            created_by: row.get(9)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

// ── Routes ─────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/custom-fields",
            get(list_custom_fields).post(create_custom_field),
        )
        .route(
            "/api/custom-fields/:id",
            axum::routing::delete(delete_custom_field),
        )
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt as _;

    fn test_state() -> AppState {
        AppState {
            db: centurisk_db::init_test_db().unwrap(),
            policy: std::sync::Arc::new(centurisk_auth::AllowAllPolicy),
        }
    }

    /// Seed a pool and return its pool_id string.
    fn seed_pool(state: &AppState) -> String {
        let conn = state.db.get().unwrap();
        let pool_id = uuid::Uuid::now_v7().to_string();
        conn.execute(
            "INSERT INTO pools (pool_id, name, created_by) VALUES (?1, 'Test Pool', 'system')",
            rusqlite::params![pool_id],
        )
        .unwrap();
        pool_id
    }

    /// Create a JWT for the CentuRisk admin with a specific pool_id.
    fn admin_token(pool_id: &str) -> String {
        use jsonwebtoken::{encode, EncodingKey, Header};
        let claims = crate::auth::Claims {
            sub: "00000000-0000-0000-0000-000000000001".into(),
            name: "RiskStar Admin".into(),
            category: "CentuRiskAdmin".into(),
            pool_id: Some(pool_id.into()),
            member_id: None,
            exp: (jsonwebtoken::get_current_timestamp() as usize) + 3600,
        };
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(b"centurisk-dev-secret-do-not-use-in-production"),
        )
        .unwrap()
    }

    /// Create a JWT for a MemberUser (non-admin).
    fn member_token(pool_id: &str) -> String {
        use jsonwebtoken::{encode, EncodingKey, Header};
        let claims = crate::auth::Claims {
            sub: "00000000-0000-0000-0000-000000000099".into(),
            name: "Member User".into(),
            category: "MemberUser".into(),
            pool_id: Some(pool_id.into()),
            member_id: Some("00000000-0000-0000-0000-000000000088".into()),
            exp: (jsonwebtoken::get_current_timestamp() as usize) + 3600,
        };
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(b"centurisk-dev-secret-do-not-use-in-production"),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn create_and_list_custom_field() {
        let state = test_state();
        let pool_id = seed_pool(&state);
        let token = admin_token(&pool_id);

        let app = crate::app(state);

        // Create a custom field
        let body = serde_json::json!({
            "field_name": "flood_zone",
            "field_type": "Enum",
            "required": true,
            "asset_types": ["Building"],
            "enum_options": ["A", "AE", "V", "X"]
        });

        let req = Request::builder()
            .method("POST")
            .uri("/api/custom-fields")
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let resp_body = axum::body::to_bytes(resp.into_body(), 4096)
            .await
            .unwrap();
        let created: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert_eq!(created["field_name"], "flood_zone");
        assert_eq!(created["field_type"], "Enum");
        assert_eq!(created["required"], true);
        assert_eq!(created["asset_types"], serde_json::json!(["Building"]));
        assert_eq!(
            created["enum_options"],
            serde_json::json!(["A", "AE", "V", "X"])
        );

        // List and verify the field exists
        let list_req = Request::builder()
            .uri("/api/custom-fields")
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        let list_resp = app.clone().oneshot(list_req).await.unwrap();
        assert_eq!(list_resp.status(), StatusCode::OK);

        let list_body = axum::body::to_bytes(list_resp.into_body(), 4096)
            .await
            .unwrap();
        let list: Vec<serde_json::Value> = serde_json::from_slice(&list_body).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["field_name"], "flood_zone");
    }

    #[tokio::test]
    async fn delete_custom_field() {
        let state = test_state();
        let pool_id = seed_pool(&state);
        let token = admin_token(&pool_id);

        let app = crate::app(state);

        // Create a field first
        let body = serde_json::json!({
            "field_name": "temp_field",
            "field_type": "Text",
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/custom-fields")
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let resp_body = axum::body::to_bytes(resp.into_body(), 4096)
            .await
            .unwrap();
        let created: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        let field_id = created["field_id"].as_str().unwrap();

        // Delete the field
        let del_req = Request::builder()
            .method("DELETE")
            .uri(format!("/api/custom-fields/{}", field_id))
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();
        let del_resp = app.clone().oneshot(del_req).await.unwrap();
        assert_eq!(del_resp.status(), StatusCode::NO_CONTENT);

        // Verify it's gone
        let list_req = Request::builder()
            .uri("/api/custom-fields")
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();
        let list_resp = app.clone().oneshot(list_req).await.unwrap();
        let list_body = axum::body::to_bytes(list_resp.into_body(), 4096)
            .await
            .unwrap();
        let list: Vec<serde_json::Value> = serde_json::from_slice(&list_body).unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn non_admin_cannot_create_custom_field() {
        let state = test_state();
        let pool_id = seed_pool(&state);
        let token = member_token(&pool_id);

        let app = crate::app(state);

        let body = serde_json::json!({
            "field_name": "should_fail",
            "field_type": "Text",
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/custom-fields")
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn validation_rejects_invalid_field_type() {
        let state = test_state();
        let pool_id = seed_pool(&state);
        let token = admin_token(&pool_id);

        let app = crate::app(state);

        let body = serde_json::json!({
            "field_name": "bad_type",
            "field_type": "Currency",
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/custom-fields")
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn validation_rejects_enum_without_options() {
        let state = test_state();
        let pool_id = seed_pool(&state);
        let token = admin_token(&pool_id);

        let app = crate::app(state);

        let body = serde_json::json!({
            "field_name": "bad_enum",
            "field_type": "Enum",
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/custom-fields")
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn custom_fields_affect_quality_completeness_scoring() {
        let state = test_state();
        let pool_id = seed_pool(&state);
        let token = admin_token(&pool_id);

        let member_id = uuid::Uuid::now_v7().to_string();
        let asset_id = uuid::Uuid::now_v7().to_string();
        let path = format!("/{}/{}/{}", pool_id, member_id, asset_id);
        let today = "2026-01-01";

        // Seed data in a block so conn is dropped before making HTTP requests
        {
            let conn = state.db.get().unwrap();

            conn.execute(
                "INSERT INTO members (member_id, pool_id, name, created_by) VALUES (?1, ?2, 'Test Member', 'system')",
                rusqlite::params![member_id, pool_id],
            )
            .unwrap();

            conn.execute(
                "INSERT INTO assets (asset_id, pool_id, member_id, path, asset_type, lifecycle, created_by)
                 VALUES (?1, ?2, ?3, ?4, 'Building', 'Active', 'system')",
                rusqlite::params![asset_id, pool_id, member_id, path],
            )
            .unwrap();

            // Add the minimal required built-in fields so we can measure the effect of custom fields
            let required_fields = [
                ("building_name", r#"{"type":"Text","value":"HQ"}"#),
                ("address", r#"{"type":"Text","value":"1 Main St"}"#),
                ("city", r#"{"type":"Text","value":"Springfield"}"#),
                ("state", r#"{"type":"Enum","value":"IL"}"#),
                ("zip_code", r#"{"type":"Text","value":"62701"}"#),
                (
                    "replacement_cost",
                    r#"{"type":"Money","value":{"amount":"500000","currency":"USD"}}"#,
                ),
            ];
            for (fname, vjson) in &required_fields {
                let mid = uuid::Uuid::now_v7().to_string();
                conn.execute(
                    "INSERT INTO field_mutations (mutation_id, asset_id, field_name, value_json, effective_date, submitted_by, approval_state)
                     VALUES (?1, ?2, ?3, ?4, ?5, 'system', 'Approved')",
                    rusqlite::params![mid, asset_id, fname, vjson, today],
                )
                .unwrap();
            }
        } // conn dropped here

        let app = crate::app(state.clone());

        // Get quality score WITHOUT custom fields
        let qual_req = Request::builder()
            .uri(format!("/api/assets/{}/quality", asset_id))
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();
        let qual_resp = app.clone().oneshot(qual_req).await.unwrap();
        assert_eq!(qual_resp.status(), StatusCode::OK);
        let qual_body = axum::body::to_bytes(qual_resp.into_body(), 4096)
            .await
            .unwrap();
        let qual: serde_json::Value = serde_json::from_slice(&qual_body).unwrap();
        let missing_required_before = qual["completeness"]["missing_required"]
            .as_array()
            .unwrap()
            .len();

        // Now create a required custom field "seismic_zone" for Building
        let body = serde_json::json!({
            "field_name": "seismic_zone",
            "field_type": "Enum",
            "required": true,
            "asset_types": ["Building"],
            "enum_options": ["Zone1", "Zone2", "Zone3"]
        });
        let create_req = Request::builder()
            .method("POST")
            .uri("/api/custom-fields")
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();
        let create_resp = app.clone().oneshot(create_req).await.unwrap();
        assert_eq!(create_resp.status(), StatusCode::CREATED);

        // Get quality score WITH the new required custom field
        let qual_req2 = Request::builder()
            .uri(format!("/api/assets/{}/quality", asset_id))
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();
        let qual_resp2 = app.clone().oneshot(qual_req2).await.unwrap();
        assert_eq!(qual_resp2.status(), StatusCode::OK);
        let qual_body2 = axum::body::to_bytes(qual_resp2.into_body(), 4096)
            .await
            .unwrap();
        let qual2: serde_json::Value = serde_json::from_slice(&qual_body2).unwrap();
        let missing_required_after = qual2["completeness"]["missing_required"]
            .as_array()
            .unwrap();

        // The new custom field should appear in missing_required
        let missing_names: Vec<&str> = missing_required_after
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(
            missing_names.contains(&"seismic_zone"),
            "seismic_zone should be in missing_required; got: {:?}",
            missing_names
        );

        // Should have one more missing required than before
        assert_eq!(missing_required_after.len(), missing_required_before + 1);
    }

    #[tokio::test]
    async fn custom_field_values_stored_and_retrievable() {
        let state = test_state();
        let pool_id = seed_pool(&state);
        let token = admin_token(&pool_id);

        let member_id = uuid::Uuid::now_v7().to_string();
        let asset_id = uuid::Uuid::now_v7().to_string();
        let path = format!("/{}/{}/{}", pool_id, member_id, asset_id);

        // Seed data in a block so conn is dropped before making HTTP requests
        {
            let conn = state.db.get().unwrap();

            conn.execute(
                "INSERT INTO members (member_id, pool_id, name, created_by) VALUES (?1, ?2, 'Test Member', 'system')",
                rusqlite::params![member_id, pool_id],
            )
            .unwrap();

            conn.execute(
                "INSERT INTO assets (asset_id, pool_id, member_id, path, asset_type, lifecycle, created_by)
                 VALUES (?1, ?2, ?3, ?4, 'Building', 'Active', 'system')",
                rusqlite::params![asset_id, pool_id, member_id, path],
            )
            .unwrap();
        } // conn dropped here

        let app = crate::app(state.clone());

        // Create a custom field definition
        let cf_body = serde_json::json!({
            "field_name": "soil_type",
            "field_type": "Text",
            "recommended": true,
            "asset_types": ["Building"]
        });
        let cf_req = Request::builder()
            .method("POST")
            .uri("/api/custom-fields")
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::from(serde_json::to_string(&cf_body).unwrap()))
            .unwrap();
        let cf_resp = app.clone().oneshot(cf_req).await.unwrap();
        assert_eq!(cf_resp.status(), StatusCode::CREATED);

        // Store a value for the custom field via the standard edit_fields endpoint
        let edit_body = serde_json::json!({
            "fields": {
                "soil_type": "Sandy loam"
            }
        });
        let edit_req = Request::builder()
            .method("PUT")
            .uri(format!("/api/assets/{}/fields", asset_id))
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::from(serde_json::to_string(&edit_body).unwrap()))
            .unwrap();
        let edit_resp = app.clone().oneshot(edit_req).await.unwrap();
        assert_eq!(edit_resp.status(), StatusCode::OK);

        // Retrieve the asset and verify the custom field value is present
        let get_req = Request::builder()
            .uri(format!("/api/assets/{}", asset_id))
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();
        let get_resp = app.clone().oneshot(get_req).await.unwrap();
        assert_eq!(get_resp.status(), StatusCode::OK);
        let get_body = axum::body::to_bytes(get_resp.into_body(), 4096)
            .await
            .unwrap();
        let asset: serde_json::Value = serde_json::from_slice(&get_body).unwrap();
        assert_eq!(
            asset["fields"]["soil_type"], "Sandy loam",
            "Custom field value should be retrievable"
        );
    }

    #[tokio::test]
    async fn duplicate_field_name_returns_conflict() {
        let state = test_state();
        let pool_id = seed_pool(&state);
        let token = admin_token(&pool_id);

        let app = crate::app(state);

        let body = serde_json::json!({
            "field_name": "dup_field",
            "field_type": "Text",
        });

        // First create
        let req = Request::builder()
            .method("POST")
            .uri("/api/custom-fields")
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        // Duplicate create
        let req2 = Request::builder()
            .method("POST")
            .uri("/api/custom-fields")
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();
        let resp2 = app.oneshot(req2).await.unwrap();
        assert_eq!(resp2.status(), StatusCode::CONFLICT);
    }
}
