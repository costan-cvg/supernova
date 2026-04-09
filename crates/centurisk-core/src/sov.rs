//! SOV pipeline types — processing result, diff summary, field changes.
//! Stub for Inc 4 implementation.

use serde::{Deserialize, Serialize};

use crate::field_value::FieldValue;
use crate::ids::AssetId;

/// Source discriminator — what triggered this SOV processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceKind {
    Renewal,
    InlineEdit,
    Onboarding,
    BulkImport,
}

/// A single field-level change detected by the diff engine.
/// Uses typed FieldValue (P0 fix — no untyped Any).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldChange {
    pub field_name: String,
    pub previous_value: Option<FieldValue>,
    pub proposed_value: FieldValue,
    pub is_valuation_field: bool,
}

/// Diff for a single asset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetDiff {
    pub asset_id: AssetId,
    pub change_type: ChangeType,
    pub field_changes: Vec<FieldChange>,
    pub has_valuation_change: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeType {
    New,
    Modified,
    Deactivated,
}
