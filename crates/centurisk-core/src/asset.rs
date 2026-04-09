//! Asset domain types — identity, lifecycle, hierarchy, mutations.
//! Stubs for Inc 2 implementation.

use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};

use crate::field_value::FieldValue;
use crate::ids::{ActorId, AssetId, MutationId, PoolId};

/// Asset lifecycle state machine.
/// Draft -> Active -> PendingChange -> Active (approved) or Active (rejected, reverts)
/// Active -> Archived, Archived -> Active (restore, rare)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecycleState {
    Draft,
    Active,
    PendingChange,
    Archived,
}

/// Asset type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
/// Known exposure types. The system is extensible — pools can define
/// additional types (K9, Watercraft, etc.) stored as strings in the DB.
/// Core logic matches on known variants; unknown types get default handling.
pub enum AssetType {
    Building,
    PropertyInTheOpen,
    MovableEquipment,
    LicensedVehicle,
    FineArts,
}

impl AssetType {
    /// Parse a string into a known AssetType, or None for pool-defined types.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "Building" => Some(Self::Building),
            "PropertyInTheOpen" => Some(Self::PropertyInTheOpen),
            "MovableEquipment" => Some(Self::MovableEquipment),
            "LicensedVehicle" => Some(Self::LicensedVehicle),
            "FineArts" => Some(Self::FineArts),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Building => "Building",
            Self::PropertyInTheOpen => "PropertyInTheOpen",
            Self::MovableEquipment => "MovableEquipment",
            Self::LicensedVehicle => "LicensedVehicle",
            Self::FineArts => "FineArts",
        }
    }
}

/// Approval state for a field mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalState {
    Pending,
    Approved,
    Rejected,
}

/// Materialized path for hierarchy placement.
/// e.g., "/pool-123/member-456/campus-789/building-012"
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterializedPath(pub String);

impl MaterializedPath {
    pub fn new(path: &str) -> Self {
        Self(path.to_string())
    }

    /// Returns all ancestor paths (excluding self).
    pub fn ancestors(&self) -> Vec<MaterializedPath> {
        let parts: Vec<&str> = self.0.trim_matches('/').split('/').collect();
        let mut result = Vec::new();
        let mut current = String::new();
        for part in &parts[..parts.len().saturating_sub(1)] {
            current.push('/');
            current.push_str(part);
            result.push(MaterializedPath(current.clone()));
        }
        result
    }

    /// Check if this path is a descendant of another.
    pub fn is_descendant_of(&self, ancestor: &MaterializedPath) -> bool {
        self.0.starts_with(&ancestor.0) && self.0.len() > ancestor.0.len()
    }

    /// Depth in the hierarchy (number of segments).
    pub fn depth(&self) -> usize {
        self.0.trim_matches('/').split('/').count()
    }

    /// SQL LIKE pattern for prefix queries: "/pool-123/member-456/%"
    pub fn prefix_pattern(&self) -> String {
        let trimmed = self.0.trim_end_matches('/');
        format!("{trimmed}/%")
    }
}

/// Core identity of an asset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetIdentity {
    pub asset_id: AssetId,
    pub pool_id: PoolId,
    pub path: MaterializedPath,
    pub asset_type: AssetType,
    pub lifecycle: LifecycleState,
    pub created_at: OffsetDateTime,
    pub created_by: ActorId,
}

/// A single field-level mutation (the storage primitive — no direct overwrites).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldMutation {
    pub mutation_id: MutationId,
    pub asset_id: AssetId,
    pub field_name: String,
    pub value: FieldValue,
    pub effective_date: Date,
    pub submitted_at: OffsetDateTime,
    pub submitted_by: ActorId,
    pub approved_at: Option<OffsetDateTime>,
    pub approved_by: Option<ActorId>,
    pub approval_state: ApprovalState,
}

/// A resolved field value at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedFieldValue {
    pub value: FieldValue,
    pub effective_date: Date,
    pub approval_state: ApprovalState,
    pub source_mutation: MutationId,
}

/// The resolved state of an asset at a specific point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedAssetState {
    pub asset_id: AssetId,
    pub as_of_date: Date,
    pub includes_pending: bool,
    pub fields: std::collections::HashMap<String, ResolvedFieldValue>,
    pub lifecycle: LifecycleState,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materialized_path_ancestors() {
        let path = MaterializedPath::new("/pool/member/campus/building");
        let ancestors = path.ancestors();
        assert_eq!(ancestors.len(), 3);
        assert_eq!(ancestors[0].0, "/pool");
        assert_eq!(ancestors[1].0, "/pool/member");
        assert_eq!(ancestors[2].0, "/pool/member/campus");
    }

    #[test]
    fn materialized_path_descendant() {
        let parent = MaterializedPath::new("/pool/member");
        let child = MaterializedPath::new("/pool/member/campus/building");
        assert!(child.is_descendant_of(&parent));
        assert!(!parent.is_descendant_of(&child));
        assert!(!parent.is_descendant_of(&parent));
    }

    #[test]
    fn materialized_path_depth() {
        assert_eq!(MaterializedPath::new("/pool").depth(), 1);
        assert_eq!(MaterializedPath::new("/pool/member/campus").depth(), 3);
    }

    #[test]
    fn prefix_pattern() {
        let path = MaterializedPath::new("/pool/member");
        assert_eq!(path.prefix_pattern(), "/pool/member/%");
    }
}
