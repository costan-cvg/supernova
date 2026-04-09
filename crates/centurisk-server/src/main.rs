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

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM pools", [], |r| r.get(0))
        .unwrap_or(0);

    if count > 0 {
        return;
    }

    // Two pools with members and users across all major role categories
    conn.execute_batch(
        "-- Pool A: Demo Risk Pool
         INSERT INTO pools (pool_id, name, created_by)
           VALUES ('00000000-0000-0000-0000-000000000010', 'Demo Risk Pool', '00000000-0000-0000-0000-000000000001');
         INSERT INTO members (member_id, pool_id, name, created_by)
           VALUES ('00000000-0000-0000-0000-000000000020', '00000000-0000-0000-0000-000000000010', 'City of Springfield', '00000000-0000-0000-0000-000000000001');
         INSERT INTO members (member_id, pool_id, name, created_by)
           VALUES ('00000000-0000-0000-0000-000000000021', '00000000-0000-0000-0000-000000000010', 'Town of Shelbyville', '00000000-0000-0000-0000-000000000001');

         -- Pool B: Separate pool for cross-tenant isolation testing
         INSERT INTO pools (pool_id, name, created_by)
           VALUES ('00000000-0000-0000-0000-000000000011', 'Coastal Counties Pool', '00000000-0000-0000-0000-000000000001');
         INSERT INTO members (member_id, pool_id, name, created_by)
           VALUES ('00000000-0000-0000-0000-000000000030', '00000000-0000-0000-0000-000000000011', 'City of Oceanview', '00000000-0000-0000-0000-000000000001');

         -- Users: one per major role
         INSERT INTO users (user_id, email, display_name, category, pool_id, member_id)
           VALUES ('00000000-0000-0000-0000-000000000001', 'admin@centurisk.dev', 'Alice Admin', 'CentuRiskAdmin', '00000000-0000-0000-0000-000000000010', NULL);

         INSERT INTO users (user_id, email, display_name, category, pool_id, member_id)
           VALUES ('00000000-0000-0000-0000-000000000002', 'pooladmin@demo.pool', 'Bob Pool-Admin', 'PoolAdministrator', '00000000-0000-0000-0000-000000000010', NULL);

         INSERT INTO users (user_id, email, display_name, category, pool_id, member_id)
           VALUES ('00000000-0000-0000-0000-000000000003', 'member@springfield.gov', 'Carol Member', 'MemberUser', '00000000-0000-0000-0000-000000000010', '00000000-0000-0000-0000-000000000020');

         INSERT INTO users (user_id, email, display_name, category, pool_id, member_id)
           VALUES ('00000000-0000-0000-0000-000000000004', 'pooladmin@coastal.pool', 'Dave Coastal-Admin', 'PoolAdministrator', '00000000-0000-0000-0000-000000000011', NULL);

         INSERT INTO users (user_id, email, display_name, category, pool_id, member_id)
           VALUES ('00000000-0000-0000-0000-000000000005', 'member@oceanview.gov', 'Eve Ocean-Member', 'MemberUser', '00000000-0000-0000-0000-000000000011', '00000000-0000-0000-0000-000000000030');
        "
    ).expect("Failed to seed demo data");

    tracing::info!("Seeded 2 pools, 3 members, 5 users");
}
