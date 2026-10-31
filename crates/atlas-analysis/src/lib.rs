//! Churn, co-change, impact ranking and temporal backtesting.
//!
//! Phase 1 provides history indexing, file identity lineage and churn. Co-change,
//! impact ranking and backtesting arrive in Phases 3 and 5.

mod churn;
mod lineage;
mod overview;
mod pipeline;

pub use churn::{ChurnInput, compute_churn};
pub use lineage::assign_identities;
pub use overview::{TOP_CHURN_LIMIT, overview};
pub use pipeline::{AnalyzeOptions, AnalyzeOutcome, analyze, find_repository, open_repository};
