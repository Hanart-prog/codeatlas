//! Temporal backtesting. Results measure how well rankings predict which files
//! change together in later commits; they say nothing about bugs, causes or
//! productivity.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::dto::CommitSummary;
use crate::ids::{Oid, RepoPath};

/// Body of `POST /api/v1/repositories/:id/backtests`. Omitted options use the
/// documented defaults.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, deny_unknown_fields)]
pub struct BacktestRequest {
    /// Eligible commits to use only as history (default 100).
    #[ts(optional)]
    pub warmup: Option<u32>,
    /// Most cases to evaluate, evenly spaced (default 200).
    #[ts(optional)]
    pub max_cases: Option<u32>,
    /// Cutoffs to report (default 5 and 10).
    #[ts(optional)]
    pub top_k: Option<Vec<u32>>,
    /// First-parent commits of history before each case (default 200).
    #[ts(optional)]
    pub history_window: Option<u32>,
    /// Seed for the random baseline, input sensitivity and intervals (default 1).
    #[ts(optional)]
    pub seed: Option<u32>,
}

/// A way of ranking files for one case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Ranker {
    /// The Change Impact ranking.
    Combined,
    /// Co-change evidence alone.
    CoChange,
    /// Import distance alone.
    ReverseDependency,
    /// The files changed most often as of the cutoff.
    MostChanged,
    /// A seeded shuffle of the files changed in the window.
    Random,
}

/// A macro average with a bootstrap 95% interval over cases.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct Estimate {
    /// Mean over cases.
    pub mean: f64,
    /// 2.5th percentile of resampled means.
    pub low: f64,
    /// 97.5th percentile of resampled means.
    pub high: f64,
}

/// Results at one cutoff.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct MetricsAtK {
    /// The cutoff.
    pub k: u32,
    /// `hits / k`, also when fewer than `k` files were ranked.
    pub precision: Estimate,
    /// `hits / held-out targets`.
    pub recall: Estimate,
    /// Share of cases with at least one hit.
    pub hit_rate: Estimate,
}

/// One ranker's results over all cases.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct RankerResult {
    /// The ranker.
    pub ranker: Ranker,
    /// One entry per cutoff.
    pub metrics: Vec<MetricsAtK>,
    /// Mean files it could rank per case.
    pub mean_candidate_count: f64,
    /// Cases where it had nothing to rank.
    pub empty_case_count: u32,
}

/// What happened to every commit.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct BacktestCounts {
    /// First-parent commits in the analysis.
    pub commit_count: u32,
    /// Commits with no eligible files.
    pub no_eligible_file_commits: u32,
    /// Commits over the bulk threshold.
    pub bulk_commits: u32,
    /// Eligible commits used only as history.
    pub warmup_commits: u32,
    /// Eligible commits after warm-up: the denominator of coverage.
    pub candidate_cases: u32,
    /// Skipped for having fewer than 2 eligible files.
    pub too_few_files: u32,
    /// Skipped because no changed file existed at the parent.
    pub no_input_at_parent: u32,
    /// Skipped because no file besides the input existed at the parent.
    pub no_target_at_parent: u32,
    /// Cases that could be evaluated.
    pub evaluable_cases: u32,
    /// Cases evaluated, after `max_cases`.
    pub evaluated_cases: u32,
    /// Evaluated cases that are merge commits.
    pub merge_cases: u32,
    /// Held-out target files in evaluated cases.
    pub target_files: u32,
    /// Files left out of evaluated cases' targets for not existing at the parent.
    pub new_files_excluded: u32,
    /// Targets the evaluated commit renamed.
    pub renamed_targets: u32,
    /// Targets the evaluated commit deleted.
    pub deleted_targets: u32,
}

/// One ranker's files for one case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CaseRanking {
    /// The ranker.
    pub ranker: Ranker,
    /// Its top files, down to the largest cutoff, at parent paths.
    pub paths: Vec<RepoPath>,
    /// Files it could rank.
    pub candidate_count: u32,
}

/// One evaluated commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct BacktestCase {
    /// First-parent position.
    pub position: u32,
    /// The evaluated commit.
    pub commit: CommitSummary,
    /// The commit every ranking is built from.
    pub parent: Oid,
    /// The file treated as known, at its parent path.
    pub input: RepoPath,
    /// The other files it changed, at parent paths.
    pub targets: Vec<RepoPath>,
    /// Every ranker's result.
    pub rankings: Vec<CaseRanking>,
}

/// One ranker's hit rate in part of the history.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct TrendRate {
    /// The ranker.
    pub ranker: Ranker,
    /// The cutoff.
    pub k: u32,
    /// Share of the bucket's cases with a hit.
    pub hit_rate: f64,
}

/// Consecutive cases, in history order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct TrendBucket {
    /// Position of the first case.
    pub from_position: u32,
    /// Position of the last case.
    pub to_position: u32,
    /// Cases in the bucket.
    pub case_count: u32,
    /// One entry per ranker and cutoff.
    pub rates: Vec<TrendRate>,
}

/// Everything a backtest produced. Identical inputs give identical bytes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct BacktestResult {
    /// The analyzed commit.
    pub target_oid: Oid,
    /// Warm-up used.
    pub warmup: u32,
    /// Case limit used.
    pub max_cases: u32,
    /// Cutoffs used.
    pub top_k: Vec<u32>,
    /// History window used.
    pub history_window: u32,
    /// Seed used.
    pub seed: u32,
    /// Import extraction version.
    pub parser_version: u32,
    /// Analysis algorithm version.
    pub algorithm_version: u32,
    /// Backtest protocol version.
    pub backtest_version: u32,
    /// Hash of the settings that affect results.
    pub config_hash: String,
    /// `evaluable_cases / candidate_cases`.
    pub coverage: f64,
    /// What happened to every commit.
    pub counts: BacktestCounts,
    /// One entry per ranker, Change Impact first.
    pub rankers: Vec<RankerResult>,
    /// The Change Impact ranking with a seeded input in place of the smallest
    /// path, to show how much that rule matters.
    pub seeded_input: Vec<MetricsAtK>,
    /// Hit rates through history.
    pub trend: Vec<TrendBucket>,
    /// Every evaluated case, so results can be audited.
    pub cases: Vec<BacktestCase>,
    /// What limits these results.
    pub notes: Vec<String>,
}

/// A stored backtest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct BacktestEntry {
    /// Unix seconds when it first ran.
    #[ts(type = "number")]
    pub created_at: i64,
    /// Its result.
    pub result: BacktestResult,
}

/// Body of `GET /api/v1/repositories/:id/backtests`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct LatestBacktest {
    /// The newest backtest, if any has run.
    pub entry: Option<BacktestEntry>,
}
