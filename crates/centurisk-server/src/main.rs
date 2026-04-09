use tokio::net::TcpListener;
use tower_http::services::{ServeDir, ServeFile};
use tracing_subscriber::EnvFilter;

fn static_dir() -> String {
    std::env::var("CENTURISK_STATIC_DIR")
        .unwrap_or_else(|_| "./crates/centurisk-web/static".to_string())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let static_path = static_dir();
    let index_file = format!("{static_path}/index.html");

    let app = centurisk_api::app()
        .nest_service("/static", ServeDir::new(&static_path))
        .fallback_service(ServeFile::new(&index_file));

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::info!("CentuRisk server listening on 0.0.0.0:3000");
    tracing::info!("Static files from: {static_path}");
    axum::serve(listener, app).await.unwrap();
}
