//! Data quality scoring — completeness, accuracy, recency.
//!
//! Pure functions, no I/O. Operates on resolved field maps.

use crate::expr;
use crate::field_value::FieldValue;
use serde::Serialize;
use std::collections::HashMap;

// ── Completeness ────────────────────────────────────────────────────────────

/// Which fields are required vs recommended for a given asset type.
#[derive(Debug, Clone)]
pub struct CompletenessConfig {
    pub required: Vec<String>,
    pub recommended: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompletenessScore {
    pub score: f64,
    pub required_total: usize,
    pub required_populated: usize,
    pub recommended_total: usize,
    pub recommended_populated: usize,
    pub missing_required: Vec<String>,
    pub missing_recommended: Vec<String>,
}

/// Score completeness of an asset's fields.
pub fn score_completeness(
    fields: &HashMap<String, FieldValue>,
    config: &CompletenessConfig,
) -> CompletenessScore {
    let mut missing_required = Vec::new();
    let mut missing_recommended = Vec::new();
    let mut required_populated = 0;
    let mut recommended_populated = 0;

    for f in &config.required {
        if is_populated(fields.get(f)) {
            required_populated += 1;
        } else {
            missing_required.push(f.clone());
        }
    }

    for f in &config.recommended {
        if is_populated(fields.get(f)) {
            recommended_populated += 1;
        } else {
            missing_recommended.push(f.clone());
        }
    }

    let total = config.required.len() + config.recommended.len();
    let populated = required_populated + recommended_populated;
    let score = if total == 0 { 1.0 } else { populated as f64 / total as f64 };

    CompletenessScore {
        score,
        required_total: config.required.len(),
        required_populated,
        recommended_total: config.recommended.len(),
        recommended_populated,
        missing_required,
        missing_recommended,
    }
}

fn is_populated(val: Option<&FieldValue>) -> bool {
    match val {
        None => false,
        Some(FieldValue::Null) => false,
        Some(FieldValue::Text(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

// ── Accuracy ────────────────────────────────────────────────────────────────

/// A rule that validates cross-field relationships.
#[derive(Debug, Clone)]
pub struct AccuracyRule {
    pub id: String,
    pub description: String,
    /// When this rule applies (e.g., "construction_class == 'Frame'").
    /// If None, applies to all assets.
    pub condition: Option<String>,
    /// What must be true (e.g., "sprinkler != null").
    pub assertion: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AccuracyFailure {
    pub rule_id: String,
    pub description: String,
    pub assertion: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AccuracyScore {
    pub score: f64,
    pub rules_evaluated: usize,
    pub rules_passed: usize,
    pub failures: Vec<AccuracyFailure>,
}

/// Score accuracy by evaluating rules against asset fields.
pub fn score_accuracy(
    fields: &HashMap<String, FieldValue>,
    rules: &[AccuracyRule],
) -> AccuracyScore {
    let mut evaluated = 0;
    let mut passed = 0;
    let mut failures = Vec::new();

    for rule in rules {
        // Check condition — does this rule apply?
        if let Some(cond) = &rule.condition {
            match expr::parse(cond) {
                Ok(cond_expr) => {
                    match expr::eval(&cond_expr, fields) {
                        Ok(FieldValue::Bool(true)) => {} // Condition met, evaluate assertion
                        _ => continue, // Condition not met or error, skip
                    }
                }
                Err(_) => continue,
            }
        }

        evaluated += 1;

        // Evaluate assertion
        match expr::parse(&rule.assertion) {
            Ok(assert_expr) => {
                match expr::eval(&assert_expr, fields) {
                    Ok(FieldValue::Bool(true)) => { passed += 1; }
                    _ => {
                        failures.push(AccuracyFailure {
                            rule_id: rule.id.clone(),
                            description: rule.description.clone(),
                            assertion: rule.assertion.clone(),
                        });
                    }
                }
            }
            Err(_) => {
                failures.push(AccuracyFailure {
                    rule_id: rule.id.clone(),
                    description: rule.description.clone(),
                    assertion: rule.assertion.clone(),
                });
            }
        }
    }

    let score = if evaluated == 0 { 1.0 } else { passed as f64 / evaluated as f64 };
    AccuracyScore { score, rules_evaluated: evaluated, rules_passed: passed, failures }
}

// ── Recency ─────────────────────────────────────────────────────────────────

/// Configuration for which fields are tracked for recency.
#[derive(Debug, Clone)]
pub struct RecencyField {
    pub field_name: String,
    pub freshness_days: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldRecency {
    pub field_name: String,
    pub days_since_update: Option<u32>,
    pub threshold_days: u32,
    pub is_stale: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecencyScore {
    pub score: f64,
    pub tracked_fields: Vec<FieldRecency>,
}

/// Score recency given per-field last-update dates.
/// `field_ages` maps field_name -> days since last mutation.
pub fn score_recency(
    field_ages: &HashMap<String, u32>,
    config: &[RecencyField],
) -> RecencyScore {
    if config.is_empty() {
        return RecencyScore { score: 1.0, tracked_fields: vec![] };
    }

    let mut tracked = Vec::new();
    let mut fresh_count = 0;

    for rc in config {
        let days = field_ages.get(&rc.field_name).copied();
        let is_stale = match days {
            Some(d) => d > rc.freshness_days,
            None => true, // Never updated = stale
        };
        if !is_stale { fresh_count += 1; }

        tracked.push(FieldRecency {
            field_name: rc.field_name.clone(),
            days_since_update: days,
            threshold_days: rc.freshness_days,
            is_stale,
        });
    }

    let score = fresh_count as f64 / config.len() as f64;
    RecencyScore { score, tracked_fields: tracked }
}

// ── Composed Score ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct QualityScore {
    pub completeness: CompletenessScore,
    pub accuracy: AccuracyScore,
    pub recency: RecencyScore,
    pub composite: f64,
}

/// Default completeness config for buildings.
pub fn building_completeness_config() -> CompletenessConfig {
    CompletenessConfig {
        required: vec![
            "building_name".into(), "address".into(), "city".into(),
            "state".into(), "zip_code".into(), "replacement_cost".into(),
        ],
        recommended: vec![
            "year_built".into(), "construction_class".into(), "occupancy".into(),
            "sq_footage".into(), "stories".into(), "sprinkler".into(),
            "roof_type".into(), "contents_value".into(),
        ],
    }
}

/// Default completeness config for vehicles.
pub fn vehicle_completeness_config() -> CompletenessConfig {
    CompletenessConfig {
        required: vec!["building_name".into(), "replacement_cost".into()],
        recommended: vec!["address".into(), "year_built".into()],
    }
}

/// Default completeness config for contents.
pub fn contents_completeness_config() -> CompletenessConfig {
    CompletenessConfig {
        required: vec!["building_name".into(), "replacement_cost".into()],
        recommended: vec!["address".into()],
    }
}

/// The three starter accuracy rules from the ADR.
pub fn default_accuracy_rules() -> Vec<AccuracyRule> {
    vec![
        AccuracyRule {
            id: "ACC-001".into(),
            description: "Frame construction with habitational occupancy must have sprinkler data".into(),
            condition: Some("construction_class == 'Frame' && occupancy == 'habitational'".into()),
            assertion: "sprinkler != null".into(),
        },
        AccuracyRule {
            id: "ACC-002".into(),
            description: "Replacement cost over $10M requires year_built".into(),
            condition: Some("replacement_cost > 10000000".into()),
            assertion: "year_built != null".into(),
        },
        AccuracyRule {
            id: "ACC-003".into(),
            description: "Buildings must have construction class if replacement cost is set".into(),
            condition: Some("replacement_cost != null".into()),
            assertion: "construction_class != null".into(),
        },
    ]
}

/// Default recency tracking config.
pub fn default_recency_config() -> Vec<RecencyField> {
    vec![
        RecencyField { field_name: "replacement_cost".into(), freshness_days: 365 },
        RecencyField { field_name: "address".into(), freshness_days: 730 },
        RecencyField { field_name: "sq_footage".into(), freshness_days: 730 },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    fn make_fields(pairs: &[(&str, FieldValue)]) -> HashMap<String, FieldValue> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    #[test]
    fn completeness_full_building() {
        let fields = make_fields(&[
            ("building_name", FieldValue::Text("Fire Station".into())),
            ("address", FieldValue::Text("123 Main".into())),
            ("city", FieldValue::Text("Springfield".into())),
            ("state", FieldValue::Enum("IL".into())),
            ("zip_code", FieldValue::Text("62701".into())),
            ("replacement_cost", FieldValue::Money { amount: Decimal::from(1500000), currency: "USD".into() }),
            ("year_built", FieldValue::Number(Decimal::from(1985))),
            ("construction_class", FieldValue::Enum("Masonry".into())),
            ("occupancy", FieldValue::Enum("Fire Station".into())),
            ("sq_footage", FieldValue::Number(Decimal::from(8500))),
            ("stories", FieldValue::Number(Decimal::from(2))),
            ("sprinkler", FieldValue::Bool(true)),
            ("roof_type", FieldValue::Enum("Flat".into())),
            ("contents_value", FieldValue::Money { amount: Decimal::from(320000), currency: "USD".into() }),
        ]);

        let score = score_completeness(&fields, &building_completeness_config());
        assert_eq!(score.score, 1.0);
        assert!(score.missing_required.is_empty());
        assert!(score.missing_recommended.is_empty());
    }

    #[test]
    fn completeness_missing_fields() {
        let fields = make_fields(&[
            ("building_name", FieldValue::Text("Fire Station".into())),
            ("replacement_cost", FieldValue::Money { amount: Decimal::from(1500000), currency: "USD".into() }),
        ]);

        let score = score_completeness(&fields, &building_completeness_config());
        assert!(score.score < 1.0);
        assert!(score.missing_required.contains(&"address".to_string()));
        assert_eq!(score.required_populated, 2); // building_name + replacement_cost
    }

    #[test]
    fn accuracy_rules_pass() {
        let fields = make_fields(&[
            ("construction_class", FieldValue::Enum("Masonry".into())),
            ("replacement_cost", FieldValue::Money { amount: Decimal::from(5000000), currency: "USD".into() }),
            ("year_built", FieldValue::Number(Decimal::from(1985))),
        ]);

        let score = score_accuracy(&fields, &default_accuracy_rules());
        // ACC-001 condition doesn't match (Masonry, not Frame), ACC-002 condition doesn't match (<10M)
        // ACC-003 matches: replacement_cost set → construction_class must be set → passes
        assert_eq!(score.rules_evaluated, 1);
        assert_eq!(score.rules_passed, 1);
        assert_eq!(score.score, 1.0);
    }

    #[test]
    fn accuracy_rule_fails() {
        let fields = make_fields(&[
            ("replacement_cost", FieldValue::Money { amount: Decimal::from(15000000), currency: "USD".into() }),
            // Missing year_built — ACC-002 should fail
            // Missing construction_class — ACC-003 should fail
        ]);

        let score = score_accuracy(&fields, &default_accuracy_rules());
        assert!(score.score < 1.0);
        let failed_ids: Vec<&str> = score.failures.iter().map(|f| f.rule_id.as_str()).collect();
        assert!(failed_ids.contains(&"ACC-002"));
        assert!(failed_ids.contains(&"ACC-003"));
    }

    #[test]
    fn recency_fresh_fields() {
        let mut ages = HashMap::new();
        ages.insert("replacement_cost".into(), 100); // 100 days, threshold 365
        ages.insert("address".into(), 200);           // 200 days, threshold 730
        ages.insert("sq_footage".into(), 500);         // 500 days, threshold 730

        let score = score_recency(&ages, &default_recency_config());
        assert_eq!(score.score, 1.0);
        assert!(score.tracked_fields.iter().all(|f| !f.is_stale));
    }

    #[test]
    fn recency_stale_field() {
        let mut ages = HashMap::new();
        ages.insert("replacement_cost".into(), 400); // 400 > 365 = stale
        ages.insert("address".into(), 200);

        let score = score_recency(&ages, &default_recency_config());
        assert!(score.score < 1.0);
        let stale: Vec<&str> = score.tracked_fields.iter()
            .filter(|f| f.is_stale)
            .map(|f| f.field_name.as_str())
            .collect();
        assert!(stale.contains(&"replacement_cost"));
        // sq_footage is missing entirely = stale
        assert!(stale.contains(&"sq_footage"));
    }
}
