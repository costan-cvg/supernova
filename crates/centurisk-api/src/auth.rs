use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use centurisk_auth::principal::{Principal, UserCategory};
use centurisk_auth::TenantContext;
use centurisk_core::ids::{ActorId, MemberId, PoolId};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::AppState;

// In production this would be loaded from config/env. Fine for Phase 1.
const JWT_SECRET: &str = "centurisk-dev-secret-do-not-use-in-production";

// ── JWT Claims ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,        // user_id
    pub name: String,       // display_name
    pub category: String,   // UserCategory variant name
    pub pool_id: Option<String>,
    pub member_id: Option<String>,
    pub exp: usize,
}

fn make_token(user: &UserRow) -> Result<String, StatusCode> {
    let exp = jsonwebtoken::get_current_timestamp() as usize + 86400; // 24h
    let claims = Claims {
        sub: user.user_id.clone(),
        name: user.display_name.clone(),
        category: user.category.clone(),
        pool_id: user.pool_id.clone(),
        member_id: user.member_id.clone(),
        exp,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(JWT_SECRET.as_bytes()),
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn decode_token(token: &str) -> Result<Claims, StatusCode> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(JWT_SECRET.as_bytes()),
        &Validation::default(),
    )
    .map(|data| data.claims)
    .map_err(|_| StatusCode::UNAUTHORIZED)
}

fn parse_category(s: &str) -> UserCategory {
    match s {
        "CentuRiskAdmin" => UserCategory::CentuRiskAdmin,
        "CentuRiskAnalyst" => UserCategory::CentuRiskAnalyst,
        "CentuRiskAuditor" => UserCategory::CentuRiskAuditor,
        "CentuRiskSupport" => UserCategory::CentuRiskSupport,
        "PoolAdministrator" => UserCategory::PoolAdministrator,
        "PoolAnalyst" => UserCategory::PoolAnalyst,
        "MemberAdmin" => UserCategory::MemberAdmin,
        "MemberUser" => UserCategory::MemberUser,
        "MemberReadOnly" => UserCategory::MemberReadOnly,
        "PoolReadOnly" => UserCategory::PoolReadOnly,
        _ => UserCategory::MemberReadOnly,
    }
}

fn parse_uuid(s: &str) -> uuid::Uuid {
    uuid::Uuid::parse_str(s).unwrap_or_else(|_| uuid::Uuid::nil())
}

// ── Auth Extractor ──────────────────────────────────────────────────────────

/// Axum extractor — reads JWT from `Authorization: Bearer <token>` header
/// or `centurisk_session` cookie. Falls back to hardcoded admin if no token.
pub struct Auth(pub Principal);

#[axum::async_trait]
impl FromRequestParts<AppState> for Auth {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Try Authorization header first
        if let Some(auth_header) = parts.headers.get("authorization") {
            if let Ok(val) = auth_header.to_str() {
                if let Some(token) = val.strip_prefix("Bearer ") {
                    let claims = decode_token(token)?;
                    return Ok(Auth(claims_to_principal(&claims)));
                }
            }
        }

        // Try cookie
        if let Some(cookie_header) = parts.headers.get("cookie") {
            if let Ok(cookies) = cookie_header.to_str() {
                for cookie in cookies.split(';') {
                    let cookie = cookie.trim();
                    if let Some(token) = cookie.strip_prefix("centurisk_session=") {
                        let claims = decode_token(token)?;
                        return Ok(Auth(claims_to_principal(&claims)));
                    }
                }
            }
        }

        // No token — fall back to hardcoded admin (preserves backward compat)
        Ok(Auth(Principal {
            actor_id: ActorId::from_uuid(parse_uuid("00000000-0000-0000-0000-000000000001")),
            category: UserCategory::CentuRiskAdmin,
            pool_id: Some(PoolId::from_uuid(parse_uuid("00000000-0000-0000-0000-000000000010"))),
            member_id: None,
            profile_ids: vec!["centurisk-admin".into()],
        }))
    }
}

fn claims_to_principal(claims: &Claims) -> Principal {
    Principal {
        actor_id: ActorId::from_uuid(parse_uuid(&claims.sub)),
        category: parse_category(&claims.category),
        pool_id: claims.pool_id.as_deref().map(|s| PoolId::from_uuid(parse_uuid(s))),
        member_id: claims.member_id.as_deref().map(|s| MemberId::from_uuid(parse_uuid(s))),
        profile_ids: vec![claims.category.clone()],
    }
}

/// Derive a TenantContext from the authenticated principal.
pub fn tenant_from_principal(principal: &Principal) -> TenantContext {
    match principal.category {
        UserCategory::CentuRiskAdmin | UserCategory::CentuRiskAnalyst
        | UserCategory::CentuRiskAuditor | UserCategory::CentuRiskSupport => {
            let pool_id = principal.pool_id.unwrap_or_else(|| {
                PoolId::from_uuid(parse_uuid("00000000-0000-0000-0000-000000000010"))
            });
            TenantContext::pool_wide(pool_id)
        }
        UserCategory::PoolAdministrator | UserCategory::PoolAnalyst | UserCategory::PoolReadOnly => {
            TenantContext::pool_wide(principal.pool_id.expect("Pool users must have pool_id"))
        }
        _ => {
            let pool_id = principal.pool_id.expect("Member users must have pool_id");
            let member_id = principal.member_id.expect("Member users must have member_id");
            TenantContext::member_scoped(pool_id, member_id)
        }
    }
}

// ── Login / Me endpoints ────────────────────────────────────────────────────

#[derive(Serialize)]
struct UserRow {
    user_id: String,
    display_name: String,
    category: String,
    pool_id: Option<String>,
    member_id: Option<String>,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub user_id: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: MeResponse,
}

#[derive(Serialize)]
pub struct MeResponse {
    pub user_id: String,
    pub display_name: String,
    pub category: String,
    pub pool_id: Option<String>,
    pub member_id: Option<String>,
}

/// POST /api/login — select a user by user_id, get back a JWT.
async fn login(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    let conn = state.db.get().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let user = conn
        .query_row(
            "SELECT user_id, display_name, category, pool_id, member_id FROM users WHERE user_id = ?1",
            rusqlite::params![req.user_id],
            |row| {
                Ok(UserRow {
                    user_id: row.get(0)?,
                    display_name: row.get(1)?,
                    category: row.get(2)?,
                    pool_id: row.get(3)?,
                    member_id: row.get(4)?,
                })
            },
        )
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let token = make_token(&user)?;

    Ok(Json(LoginResponse {
        token,
        user: MeResponse {
            user_id: user.user_id,
            display_name: user.display_name,
            category: user.category,
            pool_id: user.pool_id,
            member_id: user.member_id,
        },
    }))
}

/// GET /api/users — list all users (for the login selector).
async fn list_users(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Result<Json<Vec<MeResponse>>, StatusCode> {
    let conn = state.db.get().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut stmt = conn
        .prepare("SELECT user_id, display_name, category, pool_id, member_id FROM users ORDER BY category, display_name")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let users: Vec<MeResponse> = stmt
        .query_map([], |row| {
            Ok(MeResponse {
                user_id: row.get(0)?,
                display_name: row.get(1)?,
                category: row.get(2)?,
                pool_id: row.get(3)?,
                member_id: row.get(4)?,
            })
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .filter_map(|r| r.ok())
        .collect();

    Ok(Json(users))
}

/// GET /api/me — return current user from JWT.
pub async fn me(Auth(principal): Auth) -> Json<MeResponse> {
    Json(MeResponse {
        user_id: principal.actor_id.to_string(),
        display_name: format!("{:?}", principal.category), // Will be overridden by JWT name
        category: format!("{:?}", principal.category),
        pool_id: principal.pool_id.map(|p| p.to_string()),
        member_id: principal.member_id.map(|m| m.to_string()),
    })
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/login", post(login))
        .route("/api/users", get(list_users))
}
