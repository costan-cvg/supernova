//! FieldValue — typed discriminated union for all field values in the system.
//!
//! P0 fix: replaces untyped `Any` at the SOV pipeline boundary and throughout
//! the asset registry. Seven variants cover all CentuRisk domain values.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::Date;

/// Typed discriminated union for all field values stored in the mutation log,
/// transmitted across the SOV pipeline boundary, and rendered in the UI.
///
/// Every field in the asset registry stores its value as a `FieldValue`.
/// This ensures type safety at serialization boundaries and prevents the
/// untyped `Any` that the systems design review flagged as P0.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum FieldValue {
    /// Free-form text (addresses, names, descriptions).
    Text(String),

    /// Numeric value with financial precision (square footage, stories, year built).
    Number(Decimal),

    /// Calendar date without time (appraisal dates, effective dates).
    Date(Date),

    /// Boolean flag (sprinkler present, ADA compliant).
    Bool(bool),

    /// Constrained string from a controlled vocabulary.
    /// Variant name validated at the adapter layer against the field definition.
    Enum(String),

    /// Monetary amount with ISO 4217 currency code.
    /// Separate from Number to preserve currency semantics.
    Money {
        amount: Decimal,
        currency: String,
    },

    /// Explicit absence — distinguishes "not set" from "set to empty string".
    Null,
}

impl FieldValue {
    /// Returns the type discriminator as a static string for logging and error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            FieldValue::Text(_) => "Text",
            FieldValue::Number(_) => "Number",
            FieldValue::Date(_) => "Date",
            FieldValue::Bool(_) => "Bool",
            FieldValue::Enum(_) => "Enum",
            FieldValue::Money { .. } => "Money",
            FieldValue::Null => "Null",
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, FieldValue::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use rust_decimal::Decimal;
    use time::Date;
    use time::Month;

    #[test]
    fn serde_roundtrip_all_variants() {
        let values = vec![
            FieldValue::Text("123 Main St".into()),
            FieldValue::Number(Decimal::from_str("42.5").unwrap()),
            FieldValue::Date(Date::from_calendar_date(2025, Month::January, 15).unwrap()),
            FieldValue::Bool(true),
            FieldValue::Enum("frame".into()),
            FieldValue::Money {
                amount: Decimal::from_str("1500000").unwrap(),
                currency: "USD".into(),
            },
            FieldValue::Null,
        ];

        for val in &values {
            let json = serde_json::to_string(val).unwrap();
            let back: FieldValue = serde_json::from_str(&json).unwrap();
            assert_eq!(*val, back, "roundtrip failed for {}", val.type_name());
        }
    }

    #[test]
    fn tagged_json_format() {
        let val = FieldValue::Money {
            amount: Decimal::from(1000),
            currency: "USD".into(),
        };
        let json = serde_json::to_value(&val).unwrap();
        assert_eq!(json["type"], "Money");
        assert!(json["value"].is_object());
    }

    #[test]
    fn null_is_null() {
        assert!(FieldValue::Null.is_null());
        assert!(!FieldValue::Bool(false).is_null());
    }
}
