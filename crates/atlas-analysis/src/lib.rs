//! Churn, co-change, impact ranking and temporal backtesting.
//!
//! This crate provides history indexing, file identity lineage, churn, import
//! snapshots, co-change and impact ranking. Temporal backtesting arrives in Phase 5.

mod churn;
mod cochange;
mod graph_view;
mod impact;
mod lineage;
mod overview;
mod pipeline;
mod snapshot;

pub use churn::{ChurnInput, compute_churn};
pub use cochange::{CoChangeIndex, ScopedHistory, load_scoped_history};
pub use graph_view::{GraphQuery, file_detail, graph};
pub use impact::impact;
pub use lineage::assign_identities;
pub use overview::{TOP_CHURN_LIMIT, overview};
pub use pipeline::{AnalyzeOptions, AnalyzeOutcome, analyze, find_repository, open_repository};
pub use snapshot::ensure_snapshot;
