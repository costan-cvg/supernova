use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::services::{ServeDir, ServeFile};
use tracing_subscriber::EnvFilter;

fn static_dir() -> String {
    std::env::var("CENTURISK_STATIC_DIR")
        .unwrap_or_else(|_| "./crates/centurisk-web/static".to_string())
}

fn db_path() -> PathBuf {
    std::env::var("CENTURISK_DB_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./data/centurisk.db"))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let db = centurisk_db::init_db(&db_path()).expect("Failed to initialize database");
    let policy = Arc::new(centurisk_auth::AllowAllPolicy);

    // Seed a default pool and member for demo purposes
    seed_demo_data(&db);

    let state = centurisk_api::AppState { db, policy };

    let static_path = static_dir();
    let index_file = format!("{static_path}/index.html");

    let app = centurisk_api::app(state)
        .nest_service("/static", ServeDir::new(&static_path))
        .fallback_service(ServeFile::new(&index_file));

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::info!("CentuRisk server listening on 0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}

fn seed_demo_data(db: &centurisk_db::DbPool) {
    let conn = db.get().expect("Failed to get DB connection for seeding");

    // Only seed if no pools exist
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM pools", [], |r| r.get(0))
        .unwrap_or(0);

    if count > 0 {
        return;
    }

    let pool_id = "00000000-0000-0000-0000-000000000010";
    let member_id = "00000000-0000-0000-0000-000000000020";
    let actor_id = "00000000-0000-0000-0000-000000000001";

    conn.execute_batch(&format!(
        "INSERT INTO pools (pool_id, name, created_by) VALUES ('{pool_id}', 'Demo Risk Pool', '{actor_id}');
         INSERT INTO members (member_id, pool_id, name, created_by) VALUES ('{member_id}', '{pool_id}', 'City of Springfield', '{actor_id}');
         INSERT INTO users (user_id, email, display_name, category, pool_id) VALUES ('{actor_id}', 'admin@centurisk.dev', 'System Admin', 'CentuRiskAdmin', '{pool_id}');"
    )).expect("Failed to seed demo data");

    tracing::info!("Seeded demo pool, member, and admin user");
}
