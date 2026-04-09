pub mod health;

use axum::Router;

pub fn app() -> Router {
    Router::new().merge(health::routes())
}
