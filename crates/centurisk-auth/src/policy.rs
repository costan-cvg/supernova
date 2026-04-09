//! PolicyGate trait — the authorization boundary.
//! Every handler calls this from Inc 1 onward.

use centurisk_core::ids::PoolId;

/// The result of an authorization decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthzDecision {
    Permit,
    Deny { reason: String },
}

/// What action is being requested.
#[derive(Debug, Clone)]
pub struct Action(pub String);

/// What resource is being accessed.
#[derive(Debug, Clone)]
pub struct Resource {
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub pool_id: Option<PoolId>,
    pub field_name: Option<String>,
}

/// The authorization gate. Every data path must call this.
/// Inc 1-4: AllowAllPolicy (logs, always permits).
/// Inc 5+: Cedar replaces AllowAllPolicy.
pub trait PolicyGate: Send + Sync {
    fn authorize(
        &self,
        principal: &super::Principal,
        action: &Action,
        resource: &Resource,
    ) -> AuthzDecision;

    fn visible_fields(
        &self,
        principal: &super::Principal,
        resource_type: &str,
        pool_id: &PoolId,
    ) -> Option<Vec<String>>;
}

/// Permissive stub — logs every decision via tracing, always returns Permit.
/// Replaced by Cedar in Inc 5. Never used in production.
pub struct AllowAllPolicy;

impl PolicyGate for AllowAllPolicy {
    fn authorize(
        &self,
        principal: &super::Principal,
        action: &Action,
        resource: &Resource,
    ) -> AuthzDecision {
        tracing::debug!(
            principal = %principal.actor_id,
            action = %action.0,
            resource_type = %resource.resource_type,
            decision = "permit",
            policy = "AllowAllPolicy",
            "authorization check (stub)"
        );
        AuthzDecision::Permit
    }

    fn visible_fields(
        &self,
        _principal: &super::Principal,
        _resource_type: &str,
        _pool_id: &PoolId,
    ) -> Option<Vec<String>> {
        // None = all fields visible (no restriction)
        None
    }
}
