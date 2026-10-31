//! Git history as the engine sees it, after validation and before persistence.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{Oid, RepoPath};

/// What happened to a file in a commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ChangeStatus {
    /// The path did not exist before.
    Added,
    /// Content or mode changed in place.
    Modified,
    /// The path no longer exists.
    Deleted,
    /// Git matched the path to a similar file at another path.
    Renamed,
    /// The entry changed kind, for example from a file to a symlink.
    TypeChanged,
}

impl ChangeStatus {
    /// The value stored in SQLite.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Modified => "modified",
            Self::Deleted => "deleted",
            Self::Renamed => "renamed",
            Self::TypeChanged => "type_changed",
        }
    }

    /// Parses the value stored in SQLite.
    pub fn parse(value: &str) -> Option<Self> {
        [
            Self::Added,
            Self::Modified,
            Self::Deleted,
            Self::Renamed,
            Self::TypeChanged,
        ]
        .into_iter()
        .find(|status| status.as_str() == value)
    }
}

/// Why a file change is left out of metrics. Excluded changes stay stored so
/// that the numbers can be reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ExclusionReason {
    /// Git classified the content as binary.
    Binary,
    /// The blob exceeds `limits.max_file_bytes`.
    Oversized,
    /// A symbolic link, whose blob is a path rather than source.
    Symlink,
    /// A submodule pointer, which has no content in this repository.
    Submodule,
    /// The path matches `analysis.exclude_globs`.
    Rule,
}

impl ExclusionReason {
    /// The value stored in SQLite.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Binary => "binary",
            Self::Oversized => "oversized",
            Self::Symlink => "symlink",
            Self::Submodule => "submodule",
            Self::Rule => "rule",
        }
    }

    /// Parses the value stored in SQLite.
    pub fn parse(value: &str) -> Option<Self> {
        [
            Self::Binary,
            Self::Oversized,
            Self::Symlink,
            Self::Submodule,
            Self::Rule,
        ]
        .into_iter()
        .find(|reason| reason.as_str() == value)
    }
}

/// A commit on the indexed first-parent chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitRecord {
    /// Commit id.
    pub oid: Oid,
    /// All parents, in order.
    pub parents: Vec<Oid>,
    /// Committer time in Unix seconds.
    pub committed_at: i64,
    /// First line of the message, with control characters replaced.
    pub subject: String,
}

/// One changed file within a commit, diffed against the commit's first parent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    /// Path before the commit, absent for additions.
    pub old_path: Option<RepoPath>,
    /// Path after the commit, absent for deletions.
    pub new_path: Option<RepoPath>,
    /// What happened.
    pub status: ChangeStatus,
    /// Lines added; 0 for excluded changes.
    pub added_lines: u32,
    /// Lines deleted; 0 for excluded changes.
    pub deleted_lines: u32,
    /// Why this change is left out of metrics, if it is.
    pub exclusion: Option<ExclusionReason>,
}

/// The changes of one commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitChanges {
    /// The commit.
    pub commit: CommitRecord,
    /// Valid changes, sorted by path.
    pub changes: Vec<FileChange>,
    /// Changes dropped because their path is not valid UTF-8 or is unsafe.
    pub unreadable_paths: u32,
}

/// Index of a file identity within one analysis run.
pub type IdentityIndex = usize;

/// A path held by a file identity over a span of first-parent positions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathSpan {
    /// The path.
    pub path: RepoPath,
    /// First position with this path.
    pub valid_from: u32,
    /// First position without this path; absent while it is still current.
    pub valid_to: Option<u32>,
}

/// A file followed through renames. A path may belong to different identities
/// over time, because deleting a file and adding one at the same path starts a
/// new identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileIdentity {
    /// Position of the first indexed change.
    pub first_seen: u32,
    /// Paths held, oldest first. Consecutive spans do not overlap.
    pub spans: Vec<PathSpan>,
}

/// A file change with its position and identity resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentifiedChange {
    /// First-parent position of the commit.
    pub position: u32,
    /// The identity the change belongs to.
    pub identity: IdentityIndex,
    /// The change.
    pub change: FileChange,
}

/// Facts about the tree at the analyzed commit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TreeSummary {
    /// Regular files.
    pub file_count: u32,
    /// TS/TSX files after exclusion rules.
    pub typescript_file_count: u32,
}

/// Everything one analysis derives from Git, ready to persist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedHistory {
    /// Commits, oldest first. An index here is a first-parent position.
    pub commits: Vec<CommitRecord>,
    /// All file identities.
    pub identities: Vec<FileIdentity>,
    /// All file changes in position order.
    pub changes: Vec<IdentifiedChange>,
    /// Total changes dropped for unreadable paths.
    pub unreadable_paths: u32,
    /// The clone is shallow.
    pub is_shallow: bool,
    /// The history limit stopped the walk before the root commit.
    pub is_truncated: bool,
    /// Tree facts at the analyzed commit.
    pub tree: TreeSummary,
}
