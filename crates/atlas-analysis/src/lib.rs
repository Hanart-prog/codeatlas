//! Churn, co-change, impact ranking and temporal backtesting.
//!
//! This crate provides history indexing, file identity lineage, churn and import
//! snapshots. Co-change, impact ranking and backtesting arrive in Phases 3 and 5.

mod churn;
mod graph_view;
mod lineage;
mod overview;
mod pipeline;
mod snapshot;

pub use churn::{ChurnInput, compute_churn};
pub use graph_view::{GraphQuery, file_detail, graph};
pub use lineage::assign_identities;
pub use overview::{TOP_CHURN_LIMIT, overview};
pub use pipeline::{AnalyzeOptions, AnalyzeOutcome, analyze, find_repository, open_repository};
pub use snapshot::ensure_snapshot;
