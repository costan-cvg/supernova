pub mod approvals;
pub mod auth;
pub mod assets;
pub mod health;
pub mod onboard;
pub mod quality;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::get;
use centurisk_auth::PolicyGate;
use centurisk_db::DbPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: DbPool,
    pub policy: Arc<dyn PolicyGate>,
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .merge(health::routes())
        .merge(assets::routes())
        .merge(auth::routes())
        .merge(onboard::routes())
        .merge(quality::routes())
        .merge(approvals::routes())
        .route("/api/me", get(auth::me))
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024)) // 10MB for CSV imports
        .with_state(state)
}
