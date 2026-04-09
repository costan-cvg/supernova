//! In-app notifications — list, count unread, acknowledge.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::auth::Auth;
use crate::AppState;

// ── Types ───────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct Notification {
    pub notification_id: String,
    pub recipient_user_id: String,
    pub pool_id: String,
    pub source_type: String,
    pub source_id: Option<String>,
    pub priority: String,
    pub title: String,
    pub body: String,
    pub state: String,
    pub created_at: String,
    pub acknowledged_at: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct UnreadCount {
    pub unread: usize,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Serialize, Deserialize)]
struct AckResult {
    success: bool,
    count: usize,
}

// ── Helper: create_notification ─────────────────────────────────────────────

/// Create a new notification. Callable from other modules.
pub fn create_notification(
    conn: &Connection,
    recipient_user_id: &str,
    pool_id: &str,
    source_type: &str,
    source_id: Option<&str>,
    title: &str,
    body: &str,
    priority: &str,
) -> Result<String, rusqlite::Error> {
    let notification_id = uuid::Uuid::now_v7().to_string();

    conn.execute(
        "INSERT INTO notifications (notification_id, recipient_user_id, pool_id, source_type, source_id, priority, title, body)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![notification_id, recipient_user_id, pool_id, source_type, source_id, priority, title, body],
    )?;

    Ok(notification_id)
}

// ── GET /api/notifications ──────────────────────────────────────────────────

/// List notifications for the logged-in user, unacknowledged first.
async fn list_notifications(
    Auth(principal): Auth,
    State(state): State<AppState>,
) -> Result<Json<Vec<Notification>>, StatusCode> {
    let conn = state.db.get().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let user_id = principal.actor_id.to_string();

    let mut stmt = conn
        .prepare(
            "SELECT notification_id, recipient_user_id, pool_id, source_type, source_id,
                    priority, title, body, state, created_at, acknowledged_at
             FROM notifications
             WHERE recipient_user_id = ?1
             ORDER BY
                 CASE state WHEN 'Created' THEN 0 WHEN 'Delivered' THEN 1 ELSE 2 END,
                 created_at DESC
             LIMIT 50",
        )
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let notifications: Vec<Notification> = stmt
        .query_map(rusqlite::params![user_id], |row| {
            Ok(Notification {
                notification_id: row.get(0)?,
                recipient_user_id: row.get(1)?,
                pool_id: row.get(2)?,
                source_type: row.get(3)?,
                source_id: row.get(4)?,
                priority: row.get(5)?,
                title: row.get(6)?,
                body: row.get(7)?,
                state: row.get(8)?,
                created_at: row.get(9)?,
                acknowledged_at: row.get(10)?,
            })
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .filter_map(|r| r.ok())
        .collect();

    Ok(Json(notifications))
}

// ── GET /api/notifications/count ────────────────────────────────────────────

/// Return the number of unacknowledged notifications.
async fn unread_count(
    Auth(principal): Auth,
    State(state): State<AppState>,
) -> Result<Json<UnreadCount>, StatusCode> {
    let conn = state.db.get().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let user_id = principal.actor_id.to_string();

    let count: usize = conn
        .query_row(
            "SELECT COUNT(*) FROM notifications WHERE recipient_user_id = ?1 AND state != 'Acknowledged'",
            rusqlite::params![user_id],
            |row| row.get(0),
        )
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(UnreadCount { unread: count }))
}

// ── POST /api/notifications/:id/acknowledge ─────────────────────────────────

/// Mark a single notification as acknowledged.
async fn acknowledge_one(
    Auth(principal): Auth,
    State(state): State<AppState>,
    Path(notification_id): Path<String>,
) -> Result<Json<AckResult>, (StatusCode, Json<ErrorResponse>)> {
    let conn = state.db.get()
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "DB error".into() })))?;

    let user_id = principal.actor_id.to_string();
    let now = now_iso();

    let updated = conn
        .execute(
            "UPDATE notifications SET state = 'Acknowledged', acknowledged_at = ?1
             WHERE notification_id = ?2 AND recipient_user_id = ?3 AND state != 'Acknowledged'",
            rusqlite::params![now, notification_id, user_id],
        )
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Update failed".into() })))?;

    if updated == 0 {
        return Err((StatusCode::NOT_FOUND, Json(ErrorResponse {
            error: "Notification not found or already acknowledged".into(),
        })));
    }

    Ok(Json(AckResult { success: true, count: updated }))
}

// ── POST /api/notifications/acknowledge-all ─────────────────────────────────

/// Mark all notifications for the logged-in user as acknowledged.
async fn acknowledge_all(
    Auth(principal): Auth,
    State(state): State<AppState>,
) -> Result<Json<AckResult>, StatusCode> {
    let conn = state.db.get().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let user_id = principal.actor_id.to_string();
    let now = now_iso();

    let updated = conn
        .execute(
            "UPDATE notifications SET state = 'Acknowledged', acknowledged_at = ?1
             WHERE recipient_user_id = ?2 AND state != 'Acknowledged'",
            rusqlite::params![now, user_id],
        )
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(AckResult { success: true, count: updated }))
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn now_iso() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

// ── Routes ──────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/notifications", get(list_notifications))
        .route("/api/notifications/count", get(unread_count))
        .route("/api/notifications/:id/acknowledge", post(acknowledge_one))
        .route("/api/notifications/acknowledge-all", post(acknowledge_all))
}

// ── Tests ───────────────────────────────────────────────────────────────────

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

    /// Seed a notification and return (state, notification_id).
    fn seed_notification(state: &AppState, user_id: &str, pool_id: &str, title: &str) -> String {
        let conn = state.db.get().unwrap();
        create_notification(&conn, user_id, pool_id, "Test", None, title, "test body", "Normal").unwrap()
    }

    // The default Auth extractor falls back to the hardcoded admin user
    // with actor_id = 00000000-0000-0000-0000-000000000001.
    const DEFAULT_USER_ID: &str = "00000000-0000-0000-0000-000000000001";

    #[tokio::test]
    async fn test_list_notifications() {
        let state = test_state();
        seed_notification(&state, DEFAULT_USER_ID, "pool1", "First");
        seed_notification(&state, DEFAULT_USER_ID, "pool1", "Second");
        // Notification for a different user — should NOT appear
        seed_notification(&state, "other-user", "pool1", "Other");

        let app = routes().with_state(state);
        let req = Request::builder()
            .uri("/api/notifications")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
        let list: Vec<Notification> = serde_json::from_slice(&body).unwrap();
        assert_eq!(list.len(), 2);
        // Both should be for our user
        for n in &list {
            assert_eq!(n.recipient_user_id, DEFAULT_USER_ID);
        }
    }

    #[tokio::test]
    async fn test_unread_count() {
        let state = test_state();
        seed_notification(&state, DEFAULT_USER_ID, "pool1", "A");
        seed_notification(&state, DEFAULT_USER_ID, "pool1", "B");
        seed_notification(&state, DEFAULT_USER_ID, "pool1", "C");

        let app = routes().with_state(state);
        let req = Request::builder()
            .uri("/api/notifications/count")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let count: UnreadCount = serde_json::from_slice(&body).unwrap();
        assert_eq!(count.unread, 3);
    }

    #[tokio::test]
    async fn test_acknowledge_one() {
        let state = test_state();
        let nid = seed_notification(&state, DEFAULT_USER_ID, "pool1", "Ack me");

        let app = routes().with_state(state.clone());
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/notifications/{}/acknowledge", nid))
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let result: AckResult = serde_json::from_slice(&body).unwrap();
        assert!(result.success);
        assert_eq!(result.count, 1);

        // Verify count decreased
        let app2 = routes().with_state(state);
        let req2 = Request::builder()
            .uri("/api/notifications/count")
            .body(Body::empty())
            .unwrap();
        let resp2 = app2.oneshot(req2).await.unwrap();
        let body2 = axum::body::to_bytes(resp2.into_body(), 1024).await.unwrap();
        let count: UnreadCount = serde_json::from_slice(&body2).unwrap();
        assert_eq!(count.unread, 0);
    }

    #[tokio::test]
    async fn test_acknowledge_all() {
        let state = test_state();
        seed_notification(&state, DEFAULT_USER_ID, "pool1", "A");
        seed_notification(&state, DEFAULT_USER_ID, "pool1", "B");
        seed_notification(&state, DEFAULT_USER_ID, "pool1", "C");

        let app = routes().with_state(state.clone());
        let req = Request::builder()
            .method("POST")
            .uri("/api/notifications/acknowledge-all")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let result: AckResult = serde_json::from_slice(&body).unwrap();
        assert!(result.success);
        assert_eq!(result.count, 3);

        // Verify count is now 0
        let app2 = routes().with_state(state);
        let req2 = Request::builder()
            .uri("/api/notifications/count")
            .body(Body::empty())
            .unwrap();
        let resp2 = app2.oneshot(req2).await.unwrap();
        let body2 = axum::body::to_bytes(resp2.into_body(), 1024).await.unwrap();
        let count: UnreadCount = serde_json::from_slice(&body2).unwrap();
        assert_eq!(count.unread, 0);
    }

    #[tokio::test]
    async fn test_acknowledge_nonexistent_returns_not_found() {
        let state = test_state();
        let app = routes().with_state(state);
        let req = Request::builder()
            .method("POST")
            .uri("/api/notifications/nonexistent-id/acknowledge")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_count_excludes_acknowledged() {
        let state = test_state();
        let nid = seed_notification(&state, DEFAULT_USER_ID, "pool1", "Will ack");
        seed_notification(&state, DEFAULT_USER_ID, "pool1", "Stay unread");

        // Acknowledge the first one directly in DB
        {
            let conn = state.db.get().unwrap();
            conn.execute(
                "UPDATE notifications SET state = 'Acknowledged', acknowledged_at = '2026-01-01T00:00:00Z' WHERE notification_id = ?1",
                rusqlite::params![nid],
            ).unwrap();
        } // conn dropped here so the single-connection pool is available for the handler

        let app = routes().with_state(state);
        let req = Request::builder()
            .uri("/api/notifications/count")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let count: UnreadCount = serde_json::from_slice(&body).unwrap();
        assert_eq!(count.unread, 1);
    }
}
