//! Read-only access to analyzed Git repositories.
//!
//! Everything here reads commits, trees and blobs from the object database. This
//! crate never checks out, writes to the index or refs, or fetches, and libgit2
//! does not run hooks for any of the operations used. All repository content is
//! untrusted input.

mod exclusions;

use std::path::{Path, PathBuf};

use atlas_model::history::{
    ChangeStatus, CommitChanges, CommitRecord, ExclusionReason, FileChange, TreeSummary,
};
use atlas_model::{AnalysisConfig, AtlasError, CommitSummary, Limits, Oid, RepoPath};
use git2::{
    Commit, Delta, DiffFindOptions, DiffOptions, FileMode, ObjectType, Patch, Repository,
    RepositoryOpenFlags, TreeWalkMode, TreeWalkResult,
};
use thiserror::Error;

pub use exclusions::ExclusionRules;

/// Candidates considered for rename matching in one commit.
const RENAME_LIMIT: usize = 1000;

/// Longest stored commit subject, in characters.
const MAX_SUBJECT_CHARS: usize = 200;

/// Failures from reading a repository.
#[derive(Debug, Error)]
pub enum GitError {
    /// The path is not a readable Git repository.
    #[error("{path} is not a Git repository")]
    NotARepository {
        /// The path that was tried.
        path: String,
    },
    /// The reference does not name a commit.
    #[error("{reference:?} does not name a commit")]
    UnknownRef {
        /// The reference that was tried.
        reference: String,
    },
    /// The repository has no commits.
    #[error("the repository has no commits")]
    Empty,
    /// An exclusion glob is invalid.
    #[error("invalid exclusion glob: {0}")]
    InvalidGlob(String),
    /// libgit2 reported an error.
    #[error(transparent)]
    Git(#[from] git2::Error),
}

impl From<GitError> for AtlasError {
    fn from(error: GitError) -> Self {
        match error {
            GitError::NotARepository { .. } | GitError::UnknownRef { .. } | GitError::Empty => {
                Self::InvalidInput(error.to_string())
            }
            GitError::InvalidGlob(_) => Self::Config(error.to_string()),
            GitError::Git(_) => Self::Git(error.to_string()),
        }
    }
}

/// A repository opened for reading.
pub struct GitRepository {
    repository: Repository,
}

/// A regular file in a commit's tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeFile {
    /// Path within the tree.
    pub path: RepoPath,
    /// Blob holding the content.
    pub blob: Oid,
    /// Blob size in bytes.
    pub size: u64,
}

/// The regular files of a tree, sorted by path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TreeListing {
    /// Files with valid paths.
    pub files: Vec<TreeFile>,
    /// Files skipped because their path is not valid UTF-8 or is unsafe.
    pub unreadable_paths: u32,
}

/// A first-parent walk from a tip, newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirstParentHistory {
    /// Commits oldest first, so an index is a first-parent position.
    pub oids: Vec<Oid>,
    /// The limit stopped the walk before a root commit.
    pub is_truncated: bool,
}

impl GitRepository {
    /// Opens exactly the repository at `path`, without searching parent
    /// directories, so a path inside another repository cannot open that one.
    pub fn open(path: &Path) -> Result<Self, GitError> {
        let not_a_repository = || GitError::NotARepository {
            path: path.display().to_string(),
        };
        let canonical = path.canonicalize().map_err(|_| not_a_repository())?;
        let repository = Repository::open_ext(
            &canonical,
            RepositoryOpenFlags::NO_SEARCH,
            std::iter::empty::<&std::ffi::OsStr>(),
        )
        .map_err(|_| not_a_repository())?;
        Ok(Self { repository })
    }

    /// The stable location of this repository: its worktree, or its Git
    /// directory when bare.
    pub fn canonical_path(&self) -> PathBuf {
        self.repository
            .workdir()
            .unwrap_or_else(|| self.repository.path())
            .to_path_buf()
    }

    /// Whether older history is missing because this is a shallow clone.
    pub fn is_shallow(&self) -> bool {
        self.repository.is_shallow()
    }

    /// Resolves a reference, branch, tag or commit id to a commit.
    pub fn resolve(&self, reference: &str) -> Result<Oid, GitError> {
        let commit = self
            .repository
            .revparse_single(reference)
            .and_then(|object| object.peel_to_commit())
            .map_err(|_| match self.repository.head() {
                Err(error) if error.code() == git2::ErrorCode::UnbornBranch => GitError::Empty,
                _ => GitError::UnknownRef {
                    reference: reference.to_owned(),
                },
            })?;
        Ok(to_model_oid(commit.id()))
    }

    /// Walks first parents from `tip`, taking at most `limit` commits.
    pub fn first_parent_history(
        &self,
        tip: &Oid,
        limit: u32,
    ) -> Result<FirstParentHistory, GitError> {
        let mut current = self.commit(tip)?;
        let mut oids = Vec::new();
        let is_truncated = loop {
            oids.push(to_model_oid(current.id()));
            if current.parent_count() == 0 {
                break false;
            }
            if oids.len() >= limit as usize {
                break true;
            }
            current = current.parent(0)?;
        };
        oids.reverse();
        Ok(FirstParentHistory { oids, is_truncated })
    }

    /// Diffs a commit against its first parent, or against nothing for a root.
    /// A merge is therefore one change set holding everything it brought in.
    pub fn commit_changes(
        &self,
        oid: &Oid,
        rules: &ExclusionRules,
    ) -> Result<CommitChanges, GitError> {
        let commit = self.commit(oid)?;
        let record = commit_record(&commit)?;
        let tree = commit.tree()?;
        let parent_tree = match commit.parent_count() {
            0 => None,
            _ => Some(commit.parent(0)?.tree()?),
        };

        let mut options = DiffOptions::new();
        options.max_size(rules.diff_max_size());
        let mut diff = self.repository.diff_tree_to_tree(
            parent_tree.as_ref(),
            Some(&tree),
            Some(&mut options),
        )?;
        let mut find = DiffFindOptions::new();
        find.renames(true)
            .rename_threshold(rules.rename_similarity_percent())
            .rename_limit(RENAME_LIMIT);
        diff.find_similar(Some(&mut find))?;

        let mut changes = Vec::new();
        let mut unreadable_paths = 0;
        for index in 0..diff.deltas().len() {
            let Some(delta) = diff.get_delta(index) else {
                continue;
            };
            let Some(status) = change_status(delta.status()) else {
                continue;
            };

            let old_path = delta.old_file().path_bytes().map(parse_path);
            let new_path = delta.new_file().path_bytes().map(parse_path);
            let (old_path, new_path) = match (status, old_path, new_path) {
                (ChangeStatus::Added, _, Some(Some(new))) => (None, Some(new)),
                (ChangeStatus::Deleted, Some(Some(old)), _) => (Some(old), None),
                (_, Some(Some(old)), Some(Some(new))) => (Some(old), Some(new)),
                _ => {
                    unreadable_paths += 1;
                    continue;
                }
            };

            let side = if status == ChangeStatus::Deleted {
                delta.old_file()
            } else {
                delta.new_file()
            };
            let mut exclusion = match side.mode() {
                FileMode::Link => Some(ExclusionReason::Symlink),
                FileMode::Commit => Some(ExclusionReason::Submodule),
                _ => None,
            }
            .or_else(|| {
                [&old_path, &new_path]
                    .into_iter()
                    .flatten()
                    .any(|path| rules.is_excluded(path))
                    .then_some(ExclusionReason::Rule)
            });
            if exclusion.is_none() && self.blob_size(side.id()) > rules.max_file_bytes() {
                exclusion = Some(ExclusionReason::Oversized);
            }

            let (mut added_lines, mut deleted_lines) = (0, 0);
            if exclusion.is_none()
                && let Some(patch) = Patch::from_diff(&diff, index)?
            {
                if patch.delta().flags().is_binary() {
                    exclusion = Some(ExclusionReason::Binary);
                } else {
                    let (_, added, deleted) = patch.line_stats()?;
                    added_lines = u32::try_from(added).unwrap_or(u32::MAX);
                    deleted_lines = u32::try_from(deleted).unwrap_or(u32::MAX);
                }
            }

            changes.push(FileChange {
                old_path,
                new_path,
                status,
                added_lines,
                deleted_lines,
                exclusion,
            });
        }
        changes.sort_by(|a, b| sort_key(a).cmp(&sort_key(b)));

        Ok(CommitChanges {
            commit: record,
            changes,
            unreadable_paths,
        })
    }

    /// Files that moved between two commits, as `(from, to)`, by diffing their
    /// trees directly. Works for any two commits, related or not.
    pub fn renames_between(
        &self,
        base: &Oid,
        target: &Oid,
        rules: &ExclusionRules,
    ) -> Result<Vec<(RepoPath, RepoPath)>, GitError> {
        let (base, target) = (self.commit(base)?.tree()?, self.commit(target)?.tree()?);
        let mut options = DiffOptions::new();
        options.max_size(rules.diff_max_size());
        let mut diff =
            self.repository
                .diff_tree_to_tree(Some(&base), Some(&target), Some(&mut options))?;
        let mut find = DiffFindOptions::new();
        find.renames(true)
            .rename_threshold(rules.rename_similarity_percent())
            .rename_limit(RENAME_LIMIT);
        diff.find_similar(Some(&mut find))?;

        Ok(diff
            .deltas()
            .filter(|delta| delta.status() == Delta::Renamed)
            .filter_map(|delta| {
                let from = delta.old_file().path_bytes().and_then(parse_path)?;
                let to = delta.new_file().path_bytes().and_then(parse_path)?;
                Some((from, to))
            })
            .collect())
    }

    /// Whether `ancestor` is a strict ancestor of `descendant`.
    pub fn is_ancestor(&self, ancestor: &Oid, descendant: &Oid) -> Result<bool, GitError> {
        let (ancestor, descendant) = (self.commit(ancestor)?.id(), self.commit(descendant)?.id());
        Ok(self.repository.graph_descendant_of(descendant, ancestor)?)
    }

    /// A commit's id, time and subject.
    pub fn commit_summary(&self, oid: &Oid) -> Result<CommitSummary, GitError> {
        let record = commit_record(&self.commit(oid)?)?;
        Ok(CommitSummary {
            oid: record.oid,
            committed_at: record.committed_at,
            subject: record.subject,
        })
    }

    /// Counts the files in a commit's tree.
    pub fn tree_summary(&self, oid: &Oid, rules: &ExclusionRules) -> Result<TreeSummary, GitError> {
        let tree = self.commit(oid)?.tree()?;
        let mut summary = TreeSummary::default();
        tree.walk(TreeWalkMode::PreOrder, |root, entry| {
            let regular_file = entry.kind() == Some(ObjectType::Blob)
                && entry.filemode() != i32::from(FileMode::Link);
            if regular_file {
                summary.file_count += 1;
                let path = entry
                    .name()
                    .ok()
                    .and_then(|name| format!("{root}{name}").parse::<RepoPath>().ok());
                if let Some(path) = path
                    && is_typescript(&path)
                    && !rules.is_excluded(&path)
                {
                    summary.typescript_file_count += 1;
                }
            }
            TreeWalkResult::Ok
        })?;
        Ok(summary)
    }

    /// Lists the regular files in a commit's tree. Symlinks and submodules are
    /// left out because they have no source content here.
    pub fn tree_files(&self, oid: &Oid) -> Result<TreeListing, GitError> {
        let tree = self.commit(oid)?.tree()?;
        let mut listing = TreeListing::default();
        tree.walk(TreeWalkMode::PreOrder, |root, entry| {
            if entry.kind() != Some(ObjectType::Blob)
                || entry.filemode() == i32::from(FileMode::Link)
            {
                return TreeWalkResult::Ok;
            }
            let path = entry
                .name()
                .ok()
                .and_then(|name| format!("{root}{name}").parse::<RepoPath>().ok());
            match path {
                Some(path) => listing.files.push(TreeFile {
                    path,
                    blob: to_model_oid(entry.id()),
                    size: self.blob_size(entry.id()),
                }),
                None => listing.unreadable_paths += 1,
            }
            TreeWalkResult::Ok
        })?;
        listing.files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(listing)
    }

    /// Reads a blob from the object database, so that working tree contents and
    /// content filters play no part.
    pub fn read_blob(&self, oid: &Oid) -> Result<Vec<u8>, GitError> {
        let id = git2::Oid::from_str(oid.as_str())?;
        Ok(self.repository.find_blob(id)?.content().to_vec())
    }

    fn commit(&self, oid: &Oid) -> Result<Commit<'_>, GitError> {
        let id = git2::Oid::from_str(oid.as_str())?;
        Ok(self.repository.find_commit(id)?)
    }

    fn blob_size(&self, id: git2::Oid) -> u64 {
        self.repository
            .odb()
            .and_then(|odb| odb.read_header(id))
            .map_or(0, |(size, _)| size as u64)
    }
}

impl ExclusionRules {
    /// Builds rules from configuration.
    pub fn from_config(analysis: &AnalysisConfig, limits: &Limits) -> Result<Self, GitError> {
        Self::new(
            &analysis.exclude_globs,
            limits.max_file_bytes,
            analysis.rename_similarity_percent,
        )
    }
}

/// True for the source files CodeAtlas parses.
pub fn is_typescript(path: &RepoPath) -> bool {
    let path = path.as_str();
    [".ts", ".tsx", ".mts", ".cts"]
        .iter()
        .any(|extension| path.ends_with(extension))
}

fn to_model_oid(oid: git2::Oid) -> Oid {
    oid.to_string()
        .parse()
        .expect("libgit2 produces well-formed object ids")
}

fn parse_path(bytes: &[u8]) -> Option<RepoPath> {
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

fn change_status(delta: Delta) -> Option<ChangeStatus> {
    match delta {
        Delta::Added => Some(ChangeStatus::Added),
        Delta::Deleted => Some(ChangeStatus::Deleted),
        Delta::Modified => Some(ChangeStatus::Modified),
        Delta::Renamed => Some(ChangeStatus::Renamed),
        Delta::Typechange => Some(ChangeStatus::TypeChanged),
        _ => None,
    }
}

fn sort_key(change: &FileChange) -> (&Option<RepoPath>, &Option<RepoPath>) {
    (&change.new_path, &change.old_path)
}

fn commit_record(commit: &Commit<'_>) -> Result<CommitRecord, GitError> {
    Ok(CommitRecord {
        oid: to_model_oid(commit.id()),
        parents: commit.parent_ids().map(to_model_oid).collect(),
        committed_at: commit.time().seconds(),
        subject: commit
            .summary_bytes()
            .map(clean_subject)
            .unwrap_or_default(),
    })
}

/// Commit messages may hold terminal escape sequences aimed at whoever prints
/// them, and any encoding, so only display-safe text is kept.
fn clean_subject(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .take(MAX_SUBJECT_CHARS)
        .map(|c| if c.is_control() { '\u{FFFD}' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::clean_subject;

    #[test]
    fn subjects_lose_control_characters_and_stay_bounded() {
        assert_eq!(clean_subject(b"fix \x1b[31mred"), "fix \u{FFFD}[31mred");
        assert_eq!(clean_subject(&[b'a'; 500]).len(), 200);
        assert_eq!(clean_subject(b"caf\xe9"), "caf\u{FFFD}");
    }
}
