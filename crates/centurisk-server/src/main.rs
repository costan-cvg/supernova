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
    let policy = Arc::new(centurisk_auth::CedarPolicyGate::new());
    tracing::info!("Cedar ABAC policy engine loaded");

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
