//! SOV pipeline — diff engine, validation, approval routing.
//!
//! Pure functions. The pipeline takes proposed changes and produces
//! a processing result with diffs, quality assessment, and approval decisions.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::field_value::FieldValue;
use crate::ids::AssetId;

/// Fields that are classified as valuation fields.
/// Changes to these ALWAYS require approval, regardless of auto_approve.
const VALUATION_FIELDS: &[&str] = &[
    "replacement_cost",
    "contents_value",
    "actual_cash_value",
    "functional_replacement_cost",
];

/// Source discriminator — what triggered this SOV processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceKind {
    Renewal,
    InlineEdit,
    Onboarding,
    BulkImport,
}

/// A single field-level change detected by the diff engine.
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

/// The result of processing changes through the SOV pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SOVProcessingResult {
    pub diffs: Vec<AssetDiff>,
    pub source: SourceKind,
    pub errors: Vec<String>,
}

/// What approval state a mutation should get.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalDecision {
    /// Auto-approved — takes effect immediately.
    AutoApprove,
    /// Requires admin review.
    Pending,
}

/// Determine approval decision for a field change.
///
/// Rules (per ADR):
/// - New asset creation → always Pending
/// - Valuation field changes → always Pending
/// - Other changes with auto_approve=true → AutoApprove
/// - Other changes with auto_approve=false → Pending
pub fn decide_approval(
    change_type: ChangeType,
    field_name: &str,
    auto_approve: bool,
) -> ApprovalDecision {
    // New assets always pend
    if change_type == ChangeType::New {
        return ApprovalDecision::Pending;
    }

    // Valuation changes always pend
    if is_valuation_field(field_name) {
        return ApprovalDecision::Pending;
    }

    // Other changes depend on auto_approve
    if auto_approve {
        ApprovalDecision::AutoApprove
    } else {
        ApprovalDecision::Pending
    }
}

/// Check if a field is a valuation field.
pub fn is_valuation_field(field_name: &str) -> bool {
    VALUATION_FIELDS.contains(&field_name)
}

/// Compute the diff between current fields and proposed changes.
pub fn compute_diff(
    asset_id: AssetId,
    current_fields: &HashMap<String, FieldValue>,
    proposed_changes: &HashMap<String, FieldValue>,
    change_type: ChangeType,
) -> AssetDiff {
    let mut field_changes = Vec::new();
    let mut has_valuation_change = false;

    for (field_name, proposed) in proposed_changes {
        let previous = current_fields.get(field_name).cloned();
        let is_val = is_valuation_field(field_name);

        // Only include if the value actually changed
        let changed = match &previous {
            Some(prev) => prev != proposed,
            None => true,
        };

        if changed {
            if is_val { has_valuation_change = true; }
            field_changes.push(FieldChange {
                field_name: field_name.clone(),
                previous_value: previous,
                proposed_value: proposed.clone(),
                is_valuation_field: is_val,
            });
        }
    }

    AssetDiff {
        asset_id,
        change_type,
        field_changes,
        has_valuation_change,
    }
}

/// Validate proposed field values. Returns a list of errors.
pub fn validate_fields(fields: &HashMap<String, FieldValue>) -> Vec<String> {
    let mut errors = Vec::new();

    // Replacement cost must be positive
    if let Some(FieldValue::Money { amount, .. }) = fields.get("replacement_cost") {
        if amount.is_sign_negative() {
            errors.push("Replacement cost cannot be negative".into());
        }
    }

    // Year built must be reasonable
    if let Some(FieldValue::Number(n)) = fields.get("year_built") {
        let year: i64 = n.to_string().parse().unwrap_or(0);
        if year < 1800 || year > 2030 {
            errors.push(format!("Year built {} is outside valid range (1800-2030)", year));
        }
    }

    // Sq footage must be positive
    if let Some(FieldValue::Number(n)) = fields.get("sq_footage") {
        if n.is_sign_negative() {
            errors.push("Square footage cannot be negative".into());
        }
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    fn fv_money(amount: i64) -> FieldValue {
        FieldValue::Money { amount: Decimal::from(amount), currency: "USD".into() }
    }

    #[test]
    fn new_assets_always_pend() {
        assert_eq!(decide_approval(ChangeType::New, "address", true), ApprovalDecision::Pending);
        assert_eq!(decide_approval(ChangeType::New, "address", false), ApprovalDecision::Pending);
    }

    #[test]
    fn valuation_changes_always_pend() {
        assert_eq!(decide_approval(ChangeType::Modified, "replacement_cost", true), ApprovalDecision::Pending);
        assert_eq!(decide_approval(ChangeType::Modified, "contents_value", true), ApprovalDecision::Pending);
    }

    #[test]
    fn non_valuation_with_auto_approve() {
        assert_eq!(decide_approval(ChangeType::Modified, "address", true), ApprovalDecision::AutoApprove);
        assert_eq!(decide_approval(ChangeType::Modified, "city", true), ApprovalDecision::AutoApprove);
    }

    #[test]
    fn non_valuation_without_auto_approve() {
        assert_eq!(decide_approval(ChangeType::Modified, "address", false), ApprovalDecision::Pending);
    }

    #[test]
    fn diff_detects_changes() {
        let mut current = HashMap::new();
        current.insert("address".into(), FieldValue::Text("123 Main".into()));
        current.insert("replacement_cost".into(), fv_money(1000000));

        let mut proposed = HashMap::new();
        proposed.insert("address".into(), FieldValue::Text("456 Oak".into()));
        proposed.insert("replacement_cost".into(), fv_money(1500000));

        let diff = compute_diff(AssetId::new(), &current, &proposed, ChangeType::Modified);
        assert_eq!(diff.field_changes.len(), 2);
        assert!(diff.has_valuation_change);
    }

    #[test]
    fn diff_excludes_unchanged() {
        let mut current = HashMap::new();
        current.insert("address".into(), FieldValue::Text("123 Main".into()));

        let mut proposed = HashMap::new();
        proposed.insert("address".into(), FieldValue::Text("123 Main".into()));

        let diff = compute_diff(AssetId::new(), &current, &proposed, ChangeType::Modified);
        assert!(diff.field_changes.is_empty());
    }

    #[test]
    fn validation_catches_negative_cost() {
        let mut fields = HashMap::new();
        fields.insert("replacement_cost".into(), FieldValue::Money {
            amount: Decimal::from(-500),
            currency: "USD".into(),
        });
        let errors = validate_fields(&fields);
        assert!(!errors.is_empty());
    }

    #[test]
    fn validation_catches_bad_year() {
        let mut fields = HashMap::new();
        fields.insert("year_built".into(), FieldValue::Number(Decimal::from(1500)));
        let errors = validate_fields(&fields);
        assert!(!errors.is_empty());
    }
}
