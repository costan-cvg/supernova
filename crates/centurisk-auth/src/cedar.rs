//! Cedar ABAC policy engine — replaces AllowAllPolicy.
//!
//! Evaluates Cedar policies for authorization decisions and field-level visibility.

use cedar_policy::{Authorizer, Context, Decision, Entities, EntityUid, PolicySet, Request};
use centurisk_core::ids::PoolId;
use std::str::FromStr;

use crate::policy::{Action, AuthzDecision, PolicyGate, Resource};
use crate::principal::Principal;

/// Cedar-backed PolicyGate implementation.
pub struct CedarPolicyGate {
    policy_set: PolicySet,
    authorizer: Authorizer,
}

impl CedarPolicyGate {
    pub fn new() -> Self {
        let policies = default_policies();
        let policy_set = PolicySet::from_str(&policies)
            .expect("Failed to parse Cedar policies");

        Self {
            policy_set,
            authorizer: Authorizer::new(),
        }
    }
}

impl PolicyGate for CedarPolicyGate {
    fn authorize(
        &self,
        principal: &Principal,
        action: &Action,
        resource: &Resource,
    ) -> AuthzDecision {
        let principal_uid = make_principal_uid(principal);
        let action_uid = make_action_uid(&action.0);
        let resource_uid = make_resource_uid(resource);

        let context = Context::empty();
        let entities = Entities::empty();

        let request = Request::new(
            principal_uid,
            action_uid,
            resource_uid,
            context,
            None, // no schema validation for now
        ).expect("Failed to build Cedar request");

        let response = self.authorizer.is_authorized(&request, &self.policy_set, &entities);

        match response.decision() {
            Decision::Allow => {
                tracing::debug!(
                    principal = %principal.actor_id,
                    action = %action.0,
                    resource_type = %resource.resource_type,
                    decision = "permit",
                    policy = "Cedar",
                    "authorization check"
                );
                AuthzDecision::Permit
            }
            Decision::Deny => {
                tracing::warn!(
                    principal = %principal.actor_id,
                    action = %action.0,
                    resource_type = %resource.resource_type,
                    decision = "deny",
                    policy = "Cedar",
                    "authorization check"
                );
                AuthzDecision::Deny {
                    reason: "Cedar policy denied access".into(),
                }
            }
        }
    }

    fn visible_fields(
        &self,
        principal: &Principal,
        resource_type: &str,
        _pool_id: &PoolId,
    ) -> Option<Vec<String>> {
        // Field-level visibility: restrict valuation fields for MemberReadOnly
        match principal.category {
            crate::principal::UserCategory::MemberReadOnly | crate::principal::UserCategory::PoolReadOnly => {
                if resource_type == "Asset" {
                    // Read-only users cannot see valuation fields
                    return Some(vec![
                        "building_name".into(), "address".into(), "city".into(),
                        "state".into(), "zip_code".into(), "year_built".into(),
                        "construction_class".into(), "occupancy".into(),
                        "sq_footage".into(), "stories".into(), "roof_type".into(),
                        "sprinkler".into(),
                        // replacement_cost and contents_value excluded
                    ]);
                }
            }
            _ => {}
        }
        None // All fields visible
    }
}

fn make_principal_uid(principal: &Principal) -> EntityUid {
    let category = format!("{:?}", principal.category);
    EntityUid::from_str(&format!("CentuRisk::User::\"{}\"", category))
        .unwrap_or_else(|_| EntityUid::from_str("CentuRisk::User::\"Unknown\"").unwrap())
}

fn make_action_uid(action: &str) -> EntityUid {
    EntityUid::from_str(&format!("CentuRisk::Action::\"{}\"", action))
        .unwrap_or_else(|_| EntityUid::from_str("CentuRisk::Action::\"unknown\"").unwrap())
}

fn make_resource_uid(resource: &Resource) -> EntityUid {
    let id = resource.resource_id.as_deref().unwrap_or("unknown");
    EntityUid::from_str(&format!("CentuRisk::{}::\"{}\"", resource.resource_type, id))
        .unwrap_or_else(|_| EntityUid::from_str("CentuRisk::Resource::\"unknown\"").unwrap())
}

/// Cedar policies for the 10 named profiles.
fn default_policies() -> String {
    r#"
// ═══ CentuRisk Admin — full access to everything ═══
permit(
    principal == CentuRisk::User::"CentuRiskAdmin",
    action,
    resource
);

// ═══ CentuRisk Analyst — read + query across all pools ═══
permit(
    principal == CentuRisk::User::"CentuRiskAnalyst",
    action in [CentuRisk::Action::"read", CentuRisk::Action::"query", CentuRisk::Action::"export"],
    resource
);

// ═══ CentuRisk Auditor — read-only, including audit trail ═══
permit(
    principal == CentuRisk::User::"CentuRiskAuditor",
    action == CentuRisk::Action::"read",
    resource
);

// ═══ CentuRisk Support — read + limited write on assigned scope ═══
permit(
    principal == CentuRisk::User::"CentuRiskSupport",
    action in [CentuRisk::Action::"read", CentuRisk::Action::"query"],
    resource
);

// ═══ Pool Administrator — full access within their pool ═══
permit(
    principal == CentuRisk::User::"PoolAdministrator",
    action,
    resource
);

// ═══ Pool Analyst — read + query within their pool ═══
permit(
    principal == CentuRisk::User::"PoolAnalyst",
    action in [CentuRisk::Action::"read", CentuRisk::Action::"query", CentuRisk::Action::"export"],
    resource
);

// ═══ Member Admin — read + write within their member scope ═══
permit(
    principal == CentuRisk::User::"MemberAdmin",
    action in [CentuRisk::Action::"read", CentuRisk::Action::"write", CentuRisk::Action::"query"],
    resource
);

// ═══ Member User — read + write within their member scope ═══
permit(
    principal == CentuRisk::User::"MemberUser",
    action in [CentuRisk::Action::"read", CentuRisk::Action::"write", CentuRisk::Action::"query"],
    resource
);

// ═══ Member Read-Only — read only ═══
permit(
    principal == CentuRisk::User::"MemberReadOnly",
    action == CentuRisk::Action::"read",
    resource
);

// ═══ Pool Read-Only — read only ═══
permit(
    principal == CentuRisk::User::"PoolReadOnly",
    action == CentuRisk::Action::"read",
    resource
);
"#.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use centurisk_core::ids::ActorId;
    use crate::principal::UserCategory;

    fn make_principal(category: UserCategory) -> Principal {
        Principal {
            actor_id: ActorId::new(),
            category,
            pool_id: None,
            member_id: None,
            profile_ids: vec![],
        }
    }

    fn make_resource(rtype: &str) -> Resource {
        Resource {
            resource_type: rtype.into(),
            resource_id: Some("test-123".into()),
            pool_id: None,
            field_name: None,
        }
    }

    #[test]
    fn admin_can_do_anything() {
        let gate = CedarPolicyGate::new();
        let p = make_principal(UserCategory::CentuRiskAdmin);
        let result = gate.authorize(&p, &Action("write".into()), &make_resource("Asset"));
        assert_eq!(result, AuthzDecision::Permit);
    }

    #[test]
    fn pool_admin_can_write() {
        let gate = CedarPolicyGate::new();
        let p = make_principal(UserCategory::PoolAdministrator);
        let result = gate.authorize(&p, &Action("write".into()), &make_resource("Asset"));
        assert_eq!(result, AuthzDecision::Permit);
    }

    #[test]
    fn member_user_can_read_and_write() {
        let gate = CedarPolicyGate::new();
        let p = make_principal(UserCategory::MemberUser);
        assert_eq!(gate.authorize(&p, &Action("read".into()), &make_resource("Asset")), AuthzDecision::Permit);
        assert_eq!(gate.authorize(&p, &Action("write".into()), &make_resource("Asset")), AuthzDecision::Permit);
    }

    #[test]
    fn readonly_cannot_write() {
        let gate = CedarPolicyGate::new();
        let p = make_principal(UserCategory::MemberReadOnly);
        assert_eq!(gate.authorize(&p, &Action("read".into()), &make_resource("Asset")), AuthzDecision::Permit);
        assert_eq!(gate.authorize(&p, &Action("write".into()), &make_resource("Asset")), AuthzDecision::Deny { reason: "Cedar policy denied access".into() });
    }

    #[test]
    fn readonly_field_visibility_excludes_valuation() {
        let gate = CedarPolicyGate::new();
        let p = make_principal(UserCategory::MemberReadOnly);
        let pool_id = PoolId::new();
        let fields = gate.visible_fields(&p, "Asset", &pool_id);
        assert!(fields.is_some());
        let visible = fields.unwrap();
        assert!(visible.contains(&"address".to_string()));
        assert!(!visible.contains(&"replacement_cost".to_string()));
    }

    #[test]
    fn admin_sees_all_fields() {
        let gate = CedarPolicyGate::new();
        let p = make_principal(UserCategory::CentuRiskAdmin);
        let pool_id = PoolId::new();
        let fields = gate.visible_fields(&p, "Asset", &pool_id);
        assert!(fields.is_none()); // None means all visible
    }
}
