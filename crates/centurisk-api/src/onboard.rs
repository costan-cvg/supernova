//! Pool onboarding: import pool definition + member SOV CSV files.
//!
//! This is the proper entry point for getting data into CentuRisk.
//! Mirrors the real workflow: pool admin submits CSV files, system
//! creates pool/member/asset records through the SOV pipeline path.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use centurisk_core::field_value::FieldValue;
use centurisk_core::ids::{ActorId, AssetId, MemberId, MutationId, PoolId};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

use crate::AppState;

// ── Request / Response types ────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct OnboardPoolRequest {
    /// Pool name
    pub pool_name: String,
    /// Members with their SOV data
    pub members: Vec<OnboardMemberRequest>,
}

#[derive(Deserialize)]
pub struct OnboardMemberRequest {
    /// Member organization name
    pub member_name: String,
    /// SOV data as CSV string (same format as the sample .csv files)
    pub sov_csv: String,
}

#[derive(Serialize)]
pub struct OnboardResult {
    pub pool_id: String,
    pub pool_name: String,
    pub members: Vec<MemberImportResult>,
    pub total_assets: usize,
    pub errors: Vec<String>,
}

#[derive(Serialize)]
pub struct MemberImportResult {
    pub member_id: String,
    pub member_name: String,
    pub assets_imported: usize,
    pub errors: Vec<String>,
}

// ── CSV Row ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct SovRow {
    asset_type: String,
    #[serde(default)]
    building_name: String,
    #[serde(default)]
    address: String,
    #[serde(default)]
    city: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    zip_code: String,
    #[serde(default)]
    year_built: String,
    #[serde(default)]
    construction_class: String,
    #[serde(default)]
    occupancy: String,
    #[serde(default)]
    sq_footage: String,
    #[serde(default)]
    stories: String,
    #[serde(default)]
    replacement_cost: String,
    #[serde(default)]
    sprinkler: String,
    #[serde(default)]
    roof_type: String,
    #[serde(default)]
    contents_value: String,
    /// Stable key to match rows across years for the same asset.
    /// Rows sharing an asset_key become mutations on one asset.
    /// If empty, each row creates a new asset.
    #[serde(default)]
    asset_key: String,
    /// Effective date for this row's field values (e.g. "2024-01-01").
    /// If empty, defaults to today.
    #[serde(default)]
    effective_date: String,
}

impl SovRow {
    /// Convert CSV row fields into typed FieldValue mutations.
    fn to_field_mutations(&self) -> Vec<(String, FieldValue)> {
        let mut fields = Vec::new();

        let text_fields = [
            ("building_name", &self.building_name),
            ("address", &self.address),
            ("city", &self.city),
            ("state", &self.state),
            ("zip_code", &self.zip_code),
        ];
        for (name, val) in text_fields {
            if !val.is_empty() {
                fields.push((name.to_string(), FieldValue::Text(val.clone())));
            }
        }

        let enum_fields = [
            ("construction_class", &self.construction_class),
            ("occupancy", &self.occupancy),
            ("roof_type", &self.roof_type),
        ];
        for (name, val) in enum_fields {
            if !val.is_empty() {
                fields.push((name.to_string(), FieldValue::Enum(val.clone())));
            }
        }

        let number_fields = [
            ("year_built", &self.year_built),
            ("sq_footage", &self.sq_footage),
            ("stories", &self.stories),
        ];
        for (name, val) in number_fields {
            if !val.is_empty() {
                if let Ok(n) = Decimal::from_str(val) {
                    fields.push((name.to_string(), FieldValue::Number(n)));
                }
            }
        }

        let money_fields = [
            ("replacement_cost", &self.replacement_cost),
            ("contents_value", &self.contents_value),
        ];
        for (name, val) in money_fields {
            if !val.is_empty() {
                if let Ok(amount) = Decimal::from_str(val) {
                    fields.push((name.to_string(), FieldValue::Money {
                        amount,
                        currency: "USD".to_string(),
                    }));
                }
            }
        }

        if !self.sprinkler.is_empty() {
            let val = matches!(self.sprinkler.as_str(), "true" | "yes" | "1" | "Y");
            fields.push(("sprinkler".to_string(), FieldValue::Bool(val)));
        }

        fields
    }
}

// ── Handler ─────────────────────────────────────────────────────────────────

/// POST /api/onboard — onboard a new pool with members and their SOV CSVs.
#[tracing::instrument(name = "api.onboard_pool", skip_all)]
async fn onboard_pool(
    State(state): State<AppState>,
    Json(req): Json<OnboardPoolRequest>,
) -> Result<(StatusCode, Json<OnboardResult>), (StatusCode, Json<OnboardResult>)> {
    let conn = state.db.get().map_err(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(OnboardResult {
            pool_id: String::new(), pool_name: req.pool_name.clone(),
            members: vec![], total_assets: 0, errors: vec!["DB connection failed".into()],
        }))
    })?;

    let actor_id = ActorId::new();
    let pool_id = PoolId::new();
    let today = time::OffsetDateTime::now_utc().date().to_string();

    // Create pool
    conn.execute(
        "INSERT INTO pools (pool_id, name, created_by) VALUES (?1, ?2, ?3)",
        rusqlite::params![pool_id.to_string(), req.pool_name, actor_id.to_string()],
    ).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(OnboardResult {
            pool_id: pool_id.to_string(), pool_name: req.pool_name.clone(),
            members: vec![], total_assets: 0, errors: vec![format!("Pool creation failed: {e}")],
        }))
    })?;

    let mut member_results = Vec::new();
    let mut total_assets = 0;

    for member_req in &req.members {
        let member_id = MemberId::new();

        // Create member
        if let Err(e) = conn.execute(
            "INSERT INTO members (member_id, pool_id, name, created_by) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![member_id.to_string(), pool_id.to_string(), member_req.member_name, actor_id.to_string()],
        ) {
            member_results.push(MemberImportResult {
                member_id: member_id.to_string(),
                member_name: member_req.member_name.clone(),
                assets_imported: 0,
                errors: vec![format!("Member creation failed: {e}")],
            });
            continue;
        }

        // Parse CSV
        let mut rdr = csv::ReaderBuilder::new()
            .flexible(true)
            .trim(csv::Trim::All)
            .from_reader(member_req.sov_csv.as_bytes());

        let mut assets_imported = 0;
        let mut errors = Vec::new();

        // Track asset_key -> asset_id so multiple rows with the same key
        // add historical mutations to the same asset instead of creating duplicates.
        let mut key_to_asset: std::collections::HashMap<String, String> = std::collections::HashMap::new();

        for (row_idx, result) in rdr.deserialize().enumerate() {
            let row: SovRow = match result {
                Ok(r) => r,
                Err(e) => {
                    errors.push(format!("Row {}: parse error: {e}", row_idx + 2));
                    continue;
                }
            };

            let asset_type = row.asset_type.trim();
            if asset_type.is_empty() {
                errors.push(format!("Row {}: asset_type is empty", row_idx + 2));
                continue;
            }

            let effective = if row.effective_date.is_empty() { today.clone() } else { row.effective_date.clone() };

            // Determine if this row creates a new asset or adds mutations to an existing one
            let asset_id_str = if !row.asset_key.is_empty() {
                if let Some(existing) = key_to_asset.get(&row.asset_key) {
                    // Existing asset — just add mutations below
                    existing.clone()
                } else {
                    // First row for this key — create the asset
                    let asset_id = AssetId::new();
                    let id_str = asset_id.to_string();
                    let path = format!("/{}/{}/{}", pool_id, member_id, asset_id);

                    if let Err(e) = conn.execute(
                        "INSERT INTO assets (asset_id, pool_id, member_id, path, asset_type, lifecycle, created_by)
                         VALUES (?1, ?2, ?3, ?4, ?5, 'Active', ?6)",
                        rusqlite::params![&id_str, pool_id.to_string(), member_id.to_string(), path, asset_type, actor_id.to_string()],
                    ) {
                        errors.push(format!("Row {}: asset insert failed: {e}", row_idx + 2));
                        continue;
                    }
                    key_to_asset.insert(row.asset_key.clone(), id_str.clone());
                    assets_imported += 1;
                    total_assets += 1;
                    id_str
                }
            } else {
                // No asset_key — every row creates a new asset (backward compatible)
                let asset_id = AssetId::new();
                let id_str = asset_id.to_string();
                let path = format!("/{}/{}/{}", pool_id, member_id, asset_id);

                if let Err(e) = conn.execute(
                    "INSERT INTO assets (asset_id, pool_id, member_id, path, asset_type, lifecycle, created_by)
                     VALUES (?1, ?2, ?3, ?4, ?5, 'Active', ?6)",
                    rusqlite::params![&id_str, pool_id.to_string(), member_id.to_string(), path, asset_type, actor_id.to_string()],
                ) {
                    errors.push(format!("Row {}: asset insert failed: {e}", row_idx + 2));
                    continue;
                }
                assets_imported += 1;
                total_assets += 1;
                id_str
            };

            // Insert field mutations (all approved — this is onboarding data)
            let field_mutations = row.to_field_mutations();
            for (field_name, field_value) in &field_mutations {
                let mutation_id = MutationId::new();
                let value_json = serde_json::to_string(field_value).unwrap();

                if let Err(e) = conn.execute(
                    "INSERT INTO field_mutations (mutation_id, asset_id, field_name, value_json, effective_date, submitted_by, approval_state)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'Approved')",
                    rusqlite::params![
                        mutation_id.to_string(), &asset_id_str,
                        field_name, value_json, &effective, actor_id.to_string(),
                    ],
                ) {
                    errors.push(format!("Row {}: mutation insert for '{}' failed: {e}", row_idx + 2, field_name));
                }
            }
        }

        // Create a member user account
        let member_user_id = ActorId::new();
        let email_slug = member_req.member_name.to_lowercase().replace(' ', "-").replace("of-", "");
        let _ = conn.execute(
            "INSERT INTO users (user_id, email, display_name, category, pool_id, member_id) VALUES (?1, ?2, ?3, 'MemberUser', ?4, ?5)",
            rusqlite::params![
                member_user_id.to_string(),
                format!("facilities@{email_slug}.gov"),
                format!("{} User", member_req.member_name),
                pool_id.to_string(),
                member_id.to_string(),
            ],
        );

        member_results.push(MemberImportResult {
            member_id: member_id.to_string(),
            member_name: member_req.member_name.clone(),
            assets_imported,
            errors,
        });
    }

    // Create a read-only user for the first member (for Cedar demo)
    if let Some(first_member) = member_results.first() {
        let ro_user_id = ActorId::new();
        let _ = conn.execute(
            "INSERT INTO users (user_id, email, display_name, category, pool_id, member_id) VALUES (?1, ?2, ?3, 'MemberReadOnly', ?4, ?5)",
            rusqlite::params![
                ro_user_id.to_string(),
                format!("readonly@{}.gov", req.pool_name.to_lowercase().replace(' ', "-")),
                format!("{} (Read-Only)", first_member.member_name),
                pool_id.to_string(),
                first_member.member_id,
            ],
        );
    }

    // Create a pool admin user
    let pool_admin_id = ActorId::new();
    let pool_slug = req.pool_name.to_lowercase().replace(' ', "-");
    let _ = conn.execute(
        "INSERT INTO users (user_id, email, display_name, category, pool_id, member_id) VALUES (?1, ?2, ?3, 'PoolAdministrator', ?4, NULL)",
        rusqlite::params![
            pool_admin_id.to_string(),
            format!("admin@{pool_slug}.dev"),
            format!("{} Admin", req.pool_name),
            pool_id.to_string(),
        ],
    );

    let result = OnboardResult {
        pool_id: pool_id.to_string(),
        pool_name: req.pool_name.clone(),
        members: member_results,
        total_assets,
        errors: vec![],
    };

    // Rebuild search index after onboarding new data
    if let Ok(c) = state.db.get() {
        let _ = centurisk_search::SearchIndex::ensure_table(&c);
        let _ = centurisk_search::SearchIndex::rebuild(&c);
    }

    Ok((StatusCode::CREATED, Json(result)))
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/onboard", post(onboard_pool))
}
