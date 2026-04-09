use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let app = centurisk_api::app();
    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();

    tracing::info!("CentuRisk server listening on 0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}
