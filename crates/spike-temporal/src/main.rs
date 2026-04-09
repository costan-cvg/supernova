//! Spike 1: Temporal Resolution Benchmark
//!
//! Validates that field-level state resolution performs at scale:
//! 100K assets, 30 fields each, ~9M mutations in SQLite WAL mode.
//!
//! Benchmarks 3 strategies:
//! 1. Indexed field-level scan
//! 2. Pre-computed snapshot table
//! 3. Hybrid (mutations + materialized current state)

use centurisk_core::field_value::FieldValue;
use rand::prelude::*;
use rust_decimal::Decimal;
use rusqlite::{params, Connection};
use std::path::Path;
use std::time::{Duration, Instant};
use time::Date;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const NUM_ASSETS: usize = 100_000;
const NUM_FIELDS: usize = 30;
const BATCH_INSERT_SIZE: usize = 10_000;
const SINGLE_BENCHMARK_ITERATIONS: usize = 1_000;
const BATCH_BENCHMARK_ITERATIONS: usize = 100;
const BATCH_RESOLVE_SIZE: usize = 100;

/// p95 targets in milliseconds.
const P95_SINGLE_TARGET_MS: f64 = 50.0;
const P95_BATCH_TARGET_MS: f64 = 500.0;

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
        "address" | "building_name" | "city" | "county" => {
            FieldValue::Text(format!("{} Main St", rng.gen_range(100..9999)))
        }
        "replacement_cost" | "contents_value" => FieldValue::Money {
            amount: Decimal::new(rng.gen_range(100_000i64..50_000_000i64), 0),
            currency: "USD".into(),
        },
        "year_built" | "electrical_update_year" | "plumbing_update_year" => {
            FieldValue::Number(Decimal::from(rng.gen_range(1920..2024)))
        }
        "construction_class" => {
            let classes = ["Frame", "Joisted Masonry", "Non-Combustible", "Masonry", "Fire Resistive", "Modified Fire Resistive"];
            FieldValue::Enum(classes[rng.gen_range(0..classes.len())].into())
        }
        "occupancy" => {
            let types = ["Office", "Retail", "Warehouse", "Manufacturing", "School", "Municipal", "Fire Station", "Library"];
            FieldValue::Enum(types[rng.gen_range(0..types.len())].into())
        }
        "sq_footage" | "parking_spaces" | "elevator_count" | "stories" => {
            FieldValue::Number(Decimal::from(rng.gen_range(500..100_000)))
        }
        "sprinkler" | "basement" | "fire_alarm" | "security_system" => {
            FieldValue::Bool(rng.gen_bool(0.5))
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
        "zip_code" => FieldValue::Text(format!("{:05}", rng.gen_range(10000..99999))),
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
    // Use Pareto-like distribution: base count + heavy tail
    let u: f64 = rng.gen_range(0.0..1.0);
    // Inverse CDF of Pareto: x_min * (1 - u)^(-1/alpha)
    // alpha = 1.5 gives a heavy tail; x_min = 3
    let x_min = 3.0_f64;
    let alpha = 1.5_f64;
    let count = x_min * (1.0 - u).powf(-1.0 / alpha);
    // Cap at 300 to avoid extreme outliers eating all memory
    (count as usize).min(300).max(3)
}

/// Submitted-at timestamp (just use effective_date + some offset).
fn random_submitted_at(rng: &mut impl Rng, effective_date: &str) -> String {
    // Append a time component
    let hour = rng.gen_range(0..24);
    let min = rng.gen_range(0..60);
    let sec = rng.gen_range(0..60);
    format!("{effective_date}T{hour:02}:{min:02}:{sec:02}Z")
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
    )
    .expect("Failed to set pragmas");

    conn
}

fn create_mutations_table(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS field_mutations (
            mutation_id TEXT PRIMARY KEY,
            asset_id TEXT NOT NULL,
            field_name TEXT NOT NULL,
            value_json TEXT NOT NULL,
            effective_date TEXT NOT NULL,
            approval_state TEXT NOT NULL DEFAULT 'approved',
            submitted_at TEXT NOT NULL
        );",
    )
    .expect("Failed to create field_mutations table");
}

fn create_strategy1_index(conn: &Connection) {
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_mutations_asset_field_date
         ON field_mutations(asset_id, field_name, effective_date DESC);",
    )
    .expect("Failed to create strategy 1 index");
}

fn create_snapshot_table(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS current_asset_state (
            asset_id TEXT NOT NULL,
            field_name TEXT NOT NULL,
            value_json TEXT NOT NULL,
            effective_date TEXT NOT NULL,
            source_mutation_id TEXT NOT NULL,
            PRIMARY KEY (asset_id, field_name)
        );",
    )
    .expect("Failed to create current_asset_state table");
}

// ---------------------------------------------------------------------------
// Data generation
// ---------------------------------------------------------------------------

struct GeneratedData {
    /// (asset_id, [(mutation_id, field_name, value_json, effective_date, submitted_at)])
    assets: Vec<(String, Vec<MutationRow>)>,
    total_mutations: usize,
}

struct MutationRow {
    mutation_id: String,
    field_name: String,
    value_json: String,
    effective_date: String,
    submitted_at: String,
}

fn generate_data() -> GeneratedData {
    let mut rng = StdRng::seed_from_u64(42); // Reproducible
    let mut assets = Vec::with_capacity(NUM_ASSETS);
    let mut total_mutations = 0;

    println!("Generating test data...");
    let gen_start = Instant::now();

    for i in 0..NUM_ASSETS {
        let asset_id = Uuid::now_v7().to_string();
        let mutation_count = mutation_count_for_asset(&mut rng);
        let mut mutations = Vec::with_capacity(mutation_count);

        // Each asset gets at least one mutation per field (initial state),
        // then additional mutations distributed across fields.
        // First pass: one mutation per field
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

        // Additional mutations beyond the base 30
        let extra = if mutation_count > NUM_FIELDS {
            mutation_count - NUM_FIELDS
        } else {
            0
        };
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
        assets.push((asset_id, mutations));

        if (i + 1) % 10_000 == 0 {
            println!(
                "  Generated {}/{} assets ({} mutations so far)...",
                i + 1,
                NUM_ASSETS,
                total_mutations
            );
        }
    }

    let gen_elapsed = gen_start.elapsed();
    println!(
        "Data generation complete: {} assets, {} mutations in {:.1}s",
        assets.len(),
        total_mutations,
        gen_elapsed.as_secs_f64()
    );

    GeneratedData {
        assets,
        total_mutations,
    }
}

fn insert_mutations(conn: &Connection, data: &GeneratedData) {
    println!("Inserting mutations into database...");
    let insert_start = Instant::now();

    let mut count = 0;
    let total = data.total_mutations;

    // Process in batches using transactions
    let mut batch: Vec<(&str, &MutationRow)> = Vec::with_capacity(BATCH_INSERT_SIZE);

    for (asset_id, mutations) in &data.assets {
        for mutation in mutations {
            batch.push((asset_id.as_str(), mutation));

            if batch.len() >= BATCH_INSERT_SIZE {
                insert_batch(conn, &batch);
                count += batch.len();
                batch.clear();

                if count % 500_000 == 0 {
                    println!("  Inserted {}/{} mutations...", count, total);
                }
            }
        }
    }

    // Insert remaining
    if !batch.is_empty() {
        insert_batch(conn, &batch);
        count += batch.len();
    }

    let insert_elapsed = insert_start.elapsed();
    println!(
        "Insertion complete: {} mutations in {:.1}s ({:.0} inserts/sec)",
        count,
        insert_elapsed.as_secs_f64(),
        count as f64 / insert_elapsed.as_secs_f64()
    );
}

fn insert_batch(conn: &Connection, batch: &[(&str, &MutationRow)]) {
    let tx = conn.unchecked_transaction().expect("Failed to begin transaction");
    {
        let mut stmt = tx
            .prepare_cached(
                "INSERT INTO field_mutations (mutation_id, asset_id, field_name, value_json, effective_date, approval_state, submitted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'approved', ?6)",
            )
            .expect("Failed to prepare insert statement");

        for (asset_id, mutation) in batch {
            stmt.execute(params![
                mutation.mutation_id,
                asset_id,
                mutation.field_name,
                mutation.value_json,
                mutation.effective_date,
                mutation.submitted_at,
            ])
            .expect("Failed to insert mutation");
        }
    }
    tx.commit().expect("Failed to commit transaction");
}

// ---------------------------------------------------------------------------
// Strategy 1: Indexed field-level scan
// ---------------------------------------------------------------------------

fn resolve_strategy1(conn: &Connection, asset_id: &str, as_of_date: &str) -> Vec<(String, String, String)> {
    // SQLite-specific: when MAX(effective_date) is in the SELECT list with GROUP BY,
    // SQLite guarantees non-aggregated columns (value_json) come from the same row
    // as the MAX value. This is a documented SQLite extension to standard SQL.
    let mut stmt = conn
        .prepare_cached(
            "SELECT field_name, value_json, MAX(effective_date) as effective_date
             FROM field_mutations
             WHERE asset_id = ?1 AND approval_state = 'approved' AND effective_date <= ?2
             GROUP BY field_name",
        )
        .expect("Failed to prepare strategy 1 query");

    let rows = stmt
        .query_map(params![asset_id, as_of_date], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .expect("Failed to execute strategy 1 query");

    rows.map(|r| r.unwrap()).collect()
}

// ---------------------------------------------------------------------------
// Strategy 2: Pre-computed snapshot table
// ---------------------------------------------------------------------------

fn build_snapshot_table(conn: &Connection) {
    println!("Building snapshot table (Strategy 2)...");
    let start = Instant::now();

    conn.execute_batch(
        "INSERT OR REPLACE INTO current_asset_state (asset_id, field_name, value_json, effective_date, source_mutation_id)
         SELECT fm.asset_id, fm.field_name, fm.value_json, MAX(fm.effective_date), fm.mutation_id
         FROM field_mutations fm
         WHERE fm.approval_state = 'approved'
         GROUP BY fm.asset_id, fm.field_name;",
    )
    .expect("Failed to build snapshot table");

    let elapsed = start.elapsed();
    println!(
        "Snapshot table built in {:.1}s",
        elapsed.as_secs_f64()
    );
}

fn resolve_strategy2(conn: &Connection, asset_id: &str) -> Vec<(String, String, String)> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT field_name, value_json, effective_date
             FROM current_asset_state
             WHERE asset_id = ?1",
        )
        .expect("Failed to prepare strategy 2 query");

    let rows = stmt
        .query_map(params![asset_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .expect("Failed to execute strategy 2 query");

    rows.map(|r| r.unwrap()).collect()
}

/// Measure write overhead for Strategy 2: inserting a mutation + updating snapshot.
fn measure_strategy2_write_overhead(conn: &Connection, asset_ids: &[String]) -> Duration {
    let mut rng = StdRng::seed_from_u64(999);
    let iterations = 1000;
    let mut total = Duration::ZERO;

    for _ in 0..iterations {
        let asset_id = &asset_ids[rng.gen_range(0..asset_ids.len())];
        let field_name = FIELD_NAMES[rng.gen_range(0..NUM_FIELDS)];
        let value = random_field_value(&mut rng, field_name);
        let value_json = serde_json::to_string(&value).unwrap();
        let mutation_id = Uuid::now_v7().to_string();
        let effective_date = "2024-06-15";
        let submitted_at = "2024-06-15T12:00:00Z";

        let start = Instant::now();

        let tx = conn.unchecked_transaction().unwrap();
        tx.execute(
            "INSERT INTO field_mutations (mutation_id, asset_id, field_name, value_json, effective_date, approval_state, submitted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'approved', ?6)",
            params![mutation_id, asset_id, field_name, value_json, effective_date, submitted_at],
        )
        .unwrap();
        // Full snapshot rebuild for this asset's field (Strategy 2 approach)
        tx.execute(
            "INSERT OR REPLACE INTO current_asset_state (asset_id, field_name, value_json, effective_date, source_mutation_id)
             SELECT fm.asset_id, fm.field_name, fm.value_json, MAX(fm.effective_date), fm.mutation_id
             FROM field_mutations fm
             WHERE fm.asset_id = ?1 AND fm.field_name = ?2 AND fm.approval_state = 'approved'
             GROUP BY fm.field_name",
            params![asset_id, field_name],
        )
        .unwrap();
        tx.commit().unwrap();

        total += start.elapsed();
    }

    total / iterations as u32
}

// ---------------------------------------------------------------------------
// Strategy 3: Hybrid
// ---------------------------------------------------------------------------

/// For current-date queries, use snapshot table (same as strategy 2).
/// For historical (as-of-date) queries, fall back to strategy 1.
fn resolve_strategy3_current(conn: &Connection, asset_id: &str) -> Vec<(String, String, String)> {
    resolve_strategy2(conn, asset_id)
}

fn resolve_strategy3_historical(conn: &Connection, asset_id: &str, as_of_date: &str) -> Vec<(String, String, String)> {
    resolve_strategy1(conn, asset_id, as_of_date)
}

/// Measure write overhead for Strategy 3: insert mutation + targeted snapshot update
/// in the same transaction.
fn measure_strategy3_write_overhead(conn: &Connection, asset_ids: &[String]) -> Duration {
    let mut rng = StdRng::seed_from_u64(888);
    let iterations = 1000;
    let mut total = Duration::ZERO;

    for _ in 0..iterations {
        let asset_id = &asset_ids[rng.gen_range(0..asset_ids.len())];
        let field_name = FIELD_NAMES[rng.gen_range(0..NUM_FIELDS)];
        let value = random_field_value(&mut rng, field_name);
        let value_json = serde_json::to_string(&value).unwrap();
        let mutation_id = Uuid::now_v7().to_string();
        let effective_date = "2024-06-15";
        let submitted_at = "2024-06-15T12:00:00Z";

        let start = Instant::now();

        let tx = conn.unchecked_transaction().unwrap();

        // Insert mutation
        tx.execute(
            "INSERT INTO field_mutations (mutation_id, asset_id, field_name, value_json, effective_date, approval_state, submitted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'approved', ?6)",
            params![mutation_id, asset_id, field_name, value_json, effective_date, submitted_at],
        )
        .unwrap();

        // Update snapshot only if this mutation is the latest for this field
        tx.execute(
            "INSERT OR REPLACE INTO current_asset_state (asset_id, field_name, value_json, effective_date, source_mutation_id)
             SELECT ?1, ?2, ?3, ?4, ?5
             WHERE NOT EXISTS (
                 SELECT 1 FROM current_asset_state
                 WHERE asset_id = ?1 AND field_name = ?2 AND effective_date > ?4
             )",
            params![asset_id, field_name, value_json, effective_date, mutation_id],
        )
        .unwrap();

        tx.commit().unwrap();

        total += start.elapsed();
    }

    total / iterations as u32
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
        write!(
            f,
            "p50={:.2}ms p95={:.2}ms p99={:.2}ms",
            self.p50.as_secs_f64() * 1000.0,
            self.p95.as_secs_f64() * 1000.0,
            self.p99.as_secs_f64() * 1000.0,
        )
    }
}

fn percentile(sorted: &[Duration], pct: f64) -> Duration {
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

fn pick_random_asset_ids(asset_ids: &[String], count: usize, rng: &mut impl Rng) -> Vec<String> {
    let mut picked = Vec::with_capacity(count);
    for _ in 0..count {
        picked.push(asset_ids[rng.gen_range(0..asset_ids.len())].clone());
    }
    picked
}

// ---------------------------------------------------------------------------
// Run benchmarks
// ---------------------------------------------------------------------------

fn benchmark_strategy1(conn: &Connection, asset_ids: &[String]) -> (BenchmarkResult, BenchmarkResult) {
    let as_of_date = "2024-12-31";
    let mut rng = StdRng::seed_from_u64(100);

    // Warmup
    for _ in 0..10 {
        let id = &asset_ids[rng.gen_range(0..asset_ids.len())];
        let _ = resolve_strategy1(conn, id, as_of_date);
    }

    // Single-asset benchmark
    let mut single_durations = Vec::with_capacity(SINGLE_BENCHMARK_ITERATIONS);
    for _ in 0..SINGLE_BENCHMARK_ITERATIONS {
        let id = &asset_ids[rng.gen_range(0..asset_ids.len())];
        let start = Instant::now();
        let _result = resolve_strategy1(conn, id, as_of_date);
        single_durations.push(start.elapsed());
    }

    // Batch benchmark
    let mut batch_durations = Vec::with_capacity(BATCH_BENCHMARK_ITERATIONS);
    for _ in 0..BATCH_BENCHMARK_ITERATIONS {
        let batch = pick_random_asset_ids(asset_ids, BATCH_RESOLVE_SIZE, &mut rng);
        let start = Instant::now();
        for id in &batch {
            let _result = resolve_strategy1(conn, id, as_of_date);
        }
        batch_durations.push(start.elapsed());
    }

    (
        compute_percentiles(single_durations),
        compute_percentiles(batch_durations),
    )
}

fn benchmark_strategy2(conn: &Connection, asset_ids: &[String]) -> (BenchmarkResult, BenchmarkResult) {
    let mut rng = StdRng::seed_from_u64(200);

    // Warmup
    for _ in 0..10 {
        let id = &asset_ids[rng.gen_range(0..asset_ids.len())];
        let _ = resolve_strategy2(conn, id);
    }

    // Single-asset benchmark
    let mut single_durations = Vec::with_capacity(SINGLE_BENCHMARK_ITERATIONS);
    for _ in 0..SINGLE_BENCHMARK_ITERATIONS {
        let id = &asset_ids[rng.gen_range(0..asset_ids.len())];
        let start = Instant::now();
        let _result = resolve_strategy2(conn, id);
        single_durations.push(start.elapsed());
    }

    // Batch benchmark
    let mut batch_durations = Vec::with_capacity(BATCH_BENCHMARK_ITERATIONS);
    for _ in 0..BATCH_BENCHMARK_ITERATIONS {
        let batch = pick_random_asset_ids(asset_ids, BATCH_RESOLVE_SIZE, &mut rng);
        let start = Instant::now();
        for id in &batch {
            let _result = resolve_strategy2(conn, id);
        }
        batch_durations.push(start.elapsed());
    }

    (
        compute_percentiles(single_durations),
        compute_percentiles(batch_durations),
    )
}

fn benchmark_strategy3(conn: &Connection, asset_ids: &[String]) -> (BenchmarkResult, BenchmarkResult, BenchmarkResult, BenchmarkResult) {
    let mut rng = StdRng::seed_from_u64(300);
    let as_of_date = "2023-06-15"; // Historical query

    // Warmup
    for _ in 0..10 {
        let id = &asset_ids[rng.gen_range(0..asset_ids.len())];
        let _ = resolve_strategy3_current(conn, id);
        let _ = resolve_strategy3_historical(conn, id, as_of_date);
    }

    // Current-date: single-asset
    let mut current_single = Vec::with_capacity(SINGLE_BENCHMARK_ITERATIONS);
    for _ in 0..SINGLE_BENCHMARK_ITERATIONS {
        let id = &asset_ids[rng.gen_range(0..asset_ids.len())];
        let start = Instant::now();
        let _result = resolve_strategy3_current(conn, id);
        current_single.push(start.elapsed());
    }

    // Current-date: batch
    let mut current_batch = Vec::with_capacity(BATCH_BENCHMARK_ITERATIONS);
    for _ in 0..BATCH_BENCHMARK_ITERATIONS {
        let batch = pick_random_asset_ids(asset_ids, BATCH_RESOLVE_SIZE, &mut rng);
        let start = Instant::now();
        for id in &batch {
            let _result = resolve_strategy3_current(conn, id);
        }
        current_batch.push(start.elapsed());
    }

    // Historical: single-asset
    let mut hist_single = Vec::with_capacity(SINGLE_BENCHMARK_ITERATIONS);
    for _ in 0..SINGLE_BENCHMARK_ITERATIONS {
        let id = &asset_ids[rng.gen_range(0..asset_ids.len())];
        let start = Instant::now();
        let _result = resolve_strategy3_historical(conn, id, as_of_date);
        hist_single.push(start.elapsed());
    }

    // Historical: batch
    let mut hist_batch = Vec::with_capacity(BATCH_BENCHMARK_ITERATIONS);
    for _ in 0..BATCH_BENCHMARK_ITERATIONS {
        let batch = pick_random_asset_ids(asset_ids, BATCH_RESOLVE_SIZE, &mut rng);
        let start = Instant::now();
        for id in &batch {
            let _result = resolve_strategy3_historical(conn, id, as_of_date);
        }
        hist_batch.push(start.elapsed());
    }

    (
        compute_percentiles(current_single),
        compute_percentiles(current_batch),
        compute_percentiles(hist_single),
        compute_percentiles(hist_batch),
    )
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let tmp_dir = std::env::temp_dir().join("spike-temporal");
    std::fs::create_dir_all(&tmp_dir).expect("Failed to create temp directory");
    let db_path = tmp_dir.join("benchmark.db");

    // Remove old DB if it exists
    let _ = std::fs::remove_file(&db_path);

    println!("=== Spike 1: Temporal Resolution Benchmark ===");
    println!("Database path: {}", db_path.display());
    println!();

    // Step 1: Generate data
    let data = generate_data();
    let asset_ids: Vec<String> = data.assets.iter().map(|(id, _)| id.clone()).collect();

    // Step 2: Create database and insert data
    let conn = create_db(&db_path);
    create_mutations_table(&conn);
    insert_mutations(&conn, &data);

    // Get DB size after mutations
    let db_size_bytes = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);
    let db_size_mb = db_size_bytes as f64 / (1024.0 * 1024.0);

    println!();
    println!(
        "Assets: {} | Fields: {} | Total mutations: {}",
        NUM_ASSETS, NUM_FIELDS, data.total_mutations
    );
    println!("Database size: {:.1} MB", db_size_mb);
    println!();

    // --- Strategy 1: Indexed Field-Level Scan ---
    println!("Creating index for Strategy 1...");
    create_strategy1_index(&conn);
    println!("Running Strategy 1 benchmark...");
    let (s1_single, s1_batch) = benchmark_strategy1(&conn, &asset_ids);

    println!();
    println!("--- Strategy 1: Indexed Field-Level Scan ---");
    println!("Single asset: {}", s1_single);
    println!("100-batch:    {}", s1_batch);

    // --- Strategy 2: Pre-Computed Snapshot ---
    create_snapshot_table(&conn);
    build_snapshot_table(&conn);
    println!("Running Strategy 2 benchmark...");
    let (s2_single, s2_batch) = benchmark_strategy2(&conn, &asset_ids);
    let s2_write_overhead = measure_strategy2_write_overhead(&conn, &asset_ids);

    println!();
    println!("--- Strategy 2: Pre-Computed Snapshot ---");
    println!("Single asset: {}", s2_single);
    println!("100-batch:    {}", s2_batch);
    println!(
        "Write overhead: {:.3}ms per mutation",
        s2_write_overhead.as_secs_f64() * 1000.0
    );

    // --- Strategy 3: Hybrid ---
    println!();
    println!("Running Strategy 3 benchmark...");
    let (s3_current_single, s3_current_batch, s3_hist_single, s3_hist_batch) =
        benchmark_strategy3(&conn, &asset_ids);
    let s3_write_overhead = measure_strategy3_write_overhead(&conn, &asset_ids);

    println!();
    println!("--- Strategy 3: Hybrid (snapshot + fallback to scan) ---");
    println!("Current-date:");
    println!("  Single asset: {}", s3_current_single);
    println!("  100-batch:    {}", s3_current_batch);
    println!("Historical (as-of-date):");
    println!("  Single asset: {}", s3_hist_single);
    println!("  100-batch:    {}", s3_hist_batch);
    println!(
        "Write overhead: {:.3}ms per mutation",
        s3_write_overhead.as_secs_f64() * 1000.0
    );

    // --- Final DB size ---
    let final_db_size_bytes = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);
    let final_db_size_mb = final_db_size_bytes as f64 / (1024.0 * 1024.0);
    println!();
    println!("Final database size (with snapshot table): {:.1} MB", final_db_size_mb);

    // --- Pass/Fail ---
    println!();
    println!("=== PASS/FAIL ASSESSMENT ===");
    println!("Target: p95 single-asset < {}ms, p95 100-batch < {}ms", P95_SINGLE_TARGET_MS, P95_BATCH_TARGET_MS);
    println!();

    let strategies = [
        (
            "Strategy 1 (Indexed Scan)",
            s1_single.p95,
            s1_batch.p95,
        ),
        (
            "Strategy 2 (Snapshot)",
            s2_single.p95,
            s2_batch.p95,
        ),
        (
            "Strategy 3 Hybrid (current)",
            s3_current_single.p95,
            s3_current_batch.p95,
        ),
        (
            "Strategy 3 Hybrid (historical)",
            s3_hist_single.p95,
            s3_hist_batch.p95,
        ),
    ];

    let mut any_pass = false;
    let mut best_strategy = "";

    for (name, single_p95, batch_p95) in &strategies {
        let single_ms = single_p95.as_secs_f64() * 1000.0;
        let batch_ms = batch_p95.as_secs_f64() * 1000.0;
        let single_pass = single_ms < P95_SINGLE_TARGET_MS;
        let batch_pass = batch_ms < P95_BATCH_TARGET_MS;
        let pass = single_pass && batch_pass;

        let status = if pass { "PASS" } else { "FAIL" };
        println!(
            "  {}: {} (single p95={:.2}ms {}, batch p95={:.2}ms {})",
            name,
            status,
            single_ms,
            if single_pass { "OK" } else { "EXCEED" },
            batch_ms,
            if batch_pass { "OK" } else { "EXCEED" },
        );

        if pass && !any_pass {
            any_pass = true;
            best_strategy = name;
        }
    }

    println!();
    if any_pass {
        println!("=== RESULT: {} meets all targets ===", best_strategy);
    } else {
        println!("=== RESULT: No strategy meets all targets. Further optimization needed. ===");
    }

    // Cleanup
    let _ = std::fs::remove_file(&db_path);
    // Also remove WAL and SHM files
    let _ = std::fs::remove_file(db_path.with_extension("db-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("db-shm"));
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> (Connection, Vec<String>) {
        let conn = Connection::open_in_memory().expect("Failed to open in-memory database");
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;",
        )
        .unwrap();

        create_mutations_table(&conn);
        create_strategy1_index(&conn);
        create_snapshot_table(&conn);

        // Insert a small dataset: 3 assets, 3 fields, a few mutations each
        let asset_ids: Vec<String> = (0..3).map(|_| Uuid::now_v7().to_string()).collect();

        let fields = ["address", "replacement_cost", "year_built"];

        let tx = conn.unchecked_transaction().unwrap();
        for asset_id in &asset_ids {
            for field in &fields {
                // Initial mutation
                let mid = Uuid::now_v7().to_string();
                let value = match *field {
                    "address" => serde_json::to_string(&FieldValue::Text("123 Main St".into())).unwrap(),
                    "replacement_cost" => serde_json::to_string(&FieldValue::Money {
                        amount: Decimal::new(1_000_000, 0),
                        currency: "USD".into(),
                    }).unwrap(),
                    "year_built" => serde_json::to_string(&FieldValue::Number(Decimal::from(1990))).unwrap(),
                    _ => unreachable!(),
                };
                tx.execute(
                    "INSERT INTO field_mutations VALUES (?1, ?2, ?3, ?4, '2020-01-15', 'approved', '2020-01-15T10:00:00Z')",
                    params![mid, asset_id, field, value],
                ).unwrap();

                // Later mutation
                let mid2 = Uuid::now_v7().to_string();
                let value2 = match *field {
                    "address" => serde_json::to_string(&FieldValue::Text("456 Oak Ave".into())).unwrap(),
                    "replacement_cost" => serde_json::to_string(&FieldValue::Money {
                        amount: Decimal::new(1_500_000, 0),
                        currency: "USD".into(),
                    }).unwrap(),
                    "year_built" => serde_json::to_string(&FieldValue::Number(Decimal::from(1990))).unwrap(),
                    _ => unreachable!(),
                };
                tx.execute(
                    "INSERT INTO field_mutations VALUES (?1, ?2, ?3, ?4, '2023-06-15', 'approved', '2023-06-15T10:00:00Z')",
                    params![mid2, asset_id, field, value2],
                ).unwrap();
            }
        }
        tx.commit().unwrap();

        (conn, asset_ids)
    }

    #[test]
    fn test_strategy1_resolves_latest_before_date() {
        let (conn, asset_ids) = setup_test_db();

        // Query as of 2024-01-01 should get the 2023 mutations
        let result = resolve_strategy1(&conn, &asset_ids[0], "2024-01-01");
        assert_eq!(result.len(), 3, "Should resolve all 3 fields");

        // Check address got updated value
        let address = result.iter().find(|(f, _, _)| f == "address").unwrap();
        assert!(address.1.contains("456 Oak Ave"), "Should have latest address");
        assert_eq!(address.2, "2023-06-15");

        // Query as of 2021-01-01 should get the 2020 mutations
        let result_old = resolve_strategy1(&conn, &asset_ids[0], "2021-01-01");
        assert_eq!(result_old.len(), 3, "Should resolve all 3 fields");
        let address_old = result_old.iter().find(|(f, _, _)| f == "address").unwrap();
        assert!(address_old.1.contains("123 Main St"), "Should have old address");
        assert_eq!(address_old.2, "2020-01-15");
    }

    #[test]
    fn test_strategy1_excludes_future_mutations() {
        let (conn, asset_ids) = setup_test_db();

        // Query as of 2019-01-01 should get nothing (all mutations are after this date)
        let result = resolve_strategy1(&conn, &asset_ids[0], "2019-01-01");
        assert_eq!(result.len(), 0, "No mutations before 2019");
    }

    #[test]
    fn test_strategy2_resolves_current_state() {
        let (conn, asset_ids) = setup_test_db();

        // Build the snapshot table
        build_snapshot_table(&conn);

        let result = resolve_strategy2(&conn, &asset_ids[0]);
        assert_eq!(result.len(), 3, "Should resolve all 3 fields");

        // Should have the latest values
        let address = result.iter().find(|(f, _, _)| f == "address").unwrap();
        assert!(address.1.contains("456 Oak Ave"), "Snapshot should have latest address");
    }

    #[test]
    fn test_strategy3_current_uses_snapshot() {
        let (conn, asset_ids) = setup_test_db();
        build_snapshot_table(&conn);

        let result = resolve_strategy3_current(&conn, &asset_ids[0]);
        assert_eq!(result.len(), 3);

        let address = result.iter().find(|(f, _, _)| f == "address").unwrap();
        assert!(address.1.contains("456 Oak Ave"));
    }

    #[test]
    fn test_strategy3_historical_falls_back_to_scan() {
        let (conn, asset_ids) = setup_test_db();
        build_snapshot_table(&conn);

        // Historical query should return old values
        let result = resolve_strategy3_historical(&conn, &asset_ids[0], "2021-01-01");
        assert_eq!(result.len(), 3);

        let address = result.iter().find(|(f, _, _)| f == "address").unwrap();
        assert!(address.1.contains("123 Main St"), "Historical query should return old address");
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
        assert!(median < 10, "Median mutation count should be < 10, got {}", median);
        // p99 should be substantially larger
        assert!(p99 > 20, "p99 should be > 20, got {}", p99);
        // All should be >= 3
        assert!(counts[0] >= 3, "Minimum should be >= 3");
    }

    #[test]
    fn test_field_value_json_roundtrip() {
        let values = vec![
            FieldValue::Text("hello".into()),
            FieldValue::Number(Decimal::new(12345, 2)),
            FieldValue::Bool(true),
            FieldValue::Money {
                amount: Decimal::new(100000, 0),
                currency: "USD".into(),
            },
            FieldValue::Null,
        ];

        for val in &values {
            let json = serde_json::to_string(val).unwrap();
            let back: FieldValue = serde_json::from_str(&json).unwrap();
            assert_eq!(*val, back);
        }
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
    fn test_percentile_computation() {
        let durations: Vec<Duration> = (1..=100)
            .map(|i| Duration::from_millis(i))
            .collect();

        let result = compute_percentiles(durations);

        // p50 should be around 50ms
        assert!(result.p50.as_millis() >= 49 && result.p50.as_millis() <= 51);
        // p95 should be around 95ms
        assert!(result.p95.as_millis() >= 94 && result.p95.as_millis() <= 96);
        // p99 should be around 99ms
        assert!(result.p99.as_millis() >= 98 && result.p99.as_millis() <= 100);
    }

    #[test]
    fn test_multiple_assets_independent() {
        let (conn, asset_ids) = setup_test_db();

        // Resolving different assets should return independent results
        let r1 = resolve_strategy1(&conn, &asset_ids[0], "2024-01-01");
        let r2 = resolve_strategy1(&conn, &asset_ids[1], "2024-01-01");

        assert_eq!(r1.len(), 3);
        assert_eq!(r2.len(), 3);

        // Both should have the same field names but values are independent
        let r1_fields: Vec<&str> = r1.iter().map(|(f, _, _)| f.as_str()).collect();
        let r2_fields: Vec<&str> = r2.iter().map(|(f, _, _)| f.as_str()).collect();
        assert_eq!(r1_fields.len(), r2_fields.len());
    }

    #[test]
    fn test_unapproved_mutations_excluded() {
        let conn = Connection::open_in_memory().unwrap();
        create_mutations_table(&conn);
        create_strategy1_index(&conn);

        let asset_id = Uuid::now_v7().to_string();
        let mid1 = Uuid::now_v7().to_string();
        let mid2 = Uuid::now_v7().to_string();

        let value_old = serde_json::to_string(&FieldValue::Text("old".into())).unwrap();
        let value_new = serde_json::to_string(&FieldValue::Text("new".into())).unwrap();

        conn.execute(
            "INSERT INTO field_mutations VALUES (?1, ?2, 'address', ?3, '2020-01-01', 'approved', '2020-01-01T00:00:00Z')",
            params![mid1, asset_id, value_old],
        ).unwrap();

        // Newer mutation but NOT approved
        conn.execute(
            "INSERT INTO field_mutations VALUES (?1, ?2, 'address', ?3, '2023-01-01', 'pending', '2023-01-01T00:00:00Z')",
            params![mid2, asset_id, value_new],
        ).unwrap();

        let result = resolve_strategy1(&conn, &asset_id, "2024-01-01");
        assert_eq!(result.len(), 1);
        assert!(result[0].1.contains("old"), "Should only see approved mutation");
    }
}
