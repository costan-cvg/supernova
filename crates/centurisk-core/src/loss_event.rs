//! Loss event types — records of loss incidents associated with assets.

use serde::{Deserialize, Serialize};

/// A recorded loss event for an asset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LossEvent {
    /// The asset this loss event is associated with.
    pub asset_id: String,
    /// Type of event: "fire", "flood", "wind", "theft", "other".
    pub event_type: String,
    /// Date of the event (ISO 8601 date string).
    pub date: String,
    /// Severity: "minor", "moderate", "major", "catastrophic".
    pub severity: String,
    /// Free-text description of the event.
    pub description: String,
}

/// Valid event types for loss events.
const VALID_EVENT_TYPES: &[&str] = &["fire", "flood", "wind", "theft", "other"];

/// Valid severity levels for loss events.
const VALID_SEVERITIES: &[&str] = &["minor", "moderate", "major", "catastrophic"];

/// Validation error for loss event input.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationError(pub String);

/// Validate a loss event's fields.
pub fn validate_loss_event(event: &LossEvent) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    if event.asset_id.is_empty() {
        errors.push(ValidationError("asset_id is required".into()));
    }

    if !VALID_EVENT_TYPES.contains(&event.event_type.as_str()) {
        errors.push(ValidationError(format!(
            "event_type must be one of: {}",
            VALID_EVENT_TYPES.join(", ")
        )));
    }

    if event.date.is_empty() {
        errors.push(ValidationError("date is required".into()));
    }

    if !VALID_SEVERITIES.contains(&event.severity.as_str()) {
        errors.push(ValidationError(format!(
            "severity must be one of: {}",
            VALID_SEVERITIES.join(", ")
        )));
    }

    if event.description.is_empty() {
        errors.push(ValidationError("description is required".into()));
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_loss_event_passes() {
        let event = LossEvent {
            asset_id: "asset-123".into(),
            event_type: "fire".into(),
            date: "2025-06-15".into(),
            severity: "major".into(),
            description: "Electrical fire in server room".into(),
        };
        let errors = validate_loss_event(&event);
        assert!(errors.is_empty(), "Valid event should have no errors");
    }

    #[test]
    fn invalid_event_type_rejected() {
        let event = LossEvent {
            asset_id: "asset-123".into(),
            event_type: "earthquake".into(),
            date: "2025-06-15".into(),
            severity: "major".into(),
            description: "Earthquake damage".into(),
        };
        let errors = validate_loss_event(&event);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].0.contains("event_type"));
    }

    #[test]
    fn invalid_severity_rejected() {
        let event = LossEvent {
            asset_id: "asset-123".into(),
            event_type: "fire".into(),
            date: "2025-06-15".into(),
            severity: "extreme".into(),
            description: "Fire damage".into(),
        };
        let errors = validate_loss_event(&event);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].0.contains("severity"));
    }

    #[test]
    fn empty_fields_rejected() {
        let event = LossEvent {
            asset_id: "".into(),
            event_type: "invalid".into(),
            date: "".into(),
            severity: "invalid".into(),
            description: "".into(),
        };
        let errors = validate_loss_event(&event);
        assert_eq!(errors.len(), 5, "Should catch asset_id, event_type, date, severity, description");
    }

    #[test]
    fn all_valid_event_types() {
        for et in &["fire", "flood", "wind", "theft", "other"] {
            let event = LossEvent {
                asset_id: "a1".into(),
                event_type: et.to_string(),
                date: "2025-01-01".into(),
                severity: "minor".into(),
                description: "test".into(),
            };
            assert!(validate_loss_event(&event).is_empty(), "Event type '{}' should be valid", et);
        }
    }

    #[test]
    fn all_valid_severities() {
        for sev in &["minor", "moderate", "major", "catastrophic"] {
            let event = LossEvent {
                asset_id: "a1".into(),
                event_type: "fire".into(),
                date: "2025-01-01".into(),
                severity: sev.to_string(),
                description: "test".into(),
            };
            assert!(validate_loss_event(&event).is_empty(), "Severity '{}' should be valid", sev);
        }
    }
}
