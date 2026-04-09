use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
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

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info,tower_http=debug".into());

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .json();

    let honeycomb_key = std::env::var("HONEYCOMB_API_KEY").ok();

    if let Some(api_key) = honeycomb_key {
        // Honeycomb OTLP configuration via standard env vars.
        // The SDK reads these automatically — no need to pass them programmatically.
        // Dataset is determined by OTEL_SERVICE_NAME in modern Honeycomb (Environments & Services).
        // x-honeycomb-dataset header is only needed for Classic keys.
        std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "https://api.honeycomb.io");
        std::env::set_var(
            "OTEL_EXPORTER_OTLP_HEADERS",
            format!("x-honeycomb-team={api_key}"),
        );
        std::env::set_var("OTEL_SERVICE_NAME", "riskstar");

        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .build()
            .expect("Failed to create OTLP exporter");

        let tracer_provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_resource(
                opentelemetry_sdk::Resource::builder()
                    .with_service_name("riskstar")
                    .build(),
            )
            .build();

        // Register globally so traces flush on shutdown
        use opentelemetry::trace::TracerProvider;
        let tracer = tracer_provider.tracer("riskstar");
        opentelemetry::global::set_tracer_provider(tracer_provider);

        let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .with(otel_layer)
            .init();

        tracing::info!("OpenTelemetry exporting to Honeycomb (service: riskstar)");
    } else {
        // Local-only: structured JSON logging to stdout
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();

        tracing::info!("No HONEYCOMB_API_KEY set — local structured logging only");
    }
}

#[tokio::main]
async fn main() {
    init_tracing();

    let db = centurisk_db::init_db(&db_path()).expect("Failed to initialize database");
    let policy = Arc::new(centurisk_auth::CedarPolicyGate::new());
    tracing::info!("Cedar ABAC policy engine loaded");

    let state = centurisk_api::AppState { db, policy };

    let static_path = static_dir();
    let index_file = format!("{static_path}/index.html");

    let app = centurisk_api::app(state)
        .layer(TraceLayer::new_for_http())
        .nest_service("/static", ServeDir::new(&static_path))
        .fallback_service(ServeFile::new(&index_file));

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::info!("RiskStar server listening on 0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}
