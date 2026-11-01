//! A shareable record of one analysis. Everything here is computed from the
//! repository; the provenance says exactly how, so that it can be reproduced.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::backtest::{BacktestCounts, RankerResult};
use crate::compare::TimelinePoint;
use crate::dto::{ExclusionCount, FileChurn, FileSummary, HistorySummary};
use crate::graph::{Cycle, GraphNode, ParserCoverage};
use crate::ids::Oid;

/// What produced a report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ReportProvenance {
    /// CodeAtlas version.
    pub tool_version: String,
    /// Unix seconds when the report was written.
    #[ts(type = "number")]
    pub generated_at: i64,
    /// The repository's directory name. The full path stays out, because reports
    /// are made to be shared and a path can reveal a user name.
    pub repository_name: String,
    /// The analyzed commit.
    pub commit: Oid,
    /// Requested limit on first-parent commits.
    pub history_limit: u32,
    /// Import extraction version.
    pub parser_version: u32,
    /// Analysis algorithm version.
    pub algorithm_version: u32,
    /// Backtest protocol version.
    pub backtest_version: u32,
    /// Hash of the settings that affect results.
    pub config_hash: String,
}

/// The import graph at the analyzed commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ReportArchitecture {
    /// Source files.
    pub file_count: u32,
    /// Import edges between them.
    pub import_count: u32,
    /// All cycles.
    pub cycle_count: u32,
    /// Files in a cycle.
    pub cyclic_file_count: u32,
    /// The largest cycles.
    pub largest_cycles: Vec<Cycle>,
    /// The files most imported by others.
    pub most_imported: Vec<GraphNode>,
    /// What the snapshot covers.
    pub coverage: ParserCoverage,
}

/// The latest backtest, without its per-commit detail, which the Backtesting
/// page exports in full.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ReportBacktest {
    /// Warm-up used.
    pub warmup: u32,
    /// `evaluable_cases / candidate_cases`.
    pub coverage: f64,
    /// What happened to every commit.
    pub counts: BacktestCounts,
    /// One entry per ranker, Change Impact first.
    pub rankers: Vec<RankerResult>,
    /// What limits these results.
    pub notes: Vec<String>,
}

/// Body of `GET /api/v1/repositories/:id/report`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ReportDocument {
    /// What produced it.
    pub provenance: ReportProvenance,
    /// The indexed history.
    pub history: HistorySummary,
    /// File counts.
    pub files: FileSummary,
    /// Changes left out of metrics, by reason.
    pub exclusions: Vec<ExclusionCount>,
    /// The most changed files.
    pub top_churn: Vec<FileChurn>,
    /// The import graph, absent if the repository is over a graph limit.
    pub architecture: Option<ReportArchitecture>,
    /// Evenly spaced commits of the analysis, oldest first.
    pub timeline: Vec<TimelinePoint>,
    /// The latest backtest of this analysis, if one has run.
    pub backtest: Option<ReportBacktest>,
    /// What these figures do not show.
    pub limitations: Vec<String>,
}
