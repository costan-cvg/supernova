pub mod approvals;
pub mod auth;
pub mod assets;
pub mod dashboard;
pub mod health;
pub mod onboard;
pub mod quality;
pub mod renewals;

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
        .merge(dashboard::routes())
        .merge(renewals::routes())
        .route("/api/me", get(auth::me))
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024)) // 10MB for CSV imports
        .with_state(state)
}
