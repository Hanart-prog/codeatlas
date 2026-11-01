//! Types that cross the HTTP boundary or are shared between engine layers. The
//! TypeScript in `apps/web/src/types/generated` is generated from these.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::ErrorCode;
use crate::history::ExclusionReason;
use crate::ids::{AnalysisRunId, Oid, RepoPath, RepositoryId};

/// Overall readiness reported by `/health`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    /// All local dependencies are usable.
    Ok,
    /// The server is up but cannot serve data.
    Degraded,
}

/// The state of the local SQLite database.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DatabaseHealth {
    /// Whether a query against the database succeeded.
    pub reachable: bool,
    /// Latest migration applied to the database, if it could be read.
    pub schema_version: Option<u32>,
    /// Latest migration this build knows about.
    pub expected_schema_version: u32,
}

/// Body of `GET /api/v1/health`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct HealthResponse {
    /// Overall readiness.
    pub status: HealthStatus,
    /// Application version.
    pub version: String,
    /// HTTP API contract version.
    pub api_version: String,
    /// Import extraction version.
    pub parser_version: u32,
    /// Analysis algorithm version.
    pub algorithm_version: u32,
    /// Database state.
    pub database: DatabaseHealth,
}

/// The error object inside every API error response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ApiErrorDetail {
    /// Stable machine-readable code.
    pub code: ErrorCode,
    /// Human-readable description.
    pub message: String,
    /// Optional structured context.
    #[ts(type = "unknown")]
    pub details: Option<serde_json::Value>,
}

/// Every API error response: `{ "error": { "code", "message", "details" } }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ApiErrorBody {
    /// The error.
    pub error: ApiErrorDetail,
}

/// Lifecycle of a row in `analysis_runs`. The `CHECK` constraint in the
/// migration must list the same values; a storage test enforces this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisStatus {
    /// Waiting for a worker.
    Queued,
    /// In progress.
    Running,
    /// Finished successfully.
    Completed,
    /// Finished with an error.
    Failed,
    /// Stopped at the user's request.
    Cancelled,
}

impl AnalysisStatus {
    /// Every status, in lifecycle order.
    pub const ALL: [Self; 5] = [
        Self::Queued,
        Self::Running,
        Self::Completed,
        Self::Failed,
        Self::Cancelled,
    ];

    /// The value stored in SQLite and sent over the API.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

/// How one file depends on another. These are all static evidence; historical
/// co-change is deliberately not a variant, because it is correlation rather
/// than a dependency and is stored and displayed separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum DependencyEdgeType {
    /// `import ... from` or a side-effect `import '...'`.
    StaticImport,
    /// `import type ... from`, erased at compile time.
    TypeOnlyImport,
    /// `export ... from`.
    ReExport,
    /// `import('./literal')`, which may load lazily or never.
    DynamicImport,
}

/// Body of `POST /api/v1/repositories/open`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct OpenRepositoryRequest {
    /// Absolute local path of a Git repository.
    pub path: String,
}

/// Body of `POST /api/v1/repositories/:id/analyze`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, deny_unknown_fields)]
pub struct AnalyzeRequest {
    /// Git reference to analyze; defaults to `HEAD`.
    #[ts(optional)]
    pub git_ref: Option<String>,
    /// Maximum first-parent commits to index.
    #[ts(optional)]
    pub history: Option<u32>,
}

/// A repository CodeAtlas has opened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RepositoryEntry {
    /// Local identifier.
    pub id: RepositoryId,
    /// Canonical filesystem path.
    pub path: String,
    /// Unix seconds of the latest completed analysis, if any.
    #[ts(type = "number | null")]
    pub analyzed_at: Option<i64>,
}

/// Lifecycle of a background job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    /// Waiting for a worker.
    Queued,
    /// In progress.
    Running,
    /// Finished successfully.
    Completed,
    /// Finished with an error.
    Failed,
}

/// Body of `GET /api/v1/jobs/:id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct JobResponse {
    /// Job identifier, valid until the server restarts.
    pub id: u32,
    /// Repository the job belongs to.
    pub repository_id: RepositoryId,
    /// Current state.
    pub status: JobStatus,
    /// Failure description, when `status` is `failed`.
    pub error: Option<String>,
    /// How far it has got, for jobs that can tell.
    pub progress: Option<JobProgress>,
}

/// Steps finished of a job's total.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct JobProgress {
    /// Steps finished.
    pub done: u32,
    /// All steps.
    pub total: u32,
}

/// One commit, as shown in summaries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CommitSummary {
    /// Commit id.
    pub oid: Oid,
    /// Committer time in Unix seconds. Not necessarily monotonic.
    #[ts(type = "number")]
    pub committed_at: i64,
    /// First line of the message. Untrusted text.
    pub subject: String,
}

/// One entry of the first-parent timeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CommitEntry {
    /// Index in first-parent order, where 0 is the oldest indexed commit.
    pub position: u32,
    /// Commit id.
    pub oid: Oid,
    /// Committer time in Unix seconds. Not necessarily monotonic.
    #[ts(type = "number")]
    pub committed_at: i64,
    /// First line of the message. Untrusted text.
    pub subject: String,
    /// Number of parents.
    pub parent_count: u32,
}

/// A page of commits, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CommitPage {
    /// The commits on this page.
    pub commits: Vec<CommitEntry>,
    /// Pass as `cursor` to fetch the next page; absent on the last page.
    pub next_cursor: Option<u32>,
}

/// Which analysis produced an overview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RunSummary {
    /// Run identifier.
    pub id: AnalysisRunId,
    /// Commit the run analyzed.
    pub target_oid: Oid,
    /// Requested limit on first-parent commits.
    pub history_limit: u32,
    /// Unix seconds when the run finished.
    #[ts(type = "number | null")]
    pub completed_at: Option<i64>,
    /// Import extraction version.
    pub parser_version: u32,
    /// Analysis algorithm version.
    pub algorithm_version: u32,
    /// Hash of the settings that affect results.
    pub config_hash: String,
}

/// The indexed span of history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct HistorySummary {
    /// First-parent commits indexed.
    pub commit_count: u32,
    /// How many of those are merge commits, counted against their first parent.
    pub merge_commit_count: u32,
    /// Oldest indexed commit.
    pub first_commit: Option<CommitSummary>,
    /// Newest indexed commit.
    pub last_commit: Option<CommitSummary>,
    /// The clone is shallow, so older history is missing.
    pub is_shallow: bool,
    /// The history limit cut off older commits.
    pub is_truncated: bool,
}

/// File counts for the analyzed commit and the indexed history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FileSummary {
    /// Regular files in the tree of the analyzed commit.
    pub tip_file_count: u32,
    /// TS/TSX files among them, after exclusion rules.
    pub tip_typescript_file_count: u32,
    /// Distinct file identities with at least one eligible change.
    pub changed_file_count: u32,
    /// Rename events in the indexed history.
    pub rename_count: u32,
    /// Changes skipped because their path is not valid UTF-8 or is unsafe.
    pub unreadable_path_count: u32,
}

/// How many file changes one rule excluded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ExclusionCount {
    /// Why they were excluded.
    pub reason: ExclusionReason,
    /// Number of file changes.
    pub count: u32,
}

/// Change activity of one file identity across renames, within the indexed
/// first-parent history. Excluded changes are not counted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FileChurn {
    /// Latest path of this identity.
    pub path: RepoPath,
    /// Whether the file exists at the analyzed commit.
    pub exists_at_tip: bool,
    /// Commits that touched the file, including commits that only renamed it.
    pub commit_count: u32,
    /// Lines added.
    pub added_lines: u32,
    /// Lines deleted.
    pub deleted_lines: u32,
    /// Earlier paths of this identity, oldest first.
    pub previous_paths: Vec<RepoPath>,
}

/// Body of `GET /api/v1/repositories/:id/overview`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OverviewResponse {
    /// The repository.
    pub repository: RepositoryEntry,
    /// The analysis this overview comes from.
    pub run: RunSummary,
    /// Indexed history.
    pub history: HistorySummary,
    /// File counts.
    pub files: FileSummary,
    /// File changes left out of metrics, by reason.
    pub exclusions: Vec<ExclusionCount>,
    /// Files with the most changes, most active first, ties by path.
    pub top_churn: Vec<FileChurn>,
    /// The repository's `HEAD` now, if it can still be read. When it differs from
    /// the analyzed commit, these results do not cover the latest work.
    pub head_oid: Option<Oid>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_body_matches_the_api_contract() {
        let body = ApiErrorBody {
            error: ApiErrorDetail {
                code: ErrorCode::NotFound,
                message: "missing".to_owned(),
                details: None,
            },
        };
        assert_eq!(
            serde_json::to_value(body).unwrap(),
            serde_json::json!({
                "error": { "code": "not_found", "message": "missing", "details": null }
            })
        );
    }

    #[test]
    fn analysis_status_as_str_matches_serde() {
        for status in AnalysisStatus::ALL {
            assert_eq!(
                serde_json::to_value(status).unwrap(),
                serde_json::Value::from(status.as_str())
            );
        }
    }
}
