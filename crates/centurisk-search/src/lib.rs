//! Search index (SQLite FTS5) and natural language query translation.
//!
//! The NL layer translates human queries like "buildings over $5M in Springfield"
//! into structured filters. All processing stays within the system boundary — no
//! external LLM calls.

pub mod fts;
pub mod nl;

pub use fts::SearchIndex;
pub use nl::{NlQuery, translate_query};
