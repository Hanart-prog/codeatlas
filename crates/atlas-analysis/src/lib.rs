//! Churn, co-change, impact ranking, comparison and temporal backtesting.
//!
//! This crate provides history indexing, file identity lineage, churn, import
//! snapshots, co-change, impact ranking and architecture comparison. Temporal
//! backtesting arrives in Phase 5.

mod churn;
mod cochange;
mod compare;
mod graph_view;
mod impact;
mod lineage;
mod overview;
mod pipeline;
mod snapshot;
mod timeline;

pub use churn::{ChurnInput, compute_churn};
pub use cochange::{CoChangeIndex, ScopedHistory, load_scoped_history};
pub use compare::compare;
pub use graph_view::{GraphQuery, file_detail, graph};
pub use impact::impact;
pub use lineage::assign_identities;
pub use overview::{TOP_CHURN_LIMIT, overview};
pub use pipeline::{AnalyzeOptions, AnalyzeOutcome, analyze, find_repository, open_repository};
pub use snapshot::ensure_snapshot;
pub use timeline::{DEFAULT_TIMELINE_POINTS, timeline};
