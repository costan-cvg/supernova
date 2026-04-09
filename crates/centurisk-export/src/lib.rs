//! SOV generation, CAT export, Certificates of Insurance.
//!
//! This crate provides pure functions for generating export artifacts
//! from asset data. No I/O — callers provide the data, get strings back.

pub mod sov;

pub use sov::{AssetExportRow, SovColumn, export_sov_csv, SOV_COLUMNS};
