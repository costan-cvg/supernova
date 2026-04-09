//! SOV CSV export — generates Statement of Values CSV from asset data.
//!
//! Pure function: takes a slice of `AssetExportRow`, returns a CSV string.
//! No database access, no I/O. The API layer is responsible for querying
//! assets and building `AssetExportRow` values.

use serde::Serialize;
use std::collections::HashMap;

/// Standard SOV column definitions in output order.
pub const SOV_COLUMNS: &[SovColumn] = &[
    SovColumn { header: "asset_type", field_key: "asset_type" },
    SovColumn { header: "building_name", field_key: "building_name" },
    SovColumn { header: "address", field_key: "address" },
    SovColumn { header: "city", field_key: "city" },
    SovColumn { header: "state", field_key: "state" },
    SovColumn { header: "zip_code", field_key: "zip_code" },
    SovColumn { header: "year_built", field_key: "year_built" },
    SovColumn { header: "construction_class", field_key: "construction_class" },
    SovColumn { header: "occupancy", field_key: "occupancy" },
    SovColumn { header: "sq_footage", field_key: "sq_footage" },
    SovColumn { header: "stories", field_key: "stories" },
    SovColumn { header: "replacement_cost", field_key: "replacement_cost" },
    SovColumn { header: "sprinkler", field_key: "sprinkler" },
    SovColumn { header: "roof_type", field_key: "roof_type" },
    SovColumn { header: "contents_value", field_key: "contents_value" },
];

/// A column definition for the SOV export.
#[derive(Debug, Clone)]
pub struct SovColumn {
    pub header: &'static str,
    pub field_key: &'static str,
}

/// A single row of asset data for export.
/// The API layer builds these from the database query results.
#[derive(Debug, Clone, Serialize)]
pub struct AssetExportRow {
    pub asset_id: String,
    pub asset_type: String,
    pub fields: HashMap<String, String>,
}

/// Required fields for an asset to be considered "ready" for SOV export.
/// These are the minimum fields that should have values.
const REQUIRED_FIELDS: &[&str] = &[
    "building_name",
    "address",
    "city",
    "state",
    "zip_code",
    "year_built",
    "construction_class",
    "occupancy",
    "sq_footage",
    "stories",
    "replacement_cost",
];

/// Gap information for a single asset.
#[derive(Debug, Clone, Serialize)]
pub struct AssetGap {
    pub asset_id: String,
    pub asset_name: String,
    pub missing_fields: Vec<String>,
}

/// Preflight readiness report.
#[derive(Debug, Clone, Serialize)]
pub struct PreflightReport {
    pub total_assets: usize,
    pub ready_assets: usize,
    pub gap_assets: usize,
    pub readiness_percentage: f64,
    pub gaps: Vec<AssetGap>,
}

/// Generate a SOV CSV string from asset export rows.
///
/// Outputs the standard SOV columns in order. Missing fields produce
/// empty cells. The `asset_type` column is populated from the row's
/// `asset_type` field (not from the fields HashMap).
pub fn export_sov_csv(assets: &[AssetExportRow]) -> String {
    let mut wtr = csv::Writer::from_writer(Vec::new());

    // Write header row
    let headers: Vec<&str> = SOV_COLUMNS.iter().map(|c| c.header).collect();
    wtr.write_record(&headers).expect("CSV header write");

    // Write data rows
    for asset in assets {
        let record: Vec<String> = SOV_COLUMNS
            .iter()
            .map(|col| {
                if col.field_key == "asset_type" {
                    asset.asset_type.clone()
                } else {
                    asset.fields.get(col.field_key).cloned().unwrap_or_default()
                }
            })
            .collect();
        wtr.write_record(&record).expect("CSV row write");
    }

    String::from_utf8(wtr.into_inner().expect("CSV flush")).expect("CSV is valid UTF-8")
}

/// Compute a preflight readiness report for the given assets.
///
/// An asset is "ready" if all required fields have non-empty values.
/// Returns a report with gap details for assets that aren't ready.
pub fn compute_preflight(assets: &[AssetExportRow]) -> PreflightReport {
    let total_assets = assets.len();
    let mut gaps = Vec::new();

    for asset in assets {
        let missing: Vec<String> = REQUIRED_FIELDS
            .iter()
            .filter(|&&field| {
                asset
                    .fields
                    .get(field)
                    .map(|v| v.is_empty() || v == "\u{2014}")
                    .unwrap_or(true)
            })
            .map(|s| s.to_string())
            .collect();

        if !missing.is_empty() {
            let name = asset
                .fields
                .get("building_name")
                .cloned()
                .unwrap_or_else(|| format!("{} {}", asset.asset_type, &asset.asset_id[..8.min(asset.asset_id.len())]));
            gaps.push(AssetGap {
                asset_id: asset.asset_id.clone(),
                asset_name: name,
                missing_fields: missing,
            });
        }
    }

    let gap_assets = gaps.len();
    let ready_assets = total_assets - gap_assets;
    let readiness_percentage = if total_assets > 0 {
        (ready_assets as f64 / total_assets as f64) * 100.0
    } else {
        100.0
    };

    // Round to one decimal place
    let readiness_percentage = (readiness_percentage * 10.0).round() / 10.0;

    PreflightReport {
        total_assets,
        ready_assets,
        gap_assets,
        readiness_percentage,
        gaps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_row(id: &str, asset_type: &str, fields: Vec<(&str, &str)>) -> AssetExportRow {
        AssetExportRow {
            asset_id: id.into(),
            asset_type: asset_type.into(),
            fields: fields.into_iter().map(|(k, v)| (k.into(), v.into())).collect(),
        }
    }

    fn make_complete_row(id: &str) -> AssetExportRow {
        make_row(id, "Building", vec![
            ("building_name", "City Hall"),
            ("address", "100 Main St"),
            ("city", "Springfield"),
            ("state", "IL"),
            ("zip_code", "62701"),
            ("year_built", "1952"),
            ("construction_class", "Masonry"),
            ("occupancy", "Government"),
            ("sq_footage", "45000"),
            ("stories", "3"),
            ("replacement_cost", "$15000000 USD"),
            ("sprinkler", "Yes"),
            ("roof_type", "Flat"),
            ("contents_value", "$2000000 USD"),
        ])
    }

    #[test]
    fn csv_produces_valid_output_with_correct_columns() {
        let assets = vec![make_complete_row("asset-001")];
        let csv = export_sov_csv(&assets);

        // Parse back to verify it's valid CSV
        let mut reader = csv::Reader::from_reader(csv.as_bytes());
        let headers = reader.headers().unwrap();
        assert_eq!(headers.len(), SOV_COLUMNS.len());
        assert_eq!(&headers[0], "asset_type");
        assert_eq!(&headers[1], "building_name");
        assert_eq!(&headers[14], "contents_value");

        let records: Vec<_> = reader.records().collect();
        assert_eq!(records.len(), 1);

        let record = records[0].as_ref().unwrap();
        assert_eq!(&record[0], "Building");
        assert_eq!(&record[1], "City Hall");
        assert_eq!(&record[2], "100 Main St");
        assert_eq!(&record[3], "Springfield");
    }

    #[test]
    fn csv_handles_empty_assets() {
        let csv = export_sov_csv(&[]);
        let mut reader = csv::Reader::from_reader(csv.as_bytes());
        let headers = reader.headers().unwrap();
        assert_eq!(headers.len(), SOV_COLUMNS.len());
        let records: Vec<_> = reader.records().collect();
        assert_eq!(records.len(), 0);
    }

    #[test]
    fn csv_missing_fields_produce_empty_cells() {
        let assets = vec![make_row("a1", "LicensedVehicle", vec![
            ("building_name", "Fire Truck #1"),
        ])];
        let csv = export_sov_csv(&assets);
        let mut reader = csv::Reader::from_reader(csv.as_bytes());
        let record = reader.records().next().unwrap().unwrap();
        assert_eq!(&record[0], "LicensedVehicle"); // asset_type populated
        assert_eq!(&record[1], "Fire Truck #1"); // building_name populated
        assert_eq!(&record[2], ""); // address missing = empty
        assert_eq!(&record[3], ""); // city missing = empty
    }

    #[test]
    fn csv_escapes_commas_and_quotes() {
        let assets = vec![make_row("a1", "Building", vec![
            ("building_name", "Hall, \"Main\""),
            ("address", "100 Main St, Suite 200"),
        ])];
        let csv = export_sov_csv(&assets);

        // Parse back — csv crate handles unquoting
        let mut reader = csv::Reader::from_reader(csv.as_bytes());
        let record = reader.records().next().unwrap().unwrap();
        assert_eq!(&record[1], "Hall, \"Main\"");
        assert_eq!(&record[2], "100 Main St, Suite 200");
    }

    #[test]
    fn csv_multiple_rows() {
        let assets = vec![
            make_complete_row("a1"),
            make_row("a2", "PropertyInTheOpen", vec![("building_name", "Library Contents")]),
        ];
        let csv = export_sov_csv(&assets);
        let mut reader = csv::Reader::from_reader(csv.as_bytes());
        let records: Vec<_> = reader.records().collect();
        assert_eq!(records.len(), 2);
    }

    #[test]
    fn preflight_all_ready() {
        let assets = vec![make_complete_row("a1"), make_complete_row("a2")];
        let report = compute_preflight(&assets);
        assert_eq!(report.total_assets, 2);
        assert_eq!(report.ready_assets, 2);
        assert_eq!(report.gap_assets, 0);
        assert_eq!(report.readiness_percentage, 100.0);
        assert!(report.gaps.is_empty());
    }

    #[test]
    fn preflight_identifies_gaps() {
        let assets = vec![
            make_complete_row("a1"),
            make_row("a2", "Building", vec![
                ("building_name", "Fire Station"),
                ("address", "200 Oak Ave"),
                // Missing: city, state, zip_code, year_built, construction_class,
                //          occupancy, sq_footage, stories, replacement_cost
            ]),
        ];
        let report = compute_preflight(&assets);
        assert_eq!(report.total_assets, 2);
        assert_eq!(report.ready_assets, 1);
        assert_eq!(report.gap_assets, 1);
        assert_eq!(report.readiness_percentage, 50.0);
        assert_eq!(report.gaps.len(), 1);
        assert_eq!(report.gaps[0].asset_id, "a2");
        assert_eq!(report.gaps[0].asset_name, "Fire Station");
        assert!(report.gaps[0].missing_fields.contains(&"city".to_string()));
        assert!(report.gaps[0].missing_fields.contains(&"year_built".to_string()));
        assert!(report.gaps[0].missing_fields.contains(&"sq_footage".to_string()));
        // address and building_name are present, so not in missing
        assert!(!report.gaps[0].missing_fields.contains(&"address".to_string()));
        assert!(!report.gaps[0].missing_fields.contains(&"building_name".to_string()));
    }

    #[test]
    fn preflight_empty_values_count_as_missing() {
        let assets = vec![make_row("a1", "Building", vec![
            ("building_name", ""),
            ("address", ""),
            ("city", ""),
            ("state", ""),
            ("zip_code", ""),
            ("year_built", ""),
            ("construction_class", ""),
            ("occupancy", ""),
            ("sq_footage", ""),
            ("stories", ""),
            ("replacement_cost", ""),
        ])];
        let report = compute_preflight(&assets);
        assert_eq!(report.gap_assets, 1);
        assert_eq!(report.gaps[0].missing_fields.len(), REQUIRED_FIELDS.len());
    }

    #[test]
    fn preflight_dash_values_count_as_missing() {
        let assets = vec![make_row("a1", "Building", vec![
            ("building_name", "Test"),
            ("address", "100 Main"),
            ("city", "Springfield"),
            ("state", "IL"),
            ("zip_code", "62701"),
            ("year_built", "\u{2014}"), // em-dash from display_field_value Null
            ("construction_class", "Masonry"),
            ("occupancy", "Government"),
            ("sq_footage", "45000"),
            ("stories", "3"),
            ("replacement_cost", "$15000000 USD"),
        ])];
        let report = compute_preflight(&assets);
        assert_eq!(report.gap_assets, 1);
        assert!(report.gaps[0].missing_fields.contains(&"year_built".to_string()));
    }

    #[test]
    fn preflight_no_assets() {
        let report = compute_preflight(&[]);
        assert_eq!(report.total_assets, 0);
        assert_eq!(report.ready_assets, 0);
        assert_eq!(report.readiness_percentage, 100.0);
    }

    #[test]
    fn preflight_readiness_percentage_rounds() {
        // 1 of 3 ready = 33.333...% → rounds to 33.3
        let assets = vec![
            make_complete_row("a1"),
            make_row("a2", "Building", vec![("building_name", "B")]),
            make_row("a3", "Building", vec![("building_name", "C")]),
        ];
        let report = compute_preflight(&assets);
        assert_eq!(report.readiness_percentage, 33.3);
    }
}
