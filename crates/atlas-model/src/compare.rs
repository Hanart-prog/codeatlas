//! Architecture comparison. Differences are in static imports between two
//! commits; they say nothing about what runs.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::dto::CommitSummary;
use crate::graph::{Cycle, GraphEdge, GraphNode, ParserCoverage};
use crate::ids::RepoPath;

/// Body of `POST /api/v1/repositories/:id/compare`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CompareRequest {
    /// Commit, branch or tag to compare from.
    pub base: String,
    /// Commit, branch or tag to compare to.
    pub target: String,
    /// Most entries in each list (default 500). Counts are never cut.
    #[serde(default)]
    #[ts(optional)]
    pub limit: Option<u32>,
}

/// How the two commits relate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CommitRelation {
    /// They are the same commit.
    Same,
    /// The base is an ancestor of the target.
    Forward,
    /// The target is an ancestor of the base, so the comparison runs backwards.
    Backward,
    /// Neither is an ancestor of the other.
    Diverged,
}

/// One side of a comparison.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ArchitectureSummary {
    /// The commit.
    pub commit: CommitSummary,
    /// Source files in the graph.
    pub file_count: u32,
    /// Import edges between them.
    pub import_count: u32,
    /// Strongly connected components with a cycle.
    pub cycle_count: u32,
    /// Files in a cycle.
    pub cyclic_file_count: u32,
    /// Members of the largest cycle, or 0.
    pub largest_cycle_size: u32,
    /// What the snapshot covers.
    pub coverage: ParserCoverage,
}

/// How a rename was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum RenameEvidence {
    /// The two commits' files are similar enough.
    Direct,
    /// Commits between them renamed it step by step, though the two versions
    /// may no longer look alike.
    History,
}

/// A file followed from one path to another by similarity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FileRename {
    /// Path at the base commit.
    pub from: RepoPath,
    /// Path at the target commit.
    pub to: RepoPath,
    /// How it was found.
    pub evidence: RenameEvidence,
}

/// A cycle that continued with different members.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CycleChange {
    /// The cycle at the base commit.
    pub before: Cycle,
    /// The cycle at the target commit.
    pub after: Cycle,
    /// Members only at the target commit, by target path.
    pub joined: Vec<RepoPath>,
    /// Members only at the base commit, by base path.
    pub left: Vec<RepoPath>,
}

/// An import present at only one commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ImportChange {
    /// The edge, with paths from the commit where it exists.
    pub edge: GraphEdge,
    /// The importing file did not parse cleanly at one of the commits, so this
    /// may come from parsing rather than from the code.
    pub uncertain: bool,
}

/// Before and after values for a file present at both commits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FileMetricChange {
    /// Path at the target commit.
    pub path: RepoPath,
    /// Path at the base commit, if the file was renamed.
    pub previous_path: Option<RepoPath>,
    /// Files importing it at the base commit.
    pub in_degree_before: u32,
    /// Files importing it at the target commit.
    pub in_degree_after: u32,
    /// Files it imports at the base commit.
    pub out_degree_before: u32,
    /// Files it imports at the target commit.
    pub out_degree_after: u32,
    /// Files depending on it, directly or not, at the base commit.
    pub dependents_before: u32,
    /// Files depending on it, directly or not, at the target commit.
    pub dependents_after: u32,
}

/// True totals, which `limit` does not affect.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CompareCounts {
    /// Files only at the target commit.
    pub added_files: u32,
    /// Files only at the base commit.
    pub removed_files: u32,
    /// Files followed across a rename.
    pub renamed_files: u32,
    /// Imports only at the target commit.
    pub added_imports: u32,
    /// Imports only at the base commit.
    pub removed_imports: u32,
    /// Added or removed imports marked uncertain.
    pub uncertain_imports: u32,
    /// Imports that kept their meaning while a file they touch was renamed.
    pub imports_following_renames: u32,
    /// Cycles only at the target commit.
    pub new_cycles: u32,
    /// Cycles only at the base commit.
    pub resolved_cycles: u32,
    /// Cycles sharing some members across the commits.
    pub changed_cycles: u32,
    /// Cycles with the same members at both commits.
    pub unchanged_cycles: u32,
    /// Files whose degrees or dependents changed.
    pub metric_changes: u32,
}

/// Body of `POST /api/v1/repositories/:id/compare`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CompareResponse {
    /// The commit compared from.
    pub base: ArchitectureSummary,
    /// The commit compared to.
    pub target: ArchitectureSummary,
    /// How the commits relate.
    pub relation: CommitRelation,
    /// True totals.
    pub counts: CompareCounts,
    /// Files only at the target commit, by path.
    pub added_files: Vec<GraphNode>,
    /// Files only at the base commit, by path.
    pub removed_files: Vec<GraphNode>,
    /// Files followed across a rename, by old path.
    pub renamed_files: Vec<FileRename>,
    /// Imports only at the target commit, by source and target.
    pub added_imports: Vec<ImportChange>,
    /// Imports only at the base commit, by source and target.
    pub removed_imports: Vec<ImportChange>,
    /// Cycles only at the target commit, largest first.
    pub new_cycles: Vec<Cycle>,
    /// Cycles only at the base commit, largest first.
    pub resolved_cycles: Vec<Cycle>,
    /// Cycles present at both commits with different members, largest first.
    pub changed_cycles: Vec<CycleChange>,
    /// Files whose metrics changed, largest change first, ties by path.
    pub metric_changes: Vec<FileMetricChange>,
    /// Why a figure here may not compare like with like.
    pub notes: Vec<String>,
}

/// The architecture at one commit of a timeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TimelinePoint {
    /// First-parent position, oldest first.
    pub position: u32,
    /// The commit.
    pub commit: CommitSummary,
    /// Source files in the graph.
    pub file_count: u32,
    /// Import edges between them.
    pub import_count: u32,
    /// Strongly connected components with a cycle.
    pub cycle_count: u32,
    /// Files in a cycle.
    pub cyclic_file_count: u32,
    /// Files not parsed, so counted without their imports.
    pub skipped_file_count: u32,
}

/// Body of `GET /api/v1/repositories/:id/timeline`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TimelineResponse {
    /// Evenly spaced commits of the latest analysis, oldest first, always
    /// including the first and last.
    pub points: Vec<TimelinePoint>,
    /// First-parent commits in the analysis.
    pub commit_count: u32,
}
