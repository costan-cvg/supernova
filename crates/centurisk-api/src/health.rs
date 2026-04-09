use axum::{Json, Router, extract::State, routing::get};
use serde::Serialize;

use crate::AppState;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub db: &'static str,
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    let db_status = match centurisk_db::health_check(&state.db) {
        Ok(()) => "connected",
        Err(_) => "error",
    };
    Json(HealthResponse {
        status: "ok",
        db: db_status,
    })
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/health", get(health))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt as _;

    fn test_state() -> AppState {
        AppState {
            db: centurisk_db::init_test_db().unwrap(),
            policy: std::sync::Arc::new(centurisk_auth::AllowAllPolicy),
        }
    }

    #[tokio::test]
    async fn health_returns_ok_with_db() {
        let app = routes().with_state(test_state());
        let req = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
        assert_eq!(json["db"], "connected");
    }
}
