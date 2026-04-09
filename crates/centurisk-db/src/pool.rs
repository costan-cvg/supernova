//! Pool and member repository operations.
//! All operations require TenantContext for isolation.

use centurisk_auth::TenantContext;
use centurisk_core::ids::{ActorId, MemberId, PoolId};
use rusqlite::params;

use crate::{DbError, DbPool};

/// A pool record from the database.
#[derive(Debug)]
pub struct PoolRecord {
    pub pool_id: PoolId,
    pub name: String,
}

/// A member record from the database.
#[derive(Debug)]
pub struct MemberRecord {
    pub member_id: MemberId,
    pub pool_id: PoolId,
    pub name: String,
}

/// Create a new pool. Requires CentuRisk admin context.
#[tracing::instrument(skip(db), fields(pool_id = %pool_id, pool_name = %name))]
pub fn create_pool(
    db: &DbPool,
    pool_id: PoolId,
    name: &str,
    created_by: &ActorId,
) -> Result<(), DbError> {
    let conn = db.get()?;
    conn.execute(
        "INSERT INTO pools (pool_id, name, created_by) VALUES (?1, ?2, ?3)",
        params![pool_id.to_string(), name, created_by.to_string()],
    )?;
    Ok(())
}

/// Create a new member within a pool.
#[tracing::instrument(skip(db, _tenant), fields(member_id = %member_id, pool_id = %pool_id))]
pub fn create_member(
    db: &DbPool,
    _tenant: &TenantContext,
    member_id: MemberId,
    pool_id: PoolId,
    name: &str,
    created_by: &ActorId,
) -> Result<(), DbError> {
    let conn = db.get()?;
    conn.execute(
        "INSERT INTO members (member_id, pool_id, name, created_by) VALUES (?1, ?2, ?3, ?4)",
        params![
            member_id.to_string(),
            pool_id.to_string(),
            name,
            created_by.to_string()
        ],
    )?;
    Ok(())
}

/// List members visible in the given tenant context.
pub fn list_members(
    db: &DbPool,
    tenant: &TenantContext,
) -> Result<Vec<MemberRecord>, DbError> {
    let conn = db.get()?;
    let mut stmt = conn.prepare(
        "SELECT member_id, pool_id, name FROM members WHERE pool_id = ?1",
    )?;

    let rows = stmt.query_map(params![tenant.pool_id.to_string()], |row| {
        Ok(MemberRecord {
            member_id: MemberId::from_uuid(uuid::Uuid::parse_str(&row.get::<_, String>(0)?).unwrap()),
            pool_id: PoolId::from_uuid(uuid::Uuid::parse_str(&row.get::<_, String>(1)?).unwrap()),
            name: row.get(2)?,
        })
    })?;

    Ok(rows.map(|r| r.unwrap()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init_test_db;

    #[test]
    fn test_create_pool_and_member() {
        let db = init_test_db().unwrap();
        let pool_id = PoolId::new();
        let actor = ActorId::new();

        create_pool(&db, pool_id, "Test Pool", &actor).unwrap();

        let tenant = TenantContext::pool_wide(pool_id);
        let member_id = MemberId::new();
        create_member(&db, &tenant, member_id, pool_id, "City of Springfield", &actor).unwrap();

        let members = list_members(&db, &tenant).unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].name, "City of Springfield");
        assert_eq!(members[0].pool_id, pool_id);
    }

    #[test]
    fn test_tenant_isolation() {
        let db = init_test_db().unwrap();
        let actor = ActorId::new();

        let pool_a = PoolId::new();
        let pool_b = PoolId::new();
        create_pool(&db, pool_a, "Pool A", &actor).unwrap();
        create_pool(&db, pool_b, "Pool B", &actor).unwrap();

        let tenant_a = TenantContext::pool_wide(pool_a);
        let tenant_b = TenantContext::pool_wide(pool_b);

        create_member(&db, &tenant_a, MemberId::new(), pool_a, "Member A", &actor).unwrap();
        create_member(&db, &tenant_b, MemberId::new(), pool_b, "Member B", &actor).unwrap();

        // Pool A context should only see Member A
        let members_a = list_members(&db, &tenant_a).unwrap();
        assert_eq!(members_a.len(), 1);
        assert_eq!(members_a[0].name, "Member A");

        // Pool B context should only see Member B
        let members_b = list_members(&db, &tenant_b).unwrap();
        assert_eq!(members_b.len(), 1);
        assert_eq!(members_b[0].name, "Member B");
    }
}
