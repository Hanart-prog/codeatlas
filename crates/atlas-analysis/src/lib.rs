//! Churn, co-change, impact ranking, comparison and temporal backtesting.
//!
//! This crate provides history indexing, file identity lineage, churn, import
//! snapshots, co-change, impact ranking, architecture comparison and backtesting.

mod backtest;
mod churn;
mod cochange;
mod commits;
mod compare;
mod graph_view;
mod impact;
mod lineage;
mod overview;
mod pipeline;
mod report;
mod snapshot;
mod timeline;

pub use backtest::backtest;
pub use churn::{ChurnInput, compute_churn};
pub use cochange::{CoChangeIndex, ScopedHistory, load_scoped_history};
pub use compare::compare;
pub use graph_view::{GraphQuery, directory_map, file_detail, graph};
pub use impact::impact;
pub use lineage::assign_identities;
pub use overview::{TOP_CHURN_LIMIT, overview};
pub use pipeline::{AnalyzeOptions, AnalyzeOutcome, analyze, find_repository, open_repository};
pub use report::{render_report, report};
pub use snapshot::ensure_snapshot;
pub use timeline::{DEFAULT_TIMELINE_POINTS, timeline};
