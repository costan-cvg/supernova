//! Audit trail types — every write produces an AuditEntry.
//! Stub for Inc 1 implementation.

use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use crate::field_value::FieldValue;
use crate::ids::{ActorId, PoolId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Operation {
    Create,
    Update,
    Archive,
    Restore,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub entry_id: Uuid,
    pub entity_id: Uuid,
    pub entity_type: String,
    pub field_name: Option<String>,
    pub old_value: Option<FieldValue>,
    pub new_value: Option<FieldValue>,
    pub effective_date: Date,
    pub actor_id: ActorId,
    pub actor_role: String,
    pub pool_id: PoolId,
    pub timestamp: OffsetDateTime,
    pub operation: Operation,
}
