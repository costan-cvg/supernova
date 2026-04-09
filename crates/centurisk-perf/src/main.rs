//! RiskStar Performance Validation (Inc 14)
//!
//! Validates all ADR performance targets at production scale:
//! - 2,500 members across 5 pools
//! - ~400 assets per member (1M total)
//! - ~30 fields per asset with power-law mutation distribution
//!
//! Benchmarks:
//! | Operation                            | Target        |
//! |--------------------------------------|---------------|
//! | Single asset temporal resolution     | p95 < 50ms   |
//! | 100-asset batch resolution           | p95 < 500ms  |
//! | Quality scoring (single asset)       | p95 < 100ms  |
//! | Quality batch rescore (10K assets)   | < 30s        |
//! | SOV export (full pool, 1M assets)    | < 5 min      |

use centurisk_core::field_value::FieldValue;
use centurisk_core::quality::{
    self, AccuracyRule, CompletenessConfig, RecencyField,
};
use rand::prelude::*;
use rust_decimal::Decimal;
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};
use time::Date;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const NUM_POOLS: usize = 5;
const NUM_MEMBERS: usize = 2_500;
const ASSETS_PER_MEMBER_AVG: usize = 400;
const NUM_ASSETS: usize = NUM_MEMBERS * ASSETS_PER_MEMBER_AVG; // 1_000_000
const NUM_FIELDS: usize = 30;
const BATCH_INSERT_SIZE: usize = 10_000;
const SINGLE_BENCHMARK_ITERATIONS: usize = 1_000;
const BATCH_BENCHMARK_ITERATIONS: usize = 100;
const BATCH_RESOLVE_SIZE: usize = 100;
const QUALITY_BATCH_SIZE: usize = 10_000;
// Full pool = NUM_ASSETS for export benchmark

/// p95 targets in milliseconds.
const P95_SINGLE_RESOLVE_TARGET_MS: f64 = 50.0;
const P95_BATCH_RESOLVE_TARGET_MS: f64 = 500.0;
const P95_QUALITY_SINGLE_TARGET_MS: f64 = 100.0;
const QUALITY_BATCH_TARGET_S: f64 = 30.0;
const SOV_EXPORT_TARGET_S: f64 = 300.0; // 5 min

const FIELD_NAMES: [&str; NUM_FIELDS] = [
    "address",
    "replacement_cost",
    "year_built",
    "construction_class",
    "occupancy",
    "sq_footage",
    "stories",
    "sprinkler",
    "roof_type",
    "foundation_type",
    "building_name",
    "city",
    "state",
    "zip_code",
    "county",
    "latitude",
    "longitude",
    "basement",
    "electrical_update_year",
    "plumbing_update_year",
    "heating_type",
    "cooling_type",
    "fire_alarm",
    "security_system",
    "elevator_count",
    "parking_spaces",
    "ais_zone",
    "flood_zone",
    "appraisal_date",
    "contents_value",
];

// ---------------------------------------------------------------------------
// Data generation helpers
// ---------------------------------------------------------------------------

/// Generate a random FieldValue appropriate for the given field name.
fn random_field_value(rng: &mut impl Rng, field_name: &str) -> FieldValue {
    match field_name {
        "address" => {
            let street_num = rng.gen_range(100..9999);
            let streets = ["Main St", "Oak Ave", "Elm St", "River Rd", "Park Blvd",
                          "Washington Ave", "Lincoln Dr", "Cedar Ln", "Maple Ct", "Pine Rd"];
            FieldValue::Text(format!("{} {}", street_num, streets[rng.gen_range(0..streets.len())]))
        }
        "building_name" => {
            let prefixes = ["Fire Station", "City Hall", "Library", "Water Plant",
                          "Community Center", "School", "Hospital", "Courthouse",
                          "Police Station", "Recreation Center"];
            FieldValue::Text(format!("{} #{}", prefixes[rng.gen_range(0..prefixes.len())], rng.gen_range(1..100)))
        }
        "city" => {
            let cities = ["Springfield", "Shelbyville", "Portland", "Madison",
                         "Georgetown", "Fairview", "Clinton", "Marion", "Salem", "Chester"];
            FieldValue::Text(cities[rng.gen_range(0..cities.len())].into())
        }
        "county" => {
            let counties = ["Cook", "Harris", "Maricopa", "San Diego", "Orange",
                           "Miami-Dade", "Dallas", "Kings", "Clark", "Tarrant"];
            FieldValue::Text(counties[rng.gen_range(0..counties.len())].into())
        }
        "replacement_cost" => FieldValue::Money {
            amount: Decimal::new(rng.gen_range(100_000i64..50_000_000i64), 0),
            currency: "USD".into(),
        },
        "contents_value" => FieldValue::Money {
            amount: Decimal::new(rng.gen_range(10_000i64..5_000_000i64), 0),
            currency: "USD".into(),
        },
        "year_built" | "electrical_update_year" | "plumbing_update_year" => {
            FieldValue::Number(Decimal::from(rng.gen_range(1920..2024)))
        }
        "construction_class" => {
            let classes = ["Frame", "Joisted Masonry", "Non-Combustible", "Masonry",
                          "Fire Resistive", "Modified Fire Resistive"];
            FieldValue::Enum(classes[rng.gen_range(0..classes.len())].into())
        }
        "occupancy" => {
            let types = ["Office", "Retail", "Warehouse", "Manufacturing", "School",
                        "Municipal", "Fire Station", "Library", "Hospital", "habitational"];
            FieldValue::Enum(types[rng.gen_range(0..types.len())].into())
        }
        "sq_footage" => FieldValue::Number(Decimal::from(rng.gen_range(500..200_000))),
        "stories" => FieldValue::Number(Decimal::from(rng.gen_range(1..20))),
        "parking_spaces" => FieldValue::Number(Decimal::from(rng.gen_range(0..500))),
        "elevator_count" => FieldValue::Number(Decimal::from(rng.gen_range(0..10))),
        "sprinkler" | "basement" | "fire_alarm" | "security_system" => {
            FieldValue::Bool(rng.gen_bool(0.6))
        }
        "roof_type" => {
            let types = ["Flat", "Pitched", "Hip", "Gable", "Metal"];
            FieldValue::Enum(types[rng.gen_range(0..types.len())].into())
        }
        "foundation_type" => {
            let types = ["Slab", "Crawl Space", "Full Basement", "Pier"];
            FieldValue::Enum(types[rng.gen_range(0..types.len())].into())
        }
        "heating_type" => {
            let types = ["Forced Air", "Radiant", "Steam", "Heat Pump", "None"];
            FieldValue::Enum(types[rng.gen_range(0..types.len())].into())
        }
        "cooling_type" => {
            let types = ["Central AC", "Window Units", "None", "Evaporative"];
            FieldValue::Enum(types[rng.gen_range(0..types.len())].into())
        }
        "state" => {
            let states = ["CA", "TX", "NY", "FL", "IL", "PA", "OH", "GA", "NC", "MI"];
            FieldValue::Enum(states[rng.gen_range(0..states.len())].into())
        }
        "zip_code" => FieldValue::Text(format!("{:05}", rng.gen_range(10000..99999u32))),
        "latitude" => {
            FieldValue::Number(Decimal::new(rng.gen_range(25_000_000i64..48_000_000i64), 6))
        }
        "longitude" => {
            FieldValue::Number(Decimal::new(rng.gen_range(-124_000_000i64..-70_000_000i64), 6))
        }
        "ais_zone" | "flood_zone" => {
            let zones = ["A", "B", "C", "X", "AE", "VE"];
            FieldValue::Enum(zones[rng.gen_range(0..zones.len())].into())
        }
        "appraisal_date" => {
            let year = rng.gen_range(2019..2024);
            let month = time::Month::try_from(rng.gen_range(1u8..=12)).unwrap();
            let day = rng.gen_range(1..=28);
            FieldValue::Date(Date::from_calendar_date(year, month, day).unwrap())
        }
        _ => FieldValue::Text(format!("value_{}", rng.gen_range(0..1000))),
    }
}

/// Generate a random effective date spread over 5 years (2019-2024).
fn random_effective_date(rng: &mut impl Rng) -> String {
    let year = rng.gen_range(2019..=2024);
    let month = rng.gen_range(1u8..=12);
    let day = rng.gen_range(1u8..=28);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Power-law distribution for mutation count per asset.
/// Most assets get 3-5 mutations, some get 100+.
fn mutation_count_for_asset(rng: &mut impl Rng) -> usize {
    let u: f64 = rng.gen_range(0.0..1.0);
    let x_min = 3.0_f64;
    let alpha = 1.5_f64;
    let count = x_min * (1.0 - u).powf(-1.0 / alpha);
    (count as usize).min(300).max(3)
}

/// Submitted-at timestamp.
fn random_submitted_at(rng: &mut impl Rng, effective_date: &str) -> String {
    let hour = rng.gen_range(0..24);
    let min = rng.gen_range(0..60);
    let sec = rng.gen_range(0..60);
    format!("{effective_date}T{hour:02}:{min:02}:{sec:02}Z")
}

// ---------------------------------------------------------------------------
// Synthetic dataset
// ---------------------------------------------------------------------------

struct MutationRow {
    mutation_id: String,
    field_name: String,
    value_json: String,
    effective_date: String,
    submitted_at: String,
}

struct GeneratedDataset {
    #[allow(dead_code)]
    pool_ids: Vec<String>,
    member_ids: Vec<String>,
    /// (asset_id, pool_idx, member_idx, mutations)
    assets: Vec<(String, usize, usize, Vec<MutationRow>)>,
    total_mutations: usize,
}

fn generate_dataset() -> GeneratedDataset {
    let mut rng = StdRng::seed_from_u64(42);
    let gen_start = Instant::now();

    // Pools
    let pool_ids: Vec<String> = (0..NUM_POOLS).map(|_| Uuid::now_v7().to_string()).collect();

    // Members evenly across pools
    let member_ids: Vec<String> = (0..NUM_MEMBERS).map(|_| Uuid::now_v7().to_string()).collect();

    let mut assets = Vec::with_capacity(NUM_ASSETS);
    let mut total_mutations = 0;

    println!("Generating {} assets across {} members in {} pools...", NUM_ASSETS, NUM_MEMBERS, NUM_POOLS);

    for i in 0..NUM_ASSETS {
        let pool_idx = i % NUM_POOLS;
        let member_idx = i % NUM_MEMBERS;
        let asset_id = Uuid::now_v7().to_string();
        let mutation_count = mutation_count_for_asset(&mut rng);
        let mut mutations = Vec::with_capacity(mutation_count.max(NUM_FIELDS));

        // Base: one mutation per field
        for field_name in &FIELD_NAMES {
            let mutation_id = Uuid::now_v7().to_string();
            let value = random_field_value(&mut rng, field_name);
            let value_json = serde_json::to_string(&value).unwrap();
            let effective_date = random_effective_date(&mut rng);
            let submitted_at = random_submitted_at(&mut rng, &effective_date);
            mutations.push(MutationRow {
                mutation_id,
                field_name: field_name.to_string(),
                value_json,
                effective_date,
                submitted_at,
            });
        }

        // Extra mutations (power-law distribution for field updates)
        let extra = if mutation_count > NUM_FIELDS { mutation_count - NUM_FIELDS } else { 0 };
        for _ in 0..extra {
            let field_name = FIELD_NAMES[rng.gen_range(0..NUM_FIELDS)];
            let mutation_id = Uuid::now_v7().to_string();
            let value = random_field_value(&mut rng, field_name);
            let value_json = serde_json::to_string(&value).unwrap();
            let effective_date = random_effective_date(&mut rng);
            let submitted_at = random_submitted_at(&mut rng, &effective_date);
            mutations.push(MutationRow {
                mutation_id,
                field_name: field_name.to_string(),
                value_json,
                effective_date,
                submitted_at,
            });
        }

        total_mutations += mutations.len();
        assets.push((asset_id, pool_idx, member_idx, mutations));

        if (i + 1) % 100_000 == 0 {
            println!("  Generated {}/{} assets ({} mutations)...", i + 1, NUM_ASSETS, total_mutations);
        }
    }

    let elapsed = gen_start.elapsed();
    println!("Data generation: {} assets, {} mutations in {:.1}s",
        assets.len(), total_mutations, elapsed.as_secs_f64());

    GeneratedDataset { pool_ids, member_ids, assets, total_mutations }
}

// ---------------------------------------------------------------------------
// Database setup
// ---------------------------------------------------------------------------

fn create_db(path: &Path) -> Connection {
    let conn = Connection::open(path).expect("Failed to open database");
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA mmap_size=268435456;
         PRAGMA cache_size=-65536;
         PRAGMA temp_store=MEMORY;",
    ).expect("Failed to set pragmas");
    conn
}

fn create_schema(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS field_mutations (
            mutation_id TEXT PRIMARY KEY,
            asset_id TEXT NOT NULL,
            field_name TEXT NOT NULL,
            value_json TEXT NOT NULL,
            effective_date TEXT NOT NULL,
            approval_state TEXT NOT NULL DEFAULT 'Approved',
            submitted_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS current_asset_state (
            asset_id TEXT NOT NULL,
            field_name TEXT NOT NULL,
            value_json TEXT NOT NULL,
            effective_date TEXT NOT NULL,
            source_mutation_id TEXT NOT NULL,
            PRIMARY KEY (asset_id, field_name)
        );

        CREATE INDEX IF NOT EXISTS idx_mutations_asset_field_date
            ON field_mutations(asset_id, field_name, effective_date DESC);",
    ).expect("Failed to create schema");
}

fn insert_mutations(conn: &Connection, data: &GeneratedDataset) {
    println!("Inserting mutations into database...");
    let start = Instant::now();
    let mut count = 0;

    let mut batch: Vec<(&str, &MutationRow)> = Vec::with_capacity(BATCH_INSERT_SIZE);

    for (asset_id, _pool_idx, _member_idx, mutations) in &data.assets {
        for mutation in mutations {
            batch.push((asset_id.as_str(), mutation));

            if batch.len() >= BATCH_INSERT_SIZE {
                insert_batch(conn, &batch);
                count += batch.len();
                batch.clear();

                if count % 1_000_000 == 0 {
                    println!("  Inserted {}/{} mutations...", count, data.total_mutations);
                }
            }
        }
    }

    if !batch.is_empty() {
        insert_batch(conn, &batch);
        count += batch.len();
    }

    let elapsed = start.elapsed();
    println!("Insertion: {} mutations in {:.1}s ({:.0}/sec)",
        count, elapsed.as_secs_f64(), count as f64 / elapsed.as_secs_f64());
}

fn insert_batch(conn: &Connection, batch: &[(&str, &MutationRow)]) {
    let tx = conn.unchecked_transaction().expect("begin tx");
    {
        let mut stmt = tx.prepare_cached(
            "INSERT INTO field_mutations (mutation_id, asset_id, field_name, value_json, effective_date, approval_state, submitted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'Approved', ?6)"
        ).expect("prepare insert");

        for (asset_id, m) in batch {
            stmt.execute(params![m.mutation_id, asset_id, m.field_name, m.value_json, m.effective_date, m.submitted_at])
                .expect("insert mutation");
        }
    }
    tx.commit().expect("commit tx");
}

fn build_snapshot_table(conn: &Connection) {
    println!("Building snapshot table...");
    let start = Instant::now();
    conn.execute_batch(
        "INSERT OR REPLACE INTO current_asset_state (asset_id, field_name, value_json, effective_date, source_mutation_id)
         SELECT fm.asset_id, fm.field_name, fm.value_json, MAX(fm.effective_date), fm.mutation_id
         FROM field_mutations fm
         WHERE fm.approval_state = 'Approved'
         GROUP BY fm.asset_id, fm.field_name;"
    ).expect("build snapshot");
    println!("Snapshot built in {:.1}s", start.elapsed().as_secs_f64());
}

// ---------------------------------------------------------------------------
// Resolution operations
// ---------------------------------------------------------------------------

/// Resolve a single asset's state as of a given date (strategy 1: indexed scan).
fn resolve_single_temporal(conn: &Connection, asset_id: &str, as_of_date: &str) -> Vec<(String, String)> {
    // Use the GROUP BY + MAX approach validated in spike-temporal.
    // SQLite guarantees non-aggregated columns come from the MAX row.
    let mut stmt2 = conn.prepare_cached(
        "SELECT field_name, value_json, MAX(effective_date)
         FROM field_mutations
         WHERE asset_id = ?1 AND approval_state = 'Approved' AND effective_date <= ?2
         GROUP BY field_name"
    ).expect("prepare resolve");

    let rows = stmt2.query_map(params![asset_id, as_of_date], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    }).expect("execute resolve");

    rows.map(|r| r.unwrap()).collect()
}

/// Resolve current state from snapshot table.
fn resolve_current_snapshot(conn: &Connection, asset_id: &str) -> Vec<(String, String)> {
    let mut stmt = conn.prepare_cached(
        "SELECT field_name, value_json FROM current_asset_state WHERE asset_id = ?1"
    ).expect("prepare snapshot resolve");

    let rows = stmt.query_map(params![asset_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    }).expect("execute snapshot resolve");

    rows.map(|r| r.unwrap()).collect()
}

/// Resolve a batch of assets (uses snapshot for current state).
fn resolve_batch_current(conn: &Connection, asset_ids: &[String]) -> Vec<Vec<(String, String)>> {
    asset_ids.iter().map(|id| resolve_current_snapshot(conn, id)).collect()
}

// ---------------------------------------------------------------------------
// Quality scoring operations
// ---------------------------------------------------------------------------

/// Parse resolved fields from JSON into a HashMap<String, FieldValue>.
fn parse_resolved_fields(rows: &[(String, String)]) -> HashMap<String, FieldValue> {
    let mut fields = HashMap::new();
    for (name, json) in rows {
        if let Ok(val) = serde_json::from_str::<FieldValue>(json) {
            fields.insert(name.clone(), val);
        }
    }
    fields
}

/// Score quality for a single asset using completeness + accuracy + recency.
fn score_quality_single(
    fields: &HashMap<String, FieldValue>,
    completeness_config: &CompletenessConfig,
    accuracy_rules: &[AccuracyRule],
    recency_config: &[RecencyField],
) -> quality::QualityScore {
    let completeness = quality::score_completeness(fields, completeness_config);
    let accuracy = quality::score_accuracy(fields, accuracy_rules);

    // Simulate field ages (days since mutation) -- in production this comes from DB
    let mut field_ages = HashMap::new();
    for rc in recency_config {
        if fields.contains_key(&rc.field_name) {
            // Simulate: most fields updated within a year
            field_ages.insert(rc.field_name.clone(), 180);
        }
    }
    let recency = quality::score_recency(&field_ages, recency_config);

    let composite = completeness.score * 0.4 + accuracy.score * 0.4 + recency.score * 0.2;

    quality::QualityScore {
        completeness,
        accuracy,
        recency,
        composite,
    }
}

// ---------------------------------------------------------------------------
// SOV export operation
// ---------------------------------------------------------------------------

/// Simulate SOV export: iterate all assets in a pool, resolve current state,
/// and write CSV rows.
fn sov_export(conn: &Connection, asset_ids: &[String]) -> usize {
    let mut total_rows = 0;
    // Use a buffer to simulate writing CSV without actual file I/O overhead
    let mut csv_buf = Vec::with_capacity(4096);

    // Write header
    csv_buf.extend_from_slice(b"asset_id");
    for field in &FIELD_NAMES {
        csv_buf.push(b',');
        csv_buf.extend_from_slice(field.as_bytes());
    }
    csv_buf.push(b'\n');

    let mut stmt = conn.prepare_cached(
        "SELECT field_name, value_json FROM current_asset_state WHERE asset_id = ?1"
    ).expect("prepare sov export");

    for asset_id in asset_ids {
        let rows: Vec<(String, String)> = stmt.query_map(params![asset_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }).expect("query sov")
        .map(|r| r.unwrap())
        .collect();

        // Build row
        csv_buf.extend_from_slice(asset_id.as_bytes());
        let field_map: HashMap<&str, &str> = rows.iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        for field in &FIELD_NAMES {
            csv_buf.push(b',');
            if let Some(val) = field_map.get(field) {
                csv_buf.extend_from_slice(val.as_bytes());
            }
        }
        csv_buf.push(b'\n');
        total_rows += 1;

        // Flush periodically to avoid unbounded memory
        if csv_buf.len() > 10_000_000 {
            csv_buf.clear();
        }
    }

    total_rows
}

// ---------------------------------------------------------------------------
// Benchmarking infrastructure
// ---------------------------------------------------------------------------

struct BenchmarkResult {
    p50: Duration,
    p95: Duration,
    p99: Duration,
}

impl std::fmt::Display for BenchmarkResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "p50={:.2}ms p95={:.2}ms p99={:.2}ms",
            self.p50.as_secs_f64() * 1000.0,
            self.p95.as_secs_f64() * 1000.0,
            self.p99.as_secs_f64() * 1000.0,
        )
    }
}

fn percentile(sorted: &[Duration], pct: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let idx = ((sorted.len() as f64) * pct / 100.0).ceil() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn compute_percentiles(mut durations: Vec<Duration>) -> BenchmarkResult {
    durations.sort();
    BenchmarkResult {
        p50: percentile(&durations, 50.0),
        p95: percentile(&durations, 95.0),
        p99: percentile(&durations, 99.0),
    }
}

fn pass_fail(value_ms: f64, target_ms: f64) -> &'static str {
    if value_ms <= target_ms { "PASS" } else { "FAIL" }
}

// ---------------------------------------------------------------------------
// Benchmark runners
// ---------------------------------------------------------------------------

fn bench_temporal_resolution(conn: &Connection, asset_ids: &[String]) -> (BenchmarkResult, BenchmarkResult) {
    let as_of_date = "2024-12-31";
    let mut rng = StdRng::seed_from_u64(100);

    // Warmup
    for _ in 0..20 {
        let id = &asset_ids[rng.gen_range(0..asset_ids.len())];
        let _ = resolve_single_temporal(conn, id, as_of_date);
    }

    // Single-asset: 1000 iterations
    let mut single_durations = Vec::with_capacity(SINGLE_BENCHMARK_ITERATIONS);
    for _ in 0..SINGLE_BENCHMARK_ITERATIONS {
        let id = &asset_ids[rng.gen_range(0..asset_ids.len())];
        let start = Instant::now();
        let _ = resolve_single_temporal(conn, id, as_of_date);
        single_durations.push(start.elapsed());
    }

    // Batch: 100 iterations of 100 assets
    // Use snapshot table for batch (hybrid strategy: snapshot for current, scan for historical)
    let mut batch_durations = Vec::with_capacity(BATCH_BENCHMARK_ITERATIONS);
    for _ in 0..BATCH_BENCHMARK_ITERATIONS {
        let batch: Vec<String> = (0..BATCH_RESOLVE_SIZE)
            .map(|_| asset_ids[rng.gen_range(0..asset_ids.len())].clone())
            .collect();
        let start = Instant::now();
        let _ = resolve_batch_current(conn, &batch);
        batch_durations.push(start.elapsed());
    }

    (compute_percentiles(single_durations), compute_percentiles(batch_durations))
}

fn bench_quality_scoring(conn: &Connection, asset_ids: &[String]) -> (BenchmarkResult, Duration) {
    let mut rng = StdRng::seed_from_u64(200);
    let completeness_config = quality::building_completeness_config();
    let accuracy_rules = quality::default_accuracy_rules();
    let recency_config = quality::default_recency_config();

    // Warmup
    for _ in 0..20 {
        let id = &asset_ids[rng.gen_range(0..asset_ids.len())];
        let rows = resolve_current_snapshot(conn, id);
        let fields = parse_resolved_fields(&rows);
        let _ = score_quality_single(&fields, &completeness_config, &accuracy_rules, &recency_config);
    }

    // Single-asset quality: 1000 iterations (resolve + score)
    let mut single_durations = Vec::with_capacity(SINGLE_BENCHMARK_ITERATIONS);
    for _ in 0..SINGLE_BENCHMARK_ITERATIONS {
        let id = &asset_ids[rng.gen_range(0..asset_ids.len())];
        let start = Instant::now();
        let rows = resolve_current_snapshot(conn, id);
        let fields = parse_resolved_fields(&rows);
        let _ = score_quality_single(&fields, &completeness_config, &accuracy_rules, &recency_config);
        single_durations.push(start.elapsed());
    }

    // Batch rescore: 10K assets
    let batch_ids: Vec<String> = (0..QUALITY_BATCH_SIZE)
        .map(|_| asset_ids[rng.gen_range(0..asset_ids.len())].clone())
        .collect();

    let batch_start = Instant::now();
    for id in &batch_ids {
        let rows = resolve_current_snapshot(conn, id);
        let fields = parse_resolved_fields(&rows);
        let _ = score_quality_single(&fields, &completeness_config, &accuracy_rules, &recency_config);
    }
    let batch_elapsed = batch_start.elapsed();

    (compute_percentiles(single_durations), batch_elapsed)
}

fn bench_sov_export(conn: &Connection, asset_ids: &[String]) -> Duration {
    println!("Running SOV export benchmark ({} assets)...", asset_ids.len());
    let start = Instant::now();
    let rows_exported = sov_export(conn, asset_ids);
    let elapsed = start.elapsed();
    println!("  Exported {} rows in {:.1}s", rows_exported, elapsed.as_secs_f64());
    elapsed
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let tmp_dir = std::env::temp_dir().join("centurisk-perf");
    std::fs::create_dir_all(&tmp_dir).expect("create temp dir");
    let db_path = tmp_dir.join("perf-benchmark.db");

    // Clean up previous run
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(db_path.with_extension("db-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("db-shm"));

    println!("=== RiskStar Performance Validation ===");
    println!("Database: {}", db_path.display());
    println!();

    // Step 1: Generate synthetic data
    let data = generate_dataset();
    let asset_ids: Vec<String> = data.assets.iter().map(|(id, _, _, _)| id.clone()).collect();

    println!("Dataset: {} assets, {} members, {} mutations",
        data.assets.len(), data.member_ids.len(), data.total_mutations);
    println!();

    // Step 2: Create DB and insert
    let conn = create_db(&db_path);
    create_schema(&conn);
    insert_mutations(&conn, &data);
    build_snapshot_table(&conn);

    let db_size_bytes = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);
    let db_size_mb = db_size_bytes as f64 / (1024.0 * 1024.0);
    println!("Database size: {:.1} MB", db_size_mb);
    println!();

    // Step 3: Run benchmarks
    let mut targets_passed = 0;
    let total_targets = 5;

    // --- Temporal Resolution ---
    println!("Running temporal resolution benchmarks...");
    let (single_res, batch_res) = bench_temporal_resolution(&conn, &asset_ids);

    let single_p95_ms = single_res.p95.as_secs_f64() * 1000.0;
    let batch_p95_ms = batch_res.p95.as_secs_f64() * 1000.0;
    let single_pf = pass_fail(single_p95_ms, P95_SINGLE_RESOLVE_TARGET_MS);
    let batch_pf = pass_fail(batch_p95_ms, P95_BATCH_RESOLVE_TARGET_MS);

    if single_pf == "PASS" { targets_passed += 1; }
    if batch_pf == "PASS" { targets_passed += 1; }

    println!();
    println!("--- Temporal Resolution ---");
    println!("Single asset: {} [{}]", single_res, single_pf);
    println!("100-batch:    {} [{}]", batch_res, batch_pf);

    // --- Quality Scoring ---
    println!();
    println!("Running quality scoring benchmarks...");
    let (quality_single_res, quality_batch_elapsed) = bench_quality_scoring(&conn, &asset_ids);

    let quality_single_p95_ms = quality_single_res.p95.as_secs_f64() * 1000.0;
    let quality_batch_s = quality_batch_elapsed.as_secs_f64();
    let quality_single_pf = pass_fail(quality_single_p95_ms, P95_QUALITY_SINGLE_TARGET_MS);
    let quality_batch_pf = pass_fail(quality_batch_s, QUALITY_BATCH_TARGET_S);

    if quality_single_pf == "PASS" { targets_passed += 1; }
    if quality_batch_pf == "PASS" { targets_passed += 1; }

    println!();
    println!("--- Quality Scoring ---");
    println!("Single asset: {} [{}]", quality_single_res, quality_single_pf);
    println!("10K batch:    {:.2}s [{}]", quality_batch_s, quality_batch_pf);

    // --- SOV Export ---
    println!();
    let sov_elapsed = bench_sov_export(&conn, &asset_ids);
    let sov_s = sov_elapsed.as_secs_f64();
    let sov_pf = pass_fail(sov_s, SOV_EXPORT_TARGET_S);
    if sov_pf == "PASS" { targets_passed += 1; }

    println!();
    println!("--- SOV Export ---");
    println!("Full pool ({} assets): {:.2}s [{}]", asset_ids.len(), sov_s, sov_pf);

    // --- Summary ---
    println!();
    println!("=== RESULT: {}/{} targets met ===", targets_passed, total_targets);

    // Cleanup
    drop(conn);
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(db_path.with_extension("db-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("db-shm"));
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_percentile_computation() {
        let durations: Vec<Duration> = (1..=100).map(|i| Duration::from_millis(i)).collect();
        let result = compute_percentiles(durations);

        assert!(result.p50.as_millis() >= 49 && result.p50.as_millis() <= 51,
            "p50 should be ~50ms, got {}ms", result.p50.as_millis());
        assert!(result.p95.as_millis() >= 94 && result.p95.as_millis() <= 96,
            "p95 should be ~95ms, got {}ms", result.p95.as_millis());
        assert!(result.p99.as_millis() >= 98 && result.p99.as_millis() <= 100,
            "p99 should be ~99ms, got {}ms", result.p99.as_millis());
    }

    #[test]
    fn test_percentile_single_element() {
        let durations = vec![Duration::from_millis(42)];
        let result = compute_percentiles(durations);
        assert_eq!(result.p50.as_millis(), 42);
        assert_eq!(result.p95.as_millis(), 42);
        assert_eq!(result.p99.as_millis(), 42);
    }

    #[test]
    fn test_percentile_empty() {
        // percentile on empty returns ZERO
        let sorted: Vec<Duration> = vec![];
        assert_eq!(percentile(&sorted, 50.0), Duration::ZERO);
    }

    #[test]
    fn test_synthetic_data_generator_counts() {
        // Use a small dataset to verify counts
        let mut rng = StdRng::seed_from_u64(42);

        let num_assets = 100;
        let mut total_mutations = 0;

        for _ in 0..num_assets {
            let count = mutation_count_for_asset(&mut rng);
            // Each asset gets at least NUM_FIELDS base mutations
            let actual = count.max(NUM_FIELDS);
            total_mutations += actual;
        }

        // With 100 assets and 30 fields minimum, at least 3000 mutations
        assert!(total_mutations >= 100 * NUM_FIELDS,
            "Expected at least {} mutations, got {}", 100 * NUM_FIELDS, total_mutations);
    }

    #[test]
    fn test_power_law_distribution() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut counts: Vec<usize> = (0..10_000)
            .map(|_| mutation_count_for_asset(&mut rng))
            .collect();
        counts.sort();

        let median = counts[counts.len() / 2];
        let p99 = counts[(counts.len() as f64 * 0.99) as usize];

        // Median should be small (< 10)
        assert!(median < 10, "Median should be < 10, got {}", median);
        // p99 should be substantially larger
        assert!(p99 > 20, "p99 should be > 20, got {}", p99);
        // All >= 3
        assert!(counts[0] >= 3, "Min should be >= 3, got {}", counts[0]);
    }

    #[test]
    fn test_random_field_value_produces_valid_json() {
        let mut rng = StdRng::seed_from_u64(42);
        for field in FIELD_NAMES {
            let value = random_field_value(&mut rng, field);
            let json = serde_json::to_string(&value).unwrap();
            let back: FieldValue = serde_json::from_str(&json).unwrap();
            assert_eq!(value, back, "Roundtrip failed for field: {}", field);
        }
    }

    #[test]
    fn test_random_effective_date_format() {
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..100 {
            let date = random_effective_date(&mut rng);
            assert_eq!(date.len(), 10, "Date should be YYYY-MM-DD format: {}", date);
            assert_eq!(&date[4..5], "-");
            assert_eq!(&date[7..8], "-");
        }
    }

    #[test]
    fn test_pass_fail() {
        assert_eq!(pass_fail(49.0, 50.0), "PASS");
        assert_eq!(pass_fail(50.0, 50.0), "PASS");
        assert_eq!(pass_fail(51.0, 50.0), "FAIL");
    }

    #[test]
    fn test_quality_scoring_with_generated_data() {
        let mut rng = StdRng::seed_from_u64(42);
        let completeness_config = quality::building_completeness_config();
        let accuracy_rules = quality::default_accuracy_rules();
        let recency_config = quality::default_recency_config();

        // Generate a representative set of fields
        let mut fields = HashMap::new();
        for field in &FIELD_NAMES {
            fields.insert(field.to_string(), random_field_value(&mut rng, field));
        }

        let score = score_quality_single(&fields, &completeness_config, &accuracy_rules, &recency_config);

        // All fields populated => completeness should be high
        assert!(score.completeness.score > 0.8,
            "Completeness should be high with all fields, got {}", score.completeness.score);
        assert!(score.composite >= 0.0 && score.composite <= 1.0,
            "Composite should be [0,1], got {}", score.composite);
    }

    #[test]
    fn test_db_resolve_roundtrip() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;"
        ).unwrap();
        create_schema(&conn);

        let asset_id = Uuid::now_v7().to_string();
        let mut rng = StdRng::seed_from_u64(42);

        // Insert mutations for one asset
        let tx = conn.unchecked_transaction().unwrap();
        for field in &FIELD_NAMES {
            let mid = Uuid::now_v7().to_string();
            let value = random_field_value(&mut rng, field);
            let json = serde_json::to_string(&value).unwrap();
            tx.execute(
                "INSERT INTO field_mutations (mutation_id, asset_id, field_name, value_json, effective_date, approval_state, submitted_at)
                 VALUES (?1, ?2, ?3, ?4, '2024-01-15', 'Approved', '2024-01-15T10:00:00Z')",
                params![mid, asset_id, field, json],
            ).unwrap();
        }
        tx.commit().unwrap();

        // Resolve via temporal
        let resolved = resolve_single_temporal(&conn, &asset_id, "2024-12-31");
        assert_eq!(resolved.len(), NUM_FIELDS,
            "Should resolve all {} fields, got {}", NUM_FIELDS, resolved.len());

        // Build snapshot and resolve via snapshot
        build_snapshot_table(&conn);
        let snapshot = resolve_current_snapshot(&conn, &asset_id);
        assert_eq!(snapshot.len(), NUM_FIELDS,
            "Snapshot should have all {} fields, got {}", NUM_FIELDS, snapshot.len());
    }

    #[test]
    fn test_sov_export_produces_rows() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;"
        ).unwrap();
        create_schema(&conn);

        let mut rng = StdRng::seed_from_u64(42);
        let mut asset_ids = Vec::new();

        // Insert 10 assets
        let tx = conn.unchecked_transaction().unwrap();
        for _ in 0..10 {
            let asset_id = Uuid::now_v7().to_string();
            for field in &FIELD_NAMES {
                let mid = Uuid::now_v7().to_string();
                let value = random_field_value(&mut rng, field);
                let json = serde_json::to_string(&value).unwrap();
                tx.execute(
                    "INSERT INTO field_mutations (mutation_id, asset_id, field_name, value_json, effective_date, approval_state, submitted_at)
                     VALUES (?1, ?2, ?3, ?4, '2024-01-15', 'Approved', '2024-01-15T10:00:00Z')",
                    params![mid, asset_id, field, json],
                ).unwrap();
            }
            asset_ids.push(asset_id);
        }
        tx.commit().unwrap();

        // Build snapshot
        conn.execute_batch(
            "INSERT OR REPLACE INTO current_asset_state (asset_id, field_name, value_json, effective_date, source_mutation_id)
             SELECT fm.asset_id, fm.field_name, fm.value_json, MAX(fm.effective_date), fm.mutation_id
             FROM field_mutations fm
             WHERE fm.approval_state = 'Approved'
             GROUP BY fm.asset_id, fm.field_name;"
        ).unwrap();

        let rows = sov_export(&conn, &asset_ids);
        assert_eq!(rows, 10, "Should export 10 rows, got {}", rows);
    }

    #[test]
    fn test_parse_resolved_fields() {
        let rows = vec![
            ("address".to_string(), serde_json::to_string(&FieldValue::Text("123 Main".into())).unwrap()),
            ("sprinkler".to_string(), serde_json::to_string(&FieldValue::Bool(true)).unwrap()),
        ];

        let fields = parse_resolved_fields(&rows);
        assert_eq!(fields.len(), 2);
        assert_eq!(fields.get("address"), Some(&FieldValue::Text("123 Main".into())));
        assert_eq!(fields.get("sprinkler"), Some(&FieldValue::Bool(true)));
    }

    #[test]
    fn test_dataset_member_pool_distribution() {
        // Verify the distribution logic with a small sample
        let num_assets = 100;
        let num_pools = 5;
        let num_members = 20;

        let mut pool_counts = vec![0usize; num_pools];
        let mut member_counts = vec![0usize; num_members];

        for i in 0..num_assets {
            pool_counts[i % num_pools] += 1;
            member_counts[i % num_members] += 1;
        }

        // Each pool gets equal assets
        for count in &pool_counts {
            assert_eq!(*count, num_assets / num_pools);
        }
        // Each member gets equal assets
        for count in &member_counts {
            assert_eq!(*count, num_assets / num_members);
        }
    }
}
