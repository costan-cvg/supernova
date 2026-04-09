pub mod approvals;
pub mod auth;
pub mod assets;
pub mod custom_fields;
pub mod dashboard;
pub mod export;
pub mod health;
pub mod notifications;
pub mod onboard;
pub mod quality;
pub mod recommendations;
pub mod renewals;
pub mod search;

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
        .merge(recommendations::routes())
        .merge(notifications::routes())
        .merge(export::routes())
        .merge(custom_fields::routes())
        .merge(search::routes())
        .route("/api/me", get(auth::me))
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024))
        .with_state(state)
}
