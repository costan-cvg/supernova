//! Natural language query translation — rule-based, no LLM.
//!
//! Translates human queries like "buildings over $5M in Springfield" into
//! structured filters. Stays within system boundary per ADR requirement.

use serde::Serialize;

/// A structured query derived from natural language input.
#[derive(Debug, Clone, Serialize)]
pub struct NlQuery {
    /// Text to search via FTS5
    pub search_text: Option<String>,
    /// Asset type filter
    pub asset_type: Option<String>,
    /// Numeric filters: field_name -> (operator, value)
    pub numeric_filters: Vec<NumericFilter>,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,
    /// Alternative phrasings when confidence is low
    pub suggestions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NumericFilter {
    pub field: String,
    pub op: String, // ">", "<", ">=", "<=", "="
    pub value: f64,
}

// Synonym map (documented here, implemented in match_asset_type and find_field_context):
// Asset types: building/bldg → Building, vehicle/truck → Vehicle, contents/equipment → Contents
// Fields: cost/value/tiv → replacement_cost, year/built → year_built, sqft/area → sq_footage

/// Translate a natural language query into a structured NlQuery.
pub fn translate_query(input: &str) -> NlQuery {
    let input_lower = input.to_lowercase();
    let tokens: Vec<&str> = input_lower.split_whitespace().collect();

    let mut search_text_parts = Vec::new();
    let mut asset_type = None;
    let mut numeric_filters = Vec::new();
    let mut confidence = 0.8; // Start with decent confidence

    let mut i = 0;
    while i < tokens.len() {
        let token = tokens[i];

        // Check for asset type synonyms
        if let Some(atype) = match_asset_type(token) {
            asset_type = Some(atype);
            i += 1;
            continue;
        }

        // Check for "over/above/more than $X" or "under/below/less than $X"
        if matches!(token, "over" | "above" | "more" | "greater" | "exceeding") {
            if let Some((value, skip)) = parse_money_value(&tokens, i + 1) {
                // Look for field context before this
                let field = find_field_context(&tokens, i);
                numeric_filters.push(NumericFilter {
                    field: field.unwrap_or("replacement_cost".into()),
                    op: ">".into(),
                    value,
                });
                i += 1 + skip;
                continue;
            }
        }

        if matches!(token, "under" | "below" | "less" | "cheaper") {
            if let Some((value, skip)) = parse_money_value(&tokens, i + 1) {
                let field = find_field_context(&tokens, i);
                numeric_filters.push(NumericFilter {
                    field: field.unwrap_or("replacement_cost".into()),
                    op: "<".into(),
                    value,
                });
                i += 1 + skip;
                continue;
            }
        }

        // Check for "$X" standalone (interpret as "over $X" by default)
        if token.starts_with('$') {
            if let Some(value) = parse_dollar_amount(token) {
                numeric_filters.push(NumericFilter {
                    field: "replacement_cost".into(),
                    op: ">".into(),
                    value,
                });
                i += 1;
                continue;
            }
        }

        // Skip noise words
        if matches!(token, "in" | "at" | "the" | "a" | "an" | "with" | "than" | "and" | "or" | "of" | "for") {
            i += 1;
            continue;
        }

        // Everything else is search text
        search_text_parts.push(token);
        i += 1;
    }

    // Lower confidence if we didn't understand much
    if search_text_parts.is_empty() && asset_type.is_none() && numeric_filters.is_empty() {
        confidence = 0.0;
    } else if search_text_parts.len() > 5 {
        confidence = 0.5; // Long query = probably didn't parse well
    }

    let search_text = if search_text_parts.is_empty() {
        None
    } else {
        Some(search_text_parts.join(" "))
    };

    let suggestions = if confidence < 0.6 {
        vec![
            "Try: buildings over $5M".into(),
            "Try: fire station in Springfield".into(),
            "Try: vehicles".into(),
        ]
    } else {
        vec![]
    };

    NlQuery {
        search_text,
        asset_type,
        numeric_filters,
        confidence,
        suggestions,
    }
}

fn match_asset_type(token: &str) -> Option<String> {
    match token {
        "building" | "buildings" | "bldg" => Some("Building".into()),
        "vehicle" | "vehicles" | "truck" | "trucks" => Some("LicensedVehicle".into()),
        "equipment" | "mower" | "mowers" | "cart" | "carts" | "drone" | "drones" => Some("MovableEquipment".into()),
        "pito" | "land" | "infrastructure" | "walkway" | "parking" | "fence" | "fencing" => Some("PropertyInTheOpen".into()),
        "art" | "arts" | "fine-arts" => Some("FineArts".into()),
        _ => None,
    }
}

fn parse_money_value(tokens: &[&str], start: usize) -> Option<(f64, usize)> {
    if start >= tokens.len() { return None; }

    // Skip "than" if present
    let mut idx = start;
    if idx < tokens.len() && tokens[idx] == "than" { idx += 1; }
    if idx >= tokens.len() { return None; }

    parse_dollar_amount(tokens[idx]).map(|v| (v, idx - start + 1))
}

fn parse_dollar_amount(token: &str) -> Option<f64> {
    let clean = token.trim_start_matches('$').replace(',', "").replace('k', "000").replace('m', "000000").replace('M', "000000");
    clean.parse::<f64>().ok()
}

fn find_field_context(tokens: &[&str], before_idx: usize) -> Option<String> {
    // Look at the 1-2 tokens before the operator for field hints
    for i in (0..before_idx).rev().take(2) {
        match tokens[i] {
            "cost" | "value" | "replacement" | "tiv" => return Some("replacement_cost".into()),
            "year" | "built" => return Some("year_built".into()),
            "sqft" | "footage" | "area" => return Some("sq_footage".into()),
            _ => {}
        }
    }
    None // Default to replacement_cost (handled by caller)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_text_search() {
        let q = translate_query("fire station");
        assert_eq!(q.search_text, Some("fire station".into()));
        assert!(q.asset_type.is_none());
        assert!(q.numeric_filters.is_empty());
    }

    #[test]
    fn asset_type_extraction() {
        let q = translate_query("buildings in Springfield");
        assert_eq!(q.asset_type, Some("Building".into()));
        assert_eq!(q.search_text, Some("springfield".into()));
    }

    #[test]
    fn money_filter_over() {
        let q = translate_query("buildings over $5M");
        assert_eq!(q.asset_type, Some("Building".into()));
        assert_eq!(q.numeric_filters.len(), 1);
        assert_eq!(q.numeric_filters[0].field, "replacement_cost");
        assert_eq!(q.numeric_filters[0].op, ">");
        assert_eq!(q.numeric_filters[0].value, 5_000_000.0);
    }

    #[test]
    fn money_filter_under() {
        let q = translate_query("vehicles under $100k");
        assert_eq!(q.asset_type, Some("LicensedVehicle".into()));
        assert_eq!(q.numeric_filters[0].op, "<");
        assert_eq!(q.numeric_filters[0].value, 100_000.0);
    }

    #[test]
    fn combined_query() {
        let q = translate_query("buildings over $5M in Springfield");
        assert_eq!(q.asset_type, Some("Building".into()));
        assert_eq!(q.search_text, Some("springfield".into()));
        assert_eq!(q.numeric_filters.len(), 1);
        assert!(q.confidence >= 0.7);
    }

    #[test]
    fn dollar_standalone() {
        let q = translate_query("$10M");
        assert_eq!(q.numeric_filters.len(), 1);
        assert_eq!(q.numeric_filters[0].value, 10_000_000.0);
    }

    #[test]
    fn empty_query_low_confidence() {
        let q = translate_query("");
        assert_eq!(q.confidence, 0.0);
    }

    #[test]
    fn noise_words_stripped() {
        let q = translate_query("the building in the city");
        assert_eq!(q.asset_type, Some("Building".into()));
        assert_eq!(q.search_text, Some("city".into()));
    }
}
