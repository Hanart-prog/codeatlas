//! Change impact. A score is a ranking heuristic built from separate kinds of
//! evidence; it is never a probability of failure.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::dto::CommitSummary;
use crate::ids::{Oid, RepoPath};

/// Body of `POST /api/v1/repositories/:id/impact`. Omitted options use the
/// documented defaults.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, deny_unknown_fields)]
pub struct ImpactRequest {
    /// Files being changed, as they exist at the cutoff.
    pub selected_paths: Vec<String>,
    /// Commit to rank as of; defaults to the latest analyzed commit. Nothing
    /// later is read.
    #[ts(optional)]
    pub cutoff: Option<String>,
    /// Reverse dependency hops to follow (default 3).
    #[ts(optional)]
    pub max_depth: Option<u32>,
    /// Recommendations to return (default 10).
    #[ts(optional)]
    pub top_k: Option<u32>,
    /// First-parent commits of history, ending at the cutoff (default 200).
    #[ts(optional)]
    pub history_window: Option<u32>,
    /// Weight of static evidence (default 0.6).
    #[ts(optional)]
    pub static_weight: Option<f64>,
    /// Weight of historical evidence (default 0.4).
    #[ts(optional)]
    pub historical_weight: Option<f64>,
}

/// How much CodeAtlas can say about a selected file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SelectedFileStatus {
    /// A TS/TSX file with import and history evidence available.
    Analyzed,
    /// Excluded by rules, so there is no evidence for it.
    Excluded,
    /// Another language, so there is no evidence for it.
    UnsupportedLanguage,
}

/// One selected file and what is known about it at the cutoff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SelectedFile {
    /// Path at the cutoff.
    pub path: RepoPath,
    /// Available evidence.
    pub status: SelectedFileStatus,
    /// Eligible commits in the window that touched the file.
    pub commit_count: u32,
    /// Files importing it directly.
    pub direct_dependent_count: u32,
}

/// Which kinds of evidence support a recommendation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    /// The file imports a selected file, directly or transitively.
    StaticDependency,
    /// The file changed together with a selected file in past commits.
    HistoricalCoChange,
}

/// Import evidence for a recommendation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct StaticEvidence {
    /// Reverse dependency hops to the nearest selected file.
    pub distance: u32,
    /// The import chain: the candidate first, the selected file last.
    pub path: Vec<RepoPath>,
}

/// Co-change evidence for a recommendation, from the selected file with the
/// highest conditional probability.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct HistoricalEvidence {
    /// The selected file this evidence comes from.
    pub selected: RepoPath,
    /// Eligible commits touching both files.
    pub shared_commit_count: u32,
    /// Eligible commits touching the selected file.
    pub selected_commit_count: u32,
    /// Eligible commits touching the candidate.
    pub candidate_commit_count: u32,
    /// `P(candidate changed | selected changed)`: shared / selected.
    pub conditional_probability: f64,
    /// shared / (selected + candidate - shared).
    pub jaccard: f64,
    /// Share of eligible commits touching the candidate at all.
    pub base_rate: f64,
    /// Conditional probability over base rate. Near 1, the candidate changes
    /// this often anyway.
    pub lift: f64,
    /// Newest supporting commits. Messages are untrusted text.
    pub commits: Vec<CommitSummary>,
}

/// One file to inspect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ImpactRecommendation {
    /// One-based rank.
    pub rank: u32,
    /// Path at the cutoff.
    pub path: RepoPath,
    /// Weighted ranking score. Not a probability.
    pub score: f64,
    /// 1 / distance, or 0 with no dependency in range.
    pub static_score: f64,
    /// Highest qualifying conditional probability, or 0.
    pub historical_score: f64,
    /// Kinds of evidence present.
    pub evidence: Vec<EvidenceKind>,
    /// Import evidence, if any.
    pub static_evidence: Option<StaticEvidence>,
    /// Co-change evidence, if any.
    pub historical_evidence: Option<HistoricalEvidence>,
    /// Reasons to treat this recommendation with care.
    pub caveats: Vec<String>,
}

/// What the co-change counts are based on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CoChangeStats {
    /// First-parent commits read, ending at the cutoff.
    pub commit_count: u32,
    /// Commits with at least one eligible file and within the bulk threshold.
    pub eligible_commit_count: u32,
    /// Commits skipped for touching too many files.
    pub bulk_commit_count: u32,
    /// Commits skipped for touching no eligible file.
    pub no_eligible_file_commit_count: u32,
    /// Merge commits among the eligible, each counted once.
    pub merge_commit_count: u32,
    /// The bulk threshold applied.
    pub bulk_threshold: u32,
    /// The minimum shared commits applied.
    pub min_shared_commits: u32,
    /// The clone is shallow, so older history is missing.
    pub is_shallow: bool,
    /// The window stopped before the root commit.
    pub is_truncated: bool,
}

/// Body of an impact response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ImpactResponse {
    /// The commit ranked as of.
    pub cutoff: Oid,
    /// The selected files.
    pub selected: Vec<SelectedFile>,
    /// Recommendations, best first, ties by path.
    pub recommendations: Vec<ImpactRecommendation>,
    /// Files with any evidence, before `top_k`.
    pub candidate_count: u32,
    /// What history was used.
    pub history: CoChangeStats,
    /// Hops followed.
    pub max_depth: u32,
    /// Static weight used.
    pub static_weight: f64,
    /// Historical weight used.
    pub historical_weight: f64,
    /// Explanations for missing evidence.
    pub notes: Vec<String>,
}

/// A pair of files that changed together, in a stable order: `file_a` < `file_b`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CoChangeEdge {
    /// One file.
    pub file_a: RepoPath,
    /// The other file.
    pub file_b: RepoPath,
    /// Eligible commits touching both.
    pub shared_commit_count: u32,
    /// Eligible commits touching `file_a`.
    pub file_a_commit_count: u32,
    /// Eligible commits touching `file_b`.
    pub file_b_commit_count: u32,
}
