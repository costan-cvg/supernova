//! Recommendation engine — pure functions that produce risk improvement suggestions
//! based on asset field data and quality scores.

use crate::field_value::FieldValue;
use crate::quality::QualityScore;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Priority level for a recommendation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Priority {
    High,
    Moderate,
    Low,
}

/// A recommendation for improving an asset's risk profile or data quality.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    /// None = portfolio-level recommendation.
    pub asset_id: Option<String>,
    /// Category of the recommendation (e.g., "fire_protection", "valuation", "data_quality").
    pub category: String,
    /// Priority of the recommendation.
    pub priority: Priority,
    /// Human-readable directive.
    pub action: String,
    /// Why this recommendation was generated.
    pub rationale: String,
}

/// Threshold for replacement cost that triggers sprinkler requirement.
const SPRINKLER_COST_THRESHOLD: i64 = 5_000_000;

/// Threshold for completeness score that triggers data quality recommendation.
const COMPLETENESS_THRESHOLD: f64 = 0.6;

/// Generate recommendations for an asset based on its fields and quality score.
///
/// This is a pure function — no I/O, no database access.
pub fn recommend(
    fields: &HashMap<String, FieldValue>,
    quality: &QualityScore,
) -> Vec<Recommendation> {
    let mut recs = Vec::new();

    // Rule 1: Missing sprinkler on buildings > $5M replacement cost
    check_sprinkler_rule(fields, &mut recs);

    // Rule 2: Completeness score < 0.6
    check_completeness_rule(quality, &mut recs);

    // Rule 3: Stale replacement_cost
    check_stale_valuation_rule(quality, &mut recs);

    recs
}

/// Rule 1: Buildings with replacement cost > $5M should have sprinkler data.
fn check_sprinkler_rule(
    fields: &HashMap<String, FieldValue>,
    recs: &mut Vec<Recommendation>,
) {
    let threshold = Decimal::from(SPRINKLER_COST_THRESHOLD);

    let has_high_value = match fields.get("replacement_cost") {
        Some(FieldValue::Money { amount, .. }) => *amount > threshold,
        Some(FieldValue::Number(n)) => *n > threshold,
        _ => false,
    };

    if !has_high_value {
        return;
    }

    let has_sprinkler = match fields.get("sprinkler") {
        Some(FieldValue::Bool(_)) => true,
        Some(FieldValue::Enum(s)) if !s.is_empty() => true,
        Some(FieldValue::Text(s)) if !s.is_empty() => true,
        _ => false,
    };

    if !has_sprinkler {
        recs.push(Recommendation {
            asset_id: None,
            category: "fire_protection".into(),
            priority: Priority::High,
            action: "Add sprinkler system data for this high-value property".into(),
            rationale: format!(
                "Building has replacement cost exceeding ${} but no sprinkler information on file",
                SPRINKLER_COST_THRESHOLD / 1_000_000
            ),
        });
    }
}

/// Rule 2: Completeness score below threshold triggers data quality recommendation.
fn check_completeness_rule(
    quality: &QualityScore,
    recs: &mut Vec<Recommendation>,
) {
    if quality.completeness.score < COMPLETENESS_THRESHOLD {
        let missing: Vec<&str> = quality
            .completeness
            .missing_required
            .iter()
            .map(|s| s.as_str())
            .collect();

        let missing_list = if missing.is_empty() {
            "recommended fields".into()
        } else {
            missing.join(", ")
        };

        recs.push(Recommendation {
            asset_id: None,
            category: "data_quality".into(),
            priority: Priority::Moderate,
            action: format!("Complete missing fields: {}", missing_list),
            rationale: format!(
                "Data completeness score is {:.0}%, below the {:.0}% threshold",
                quality.completeness.score * 100.0,
                COMPLETENESS_THRESHOLD * 100.0
            ),
        });
    }
}

/// Rule 3: Stale replacement_cost triggers valuation update recommendation.
fn check_stale_valuation_rule(
    quality: &QualityScore,
    recs: &mut Vec<Recommendation>,
) {
    for field_recency in &quality.recency.tracked_fields {
        if field_recency.field_name == "replacement_cost" && field_recency.is_stale {
            let days_info = match field_recency.days_since_update {
                Some(days) => format!("{} days since last update", days),
                None => "never updated".into(),
            };

            recs.push(Recommendation {
                asset_id: None,
                category: "valuation".into(),
                priority: Priority::Moderate,
                action: "Update replacement cost valuation".into(),
                rationale: format!(
                    "Replacement cost is stale ({}, threshold is {} days)",
                    days_info, field_recency.threshold_days
                ),
            });
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quality::*;
    use rust_decimal::Decimal;

    fn make_fields(pairs: &[(&str, FieldValue)]) -> HashMap<String, FieldValue> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    fn make_quality(
        completeness_score: f64,
        missing_required: Vec<String>,
        replacement_cost_stale: bool,
        replacement_cost_days: Option<u32>,
    ) -> QualityScore {
        QualityScore {
            completeness: CompletenessScore {
                score: completeness_score,
                required_total: 6,
                required_populated: (completeness_score * 6.0) as usize,
                recommended_total: 8,
                recommended_populated: (completeness_score * 8.0) as usize,
                missing_required,
                missing_recommended: vec![],
            },
            accuracy: AccuracyScore {
                score: 1.0,
                rules_evaluated: 0,
                rules_passed: 0,
                failures: vec![],
            },
            recency: RecencyScore {
                score: if replacement_cost_stale { 0.5 } else { 1.0 },
                tracked_fields: vec![FieldRecency {
                    field_name: "replacement_cost".into(),
                    days_since_update: replacement_cost_days,
                    threshold_days: 365,
                    is_stale: replacement_cost_stale,
                }],
            },
            composite: 0.8,
        }
    }

    #[test]
    fn rule_sprinkler_high_value_no_sprinkler() {
        let fields = make_fields(&[
            (
                "replacement_cost",
                FieldValue::Money {
                    amount: Decimal::from(6_000_000),
                    currency: "USD".into(),
                },
            ),
            ("building_name", FieldValue::Text("HQ".into())),
        ]);
        let quality = make_quality(0.8, vec![], false, Some(100));

        let recs = recommend(&fields, &quality);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].category, "fire_protection");
        assert_eq!(recs[0].priority, Priority::High);
    }

    #[test]
    fn rule_sprinkler_high_value_has_sprinkler() {
        let fields = make_fields(&[
            (
                "replacement_cost",
                FieldValue::Money {
                    amount: Decimal::from(6_000_000),
                    currency: "USD".into(),
                },
            ),
            ("sprinkler", FieldValue::Bool(true)),
        ]);
        let quality = make_quality(0.8, vec![], false, Some(100));

        let recs = recommend(&fields, &quality);
        assert!(
            recs.iter().all(|r| r.category != "fire_protection"),
            "Should not recommend sprinkler when already present"
        );
    }

    #[test]
    fn rule_sprinkler_low_value_no_sprinkler() {
        let fields = make_fields(&[(
            "replacement_cost",
            FieldValue::Money {
                amount: Decimal::from(1_000_000),
                currency: "USD".into(),
            },
        )]);
        let quality = make_quality(0.8, vec![], false, Some(100));

        let recs = recommend(&fields, &quality);
        assert!(
            recs.iter().all(|r| r.category != "fire_protection"),
            "Should not trigger sprinkler rule for low-value buildings"
        );
    }

    #[test]
    fn rule_low_completeness() {
        let fields = make_fields(&[(
            "replacement_cost",
            FieldValue::Money {
                amount: Decimal::from(1_000_000),
                currency: "USD".into(),
            },
        )]);
        let quality = make_quality(
            0.4,
            vec!["address".into(), "city".into(), "state".into()],
            false,
            Some(100),
        );

        let recs = recommend(&fields, &quality);
        let data_quality_recs: Vec<&Recommendation> =
            recs.iter().filter(|r| r.category == "data_quality").collect();
        assert_eq!(data_quality_recs.len(), 1);
        assert_eq!(data_quality_recs[0].priority, Priority::Moderate);
        assert!(data_quality_recs[0].action.contains("address"));
    }

    #[test]
    fn rule_good_completeness_no_recommendation() {
        let fields = make_fields(&[(
            "replacement_cost",
            FieldValue::Money {
                amount: Decimal::from(1_000_000),
                currency: "USD".into(),
            },
        )]);
        let quality = make_quality(0.8, vec![], false, Some(100));

        let recs = recommend(&fields, &quality);
        assert!(
            recs.iter().all(|r| r.category != "data_quality"),
            "Should not recommend data quality improvements when completeness is good"
        );
    }

    #[test]
    fn rule_stale_replacement_cost() {
        let fields = make_fields(&[(
            "replacement_cost",
            FieldValue::Money {
                amount: Decimal::from(1_000_000),
                currency: "USD".into(),
            },
        )]);
        let quality = make_quality(0.8, vec![], true, Some(400));

        let recs = recommend(&fields, &quality);
        let valuation_recs: Vec<&Recommendation> =
            recs.iter().filter(|r| r.category == "valuation").collect();
        assert_eq!(valuation_recs.len(), 1);
        assert_eq!(valuation_recs[0].priority, Priority::Moderate);
        assert!(valuation_recs[0].rationale.contains("400 days"));
    }

    #[test]
    fn rule_fresh_replacement_cost_no_recommendation() {
        let fields = make_fields(&[(
            "replacement_cost",
            FieldValue::Money {
                amount: Decimal::from(1_000_000),
                currency: "USD".into(),
            },
        )]);
        let quality = make_quality(0.8, vec![], false, Some(100));

        let recs = recommend(&fields, &quality);
        assert!(
            recs.iter().all(|r| r.category != "valuation"),
            "Should not recommend valuation update when replacement_cost is fresh"
        );
    }

    #[test]
    fn multiple_rules_fire_together() {
        // High-value, no sprinkler, low completeness, stale valuation
        let fields = make_fields(&[(
            "replacement_cost",
            FieldValue::Money {
                amount: Decimal::from(10_000_000),
                currency: "USD".into(),
            },
        )]);
        let quality = make_quality(
            0.3,
            vec!["address".into(), "city".into()],
            true,
            Some(500),
        );

        let recs = recommend(&fields, &quality);
        assert_eq!(recs.len(), 3, "All three rules should fire");

        let categories: Vec<&str> = recs.iter().map(|r| r.category.as_str()).collect();
        assert!(categories.contains(&"fire_protection"));
        assert!(categories.contains(&"data_quality"));
        assert!(categories.contains(&"valuation"));
    }
}
