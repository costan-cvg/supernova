//! Principal — the authenticated identity making a request.

use centurisk_core::ids::{ActorId, MemberId, PoolId};
use serde::{Deserialize, Serialize};

/// User category within CentuRisk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserCategory {
    CentuRiskAdmin,
    CentuRiskAnalyst,
    CentuRiskAuditor,
    CentuRiskSupport,
    PoolAdministrator,
    PoolAnalyst,
    MemberAdmin,
    MemberUser,
    MemberReadOnly,
    PoolReadOnly,
}

/// The authenticated principal making a request.
/// Derived from the identity provider token + CentuRisk user database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Principal {
    pub actor_id: ActorId,
    pub category: UserCategory,
    pub pool_id: Option<PoolId>,
    pub member_id: Option<MemberId>,
    pub profile_ids: Vec<String>,
}
