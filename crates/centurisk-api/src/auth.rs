use axum::extract::FromRequestParts;
use centurisk_auth::principal::{Principal, UserCategory};
use centurisk_auth::TenantContext;
use centurisk_core::ids::{ActorId, PoolId};
use serde::Serialize;

use crate::AppState;

/// Axum extractor that provides the authenticated principal.
/// Inc 1.4: Returns a hardcoded System Admin.
/// Inc 1.5: Will read from JWT session.
pub struct Auth(pub Principal);

#[axum::async_trait]
impl FromRequestParts<AppState> for Auth {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(Auth(hardcoded_admin()))
    }
}

fn hardcoded_admin() -> Principal {
    Principal {
        actor_id: ActorId::from_uuid(uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()),
        category: UserCategory::CentuRiskAdmin,
        pool_id: None,
        member_id: None,
        profile_ids: vec!["centurisk-admin".into()],
    }
}

/// Derive a TenantContext from the authenticated principal.
pub fn tenant_from_principal(principal: &Principal) -> TenantContext {
    match principal.category {
        UserCategory::CentuRiskAdmin | UserCategory::CentuRiskAnalyst
        | UserCategory::CentuRiskAuditor | UserCategory::CentuRiskSupport => {
            // CentuRisk users get a default pool context
            // In production, they'd select a pool or see cross-pool views
            let pool_id = principal.pool_id.unwrap_or_else(|| {
                PoolId::from_uuid(uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000010").unwrap())
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

#[derive(Serialize)]
pub struct MeResponse {
    pub user_id: String,
    pub display_name: String,
    pub category: String,
}

pub async fn me(Auth(principal): Auth) -> axum::Json<MeResponse> {
    axum::Json(MeResponse {
        user_id: principal.actor_id.to_string(),
        display_name: "System Admin".into(),
        category: format!("{:?}", principal.category),
    })
}
