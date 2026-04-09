//! SQLite persistence layer for CentuRisk RMIS.
//! Provides connection pooling, migrations, and repository access.

pub mod pool;

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use std::path::Path;
use thiserror::Error;

mod embedded {
    use refinery::embed_migrations;
    embed_migrations!("src/migrations");
}

#[derive(Error, Debug)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("connection pool error: {0}")]
    Pool(#[from] r2d2::Error),

    #[error("migration error: {0}")]
    Migration(String),
}

pub type DbPool = Pool<SqliteConnectionManager>;

/// Configure a SQLite connection with WAL mode and performance pragmas.
fn configure_connection(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA foreign_keys=ON;
         PRAGMA mmap_size=268435456;
         PRAGMA cache_size=-65536;
         PRAGMA temp_store=MEMORY;",
    )
}

/// Create a connection pool and run migrations.
pub fn init_db(db_path: &Path) -> Result<DbPool, DbError> {
    // Ensure parent directory exists
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let manager = SqliteConnectionManager::file(db_path)
        .with_init(|conn| configure_connection(conn));

    let pool = Pool::builder()
        .max_size(8)
        .build(manager)?;

    // Run migrations on a dedicated connection
    run_migrations(&pool)?;

    tracing::info!("Database initialized at {}", db_path.display());
    Ok(pool)
}

/// Create an in-memory pool for testing.
pub fn init_test_db() -> Result<DbPool, DbError> {
    let manager = SqliteConnectionManager::memory()
        .with_init(|conn| configure_connection(conn));

    let pool = Pool::builder()
        .max_size(1) // Single connection for in-memory to share state
        .build(manager)?;

    run_migrations(&pool)?;
    Ok(pool)
}

fn run_migrations(pool: &DbPool) -> Result<(), DbError> {
    let mut conn = pool.get()?;
    embedded::migrations::runner()
        .run(&mut *conn)
        .map_err(|e| DbError::Migration(e.to_string()))?;
    tracing::info!("Database migrations complete");
    Ok(())
}

/// Check database connectivity (for health endpoint).
pub fn health_check(pool: &DbPool) -> Result<(), DbError> {
    let conn = pool.get()?;
    conn.execute_batch("SELECT 1")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_and_migrate() {
        let pool = init_test_db().expect("Failed to init test DB");
        let conn = pool.get().unwrap();

        // Verify tables exist
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        assert!(tables.contains(&"pools".to_string()), "pools table missing");
        assert!(tables.contains(&"members".to_string()), "members table missing");
        assert!(tables.contains(&"users".to_string()), "users table missing");
        assert!(tables.contains(&"access_grants".to_string()), "access_grants table missing");
        assert!(tables.contains(&"audit_entries".to_string()), "audit_entries table missing");
    }

    #[test]
    fn test_health_check() {
        let pool = init_test_db().unwrap();
        assert!(health_check(&pool).is_ok());
    }

    #[test]
    fn test_foreign_keys_enforced() {
        let pool = init_test_db().unwrap();
        let conn = pool.get().unwrap();

        // Inserting a member with nonexistent pool_id should fail
        let result = conn.execute(
            "INSERT INTO members (member_id, pool_id, name, created_by) VALUES ('m1', 'nonexistent', 'Test', 'system')",
            [],
        );
        assert!(result.is_err(), "Foreign key should be enforced");
    }

    #[test]
    fn test_audit_entry_insert() {
        let pool = init_test_db().unwrap();
        let conn = pool.get().unwrap();

        // Create a pool first
        conn.execute(
            "INSERT INTO pools (pool_id, name, created_by) VALUES ('p1', 'Test Pool', 'system')",
            [],
        ).unwrap();

        // Insert an audit entry
        conn.execute(
            "INSERT INTO audit_entries (entry_id, entity_id, entity_type, effective_date, actor_id, actor_role, pool_id, operation)
             VALUES ('a1', 'p1', 'Pool', '2024-01-01', 'system', 'CentuRiskAdmin', 'p1', 'Create')",
            [],
        ).unwrap();

        let count: i64 = conn.query_row("SELECT COUNT(*) FROM audit_entries", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 1);
    }
}
