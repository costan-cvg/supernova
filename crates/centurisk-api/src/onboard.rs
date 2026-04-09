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

        for (row_idx, result) in rdr.deserialize().enumerate() {
            let row: SovRow = match result {
                Ok(r) => r,
                Err(e) => {
                    errors.push(format!("Row {}: parse error: {e}", row_idx + 2));
                    continue;
                }
            };

            let asset_type = match row.asset_type.as_str() {
                "Building" | "Contents" | "Vehicle" | "FineArts" => row.asset_type.as_str(),
                _ => {
                    errors.push(format!("Row {}: unknown asset_type '{}'", row_idx + 2, row.asset_type));
                    continue;
                }
            };

            let asset_id = AssetId::new();
            let path = format!("/{}/{}/{}", pool_id, member_id, asset_id);

            // Insert asset
            if let Err(e) = conn.execute(
                "INSERT INTO assets (asset_id, pool_id, member_id, path, asset_type, lifecycle, created_by)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'Active', ?6)",
                rusqlite::params![
                    asset_id.to_string(), pool_id.to_string(), member_id.to_string(),
                    path, asset_type, actor_id.to_string(),
                ],
            ) {
                errors.push(format!("Row {}: asset insert failed: {e}", row_idx + 2));
                continue;
            }

            // Insert field mutations (all approved — this is onboarding data)
            let field_mutations = row.to_field_mutations();
            for (field_name, field_value) in &field_mutations {
                let mutation_id = MutationId::new();
                let value_json = serde_json::to_string(field_value).unwrap();

                if let Err(e) = conn.execute(
                    "INSERT INTO field_mutations (mutation_id, asset_id, field_name, value_json, effective_date, submitted_by, approval_state)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'Approved')",
                    rusqlite::params![
                        mutation_id.to_string(), asset_id.to_string(),
                        field_name, value_json, today, actor_id.to_string(),
                    ],
                ) {
                    errors.push(format!("Row {}: mutation insert for '{}' failed: {e}", row_idx + 2, field_name));
                }
            }

            assets_imported += 1;
            total_assets += 1;
        }

        member_results.push(MemberImportResult {
            member_id: member_id.to_string(),
            member_name: member_req.member_name.clone(),
            assets_imported,
            errors,
        });
    }

    let result = OnboardResult {
        pool_id: pool_id.to_string(),
        pool_name: req.pool_name.clone(),
        members: member_results,
        total_assets,
        errors: vec![],
    };

    Ok((StatusCode::CREATED, Json(result)))
}

/// Onboard from sample files on disk. Called at server startup if DB is empty.
pub fn onboard_from_samples(db: &crate::centurisk_db::DbPool, samples_dir: &std::path::Path) {
    let conn = db.get().expect("DB connection for onboarding");

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM pools", [], |r| r.get(0))
        .unwrap_or(0);

    if count > 0 {
        tracing::info!("Database already has data, skipping sample import");
        return;
    }

    if !samples_dir.exists() {
        tracing::warn!("Samples directory not found at {}, skipping", samples_dir.display());
        return;
    }

    tracing::info!("Importing sample data from {}", samples_dir.display());

    // Read each subdirectory as a pool
    let mut entries: Vec<_> = std::fs::read_dir(samples_dir)
        .expect("Failed to read samples directory")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    entries.sort_by_key(|e| e.file_name());

    // We also need to create users — collect pool/member IDs for user creation
    let mut user_seed_sql = String::new();
    let system_actor = ActorId::new();

    for entry in &entries {
        let pool_dir = entry.path();
        let pool_csv_path = pool_dir.join("pool.csv");

        if !pool_csv_path.exists() {
            continue;
        }

        // Read pool.csv to get pool name and members
        let pool_csv = std::fs::read_to_string(&pool_csv_path).expect("Failed to read pool.csv");
        let mut rdr = csv::ReaderBuilder::new()
            .trim(csv::Trim::All)
            .from_reader(pool_csv.as_bytes());

        let pool_id = PoolId::new();
        let mut pool_name = String::new();
        let mut members: Vec<(MemberId, String)> = Vec::new();

        #[derive(Deserialize)]
        struct PoolRow {
            pool_name: String,
            member_name: String,
            #[allow(dead_code)]
            member_contact_email: String,
        }

        for result in rdr.deserialize() {
            let row: PoolRow = result.expect("Failed to parse pool.csv row");
            if pool_name.is_empty() {
                pool_name = row.pool_name.clone();
            }
            members.push((MemberId::new(), row.member_name));
        }

        // Create pool
        conn.execute(
            "INSERT INTO pools (pool_id, name, created_by) VALUES (?1, ?2, ?3)",
            rusqlite::params![pool_id.to_string(), pool_name, system_actor.to_string()],
        ).expect("Failed to create pool");

        // Create members
        for (member_id, member_name) in &members {
            conn.execute(
                "INSERT INTO members (member_id, pool_id, name, created_by) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![member_id.to_string(), pool_id.to_string(), member_name, system_actor.to_string()],
            ).expect("Failed to create member");
        }

        // Import SOV CSVs — match by member name in filename
        let mut sov_files: Vec<_> = std::fs::read_dir(&pool_dir)
            .expect("Failed to read pool directory")
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.ends_with("-sov.csv")
            })
            .collect();
        sov_files.sort_by_key(|e| e.file_name());

        let today = time::OffsetDateTime::now_utc().date().to_string();

        for (member_idx, sov_entry) in sov_files.iter().enumerate() {
            // Use the member at the same index (order matches)
            let (member_id, _member_name) = if member_idx < members.len() {
                &members[member_idx]
            } else {
                &members[0]
            };

            let sov_csv = std::fs::read_to_string(sov_entry.path())
                .expect("Failed to read SOV CSV");
            let mut rdr = csv::ReaderBuilder::new()
                .flexible(true)
                .trim(csv::Trim::All)
                .from_reader(sov_csv.as_bytes());

            let mut asset_count = 0;
            for result in rdr.deserialize() {
                let row: SovRow = match result {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!("Skipping row in {:?}: {e}", sov_entry.file_name());
                        continue;
                    }
                };

                let asset_id = AssetId::new();
                let path = format!("/{}/{}/{}", pool_id, member_id, asset_id);

                conn.execute(
                    "INSERT INTO assets (asset_id, pool_id, member_id, path, asset_type, lifecycle, created_by)
                     VALUES (?1, ?2, ?3, ?4, ?5, 'Active', ?6)",
                    rusqlite::params![
                        asset_id.to_string(), pool_id.to_string(), member_id.to_string(),
                        path, row.asset_type, system_actor.to_string(),
                    ],
                ).expect("Failed to insert asset");

                for (field_name, field_value) in row.to_field_mutations() {
                    let mutation_id = MutationId::new();
                    let value_json = serde_json::to_string(&field_value).unwrap();

                    conn.execute(
                        "INSERT INTO field_mutations (mutation_id, asset_id, field_name, value_json, effective_date, submitted_by, approval_state)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'Approved')",
                        rusqlite::params![
                            mutation_id.to_string(), asset_id.to_string(),
                            field_name, value_json, today, system_actor.to_string(),
                        ],
                    ).expect("Failed to insert mutation");
                }

                asset_count += 1;
            }

            tracing::info!(
                "Imported {} assets from {:?}",
                asset_count,
                sov_entry.file_name()
            );
        }

        // Generate user seed SQL for this pool
        let admin_id = ActorId::new();
        let pool_admin_id = ActorId::new();
        let slug = pool_name.to_lowercase().replace(' ', "-");

        user_seed_sql.push_str(&format!(
            "INSERT INTO users (user_id, email, display_name, category, pool_id, member_id) VALUES ('{admin_id}', 'admin@{slug}.dev', '{pool_name} Admin', 'PoolAdministrator', '{pool_id}', NULL);\n"
        ));

        for (member_id, member_name) in &members {
            let user_id = ActorId::new();
            let email_slug = member_name.to_lowercase().replace(' ', "-").replace("of-", "");
            user_seed_sql.push_str(&format!(
                "INSERT INTO users (user_id, email, display_name, category, pool_id, member_id) VALUES ('{user_id}', 'facilities@{email_slug}.gov', '{member_name} User', 'MemberUser', '{pool_id}', '{member_id}');\n"
            ));
        }

        let _ = pool_admin_id; // used above
        tracing::info!("Onboarded pool '{}' with {} members", pool_name, members.len());
    }

    // Create a CentuRisk system admin
    let centurisk_admin = ActorId::new();
    conn.execute(
        "INSERT INTO users (user_id, email, display_name, category, pool_id, member_id) VALUES (?1, 'admin@centurisk.dev', 'Alice Admin (CentuRisk)', 'CentuRiskAdmin', NULL, NULL)",
        rusqlite::params![centurisk_admin.to_string()],
    ).expect("Failed to create CentuRisk admin");

    // Create pool-specific users
    if !user_seed_sql.is_empty() {
        conn.execute_batch(&user_seed_sql).expect("Failed to create users");
    }

    tracing::info!("Sample data import complete");
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/onboard", post(onboard_pool))
}
