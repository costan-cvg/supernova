//! TenantContext — required on every repository operation.

use centurisk_core::ids::{MemberId, PoolId};
use serde::{Deserialize, Serialize};

/// Isolation scope for a repository operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IsolationScope {
    /// Pool-wide: can see all members in the pool.
    PoolWide,
    /// Member-scoped: can only see data for this specific member.
    MemberScoped { member_id: MemberId },
    /// Cross-pool: CentuRisk admin visibility across pools.
    CrossPool { pool_ids: Vec<PoolId> },
}

/// Injected into all repository operations. Enforces data isolation.
/// A repository operation without a TenantContext is a P0 defect.
#[derive(Debug, Clone)]
pub struct TenantContext {
    pub pool_id: PoolId,
    pub member_id: Option<MemberId>,
    pub isolation_scope: IsolationScope,
}

impl TenantContext {
    /// Create a pool-wide context (for pool admins).
    pub fn pool_wide(pool_id: PoolId) -> Self {
        Self {
            pool_id,
            member_id: None,
            isolation_scope: IsolationScope::PoolWide,
        }
    }

    /// Create a member-scoped context (for member users).
    pub fn member_scoped(pool_id: PoolId, member_id: MemberId) -> Self {
        Self {
            pool_id,
            member_id: Some(member_id),
            isolation_scope: IsolationScope::MemberScoped { member_id },
        }
    }

    /// Create a cross-pool context (for CentuRisk admins).
    pub fn cross_pool(pool_ids: Vec<PoolId>) -> Self {
        Self {
            // Use first pool as primary for queries that need a single pool_id
            pool_id: pool_ids[0],
            member_id: None,
            isolation_scope: IsolationScope::CrossPool { pool_ids },
        }
    }
}
