pub mod health;

use axum::Router;
use centurisk_db::DbPool;

#[derive(Clone)]
pub struct AppState {
    pub db: DbPool,
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .merge(health::routes())
        .with_state(state)
}
