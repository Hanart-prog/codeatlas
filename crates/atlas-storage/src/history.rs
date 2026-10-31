//! Persistence of indexed Git history.

use std::path::Path;

use atlas_model::history::{ChangeStatus, ExclusionReason, IndexedHistory};
use atlas_model::{
    AnalysisRunId, AnalysisStatus, CommitEntry, CommitPage, CommitSummary, ExclusionCount, Oid,
    RepoPath, RepositoryEntry, RepositoryId,
};
use rusqlite::{OptionalExtension, Row, params};

use crate::{Database, StorageError};

/// A completed analysis run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRow {
    /// Run identifier.
    pub id: AnalysisRunId,
    /// Commit analyzed.
    pub target_oid: Oid,
    /// Requested history limit.
    pub history_limit: u32,
    /// Unix seconds when the run finished.
    pub completed_at: Option<i64>,
    /// Parser version.
    pub parser_version: u32,
    /// Algorithm version.
    pub algorithm_version: u32,
    /// Hash of the settings that affect results.
    pub config_hash: String,
    /// The clone was shallow.
    pub is_shallow: bool,
    /// The limit cut off older history.
    pub is_truncated: bool,
    /// Changes dropped for unreadable paths.
    pub unreadable_path_count: u32,
    /// Regular files at the target.
    pub tip_file_count: u32,
    /// TS/TSX files at the target.
    pub tip_typescript_file_count: u32,
}

/// The identifying fields of a run, which double as its cache key.
#[derive(Debug, Clone, Copy)]
pub struct RunKey<'a> {
    /// Repository.
    pub repository_id: RepositoryId,
    /// Commit to analyze.
    pub target_oid: &'a Oid,
    /// History limit.
    pub history_limit: u32,
    /// Parser version.
    pub parser_version: u32,
    /// Algorithm version.
    pub algorithm_version: u32,
    /// Hash of the settings that affect results.
    pub config_hash: &'a str,
}

/// A file identity with its path history, oldest path first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityRow {
    /// Index within the run.
    pub run_index: usize,
    /// Paths held, oldest first.
    pub paths: Vec<RepoPath>,
    /// Whether the identity still has a current path at the run's target.
    pub exists_at_tip: bool,
}

/// One stored change, reduced to what metrics need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredChange {
    /// Index of the identity within the run.
    pub identity: usize,
    /// First-parent position of the commit.
    pub position: u32,
    /// What happened.
    pub status: ChangeStatus,
    /// Lines added.
    pub added_lines: u32,
    /// Lines deleted.
    pub deleted_lines: u32,
    /// Why the change is left out of metrics, if it is.
    pub exclusion: Option<ExclusionReason>,
}

const RUN_COLUMNS: &str = "id, target_oid, history_limit, completed_at, parser_version,
    algorithm_version, config_hash, is_shallow, is_truncated, unreadable_path_count,
    tip_file_count, tip_typescript_file_count";

fn oid_column(row: &Row<'_>, index: usize) -> rusqlite::Result<Oid> {
    let text: String = row.get(index)?;
    text.parse().map_err(|error: atlas_model::AtlasError| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            error.to_string().into(),
        )
    })
}

fn run_row(row: &Row<'_>) -> rusqlite::Result<RunRow> {
    Ok(RunRow {
        id: AnalysisRunId(row.get(0)?),
        target_oid: oid_column(row, 1)?,
        history_limit: row.get(2)?,
        completed_at: row.get(3)?,
        parser_version: row.get(4)?,
        algorithm_version: row.get(5)?,
        config_hash: row.get(6)?,
        is_shallow: row.get(7)?,
        is_truncated: row.get(8)?,
        unreadable_path_count: row.get(9)?,
        tip_file_count: row.get(10)?,
        tip_typescript_file_count: row.get(11)?,
    })
}

fn repository_row(row: &Row<'_>) -> rusqlite::Result<RepositoryEntry> {
    Ok(RepositoryEntry {
        id: RepositoryId(row.get(0)?),
        path: row.get(1)?,
        analyzed_at: row.get(2)?,
    })
}

impl Database {
    /// Registers a repository, or returns the existing row for that path.
    pub fn upsert_repository(&self, path: &Path) -> Result<RepositoryEntry, StorageError> {
        let path = path.to_string_lossy();
        self.connection.execute(
            "INSERT INTO repositories (canonical_path, created_at) VALUES (?1, unixepoch())
             ON CONFLICT (canonical_path) DO NOTHING",
            [path.as_ref()],
        )?;
        let entry = self.connection.query_row(
            "SELECT id, canonical_path, analyzed_at FROM repositories WHERE canonical_path = ?1",
            [path.as_ref()],
            repository_row,
        )?;
        Ok(entry)
    }

    /// Looks up a repository by id.
    pub fn repository(&self, id: RepositoryId) -> Result<Option<RepositoryEntry>, StorageError> {
        let entry = self
            .connection
            .query_row(
                "SELECT id, canonical_path, analyzed_at FROM repositories WHERE id = ?1",
                [id.0],
                repository_row,
            )
            .optional()?;
        Ok(entry)
    }

    /// Looks up a repository by canonical path.
    pub fn repository_by_path(&self, path: &Path) -> Result<Option<RepositoryEntry>, StorageError> {
        let entry = self
            .connection
            .query_row(
                "SELECT id, canonical_path, analyzed_at FROM repositories
                 WHERE canonical_path = ?1",
                [path.to_string_lossy().as_ref()],
                repository_row,
            )
            .optional()?;
        Ok(entry)
    }

    /// All opened repositories, by path.
    pub fn repositories(&self) -> Result<Vec<RepositoryEntry>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, canonical_path, analyzed_at FROM repositories ORDER BY canonical_path",
        )?;
        let rows = statement.query_map([], repository_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Finds a completed run for exactly this key.
    pub fn cached_run(&self, key: &RunKey<'_>) -> Result<Option<RunRow>, StorageError> {
        let run = self
            .connection
            .query_row(
                &format!(
                    "SELECT {RUN_COLUMNS} FROM analysis_runs
                     WHERE status = 'completed' AND repository_id = ?1 AND target_oid = ?2
                       AND history_limit = ?3 AND parser_version = ?4
                       AND algorithm_version = ?5 AND config_hash = ?6"
                ),
                params![
                    key.repository_id.0,
                    key.target_oid.as_str(),
                    key.history_limit,
                    key.parser_version,
                    key.algorithm_version,
                    key.config_hash,
                ],
                run_row,
            )
            .optional()?;
        Ok(run)
    }

    /// Records that an analysis has started.
    pub fn begin_run(&self, key: &RunKey<'_>) -> Result<AnalysisRunId, StorageError> {
        self.connection.execute(
            "INSERT INTO analysis_runs (repository_id, target_oid, status, started_at,
                 parser_version, algorithm_version, config_hash, history_limit)
             VALUES (?1, ?2, ?3, unixepoch(), ?4, ?5, ?6, ?7)",
            params![
                key.repository_id.0,
                key.target_oid.as_str(),
                AnalysisStatus::Running.as_str(),
                key.parser_version,
                key.algorithm_version,
                key.config_hash,
                key.history_limit,
            ],
        )?;
        Ok(AnalysisRunId(self.connection.last_insert_rowid()))
    }

    /// Records that an analysis failed. Failed runs hold no history.
    pub fn fail_run(&self, run: AnalysisRunId, error: &str) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE analysis_runs SET status = ?1, completed_at = unixepoch(), error = ?2
             WHERE id = ?3",
            params![AnalysisStatus::Failed.as_str(), error, run.0],
        )?;
        Ok(())
    }

    /// Writes all history for a run and marks it complete, atomically.
    pub fn complete_run(
        &mut self,
        repository_id: RepositoryId,
        run: AnalysisRunId,
        history: &IndexedHistory,
    ) -> Result<(), StorageError> {
        let transaction = self.connection.transaction()?;
        {
            let mut insert_commit = transaction.prepare(
                "INSERT INTO commits (repository_id, oid, committed_at, message_subject, parent_count)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (repository_id, oid) DO NOTHING",
            )?;
            let mut insert_parent = transaction.prepare(
                "INSERT INTO commit_parents (repository_id, child_oid, parent_oid, parent_index)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT DO NOTHING",
            )?;
            let mut insert_position = transaction.prepare(
                "INSERT INTO run_commits (analysis_run_id, position, repository_id, commit_oid)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (position, commit) in history.commits.iter().enumerate() {
                insert_commit.execute(params![
                    repository_id.0,
                    commit.oid.as_str(),
                    commit.committed_at,
                    commit.subject,
                    commit.parents.len() as i64,
                ])?;
                for (index, parent) in commit.parents.iter().enumerate() {
                    insert_parent.execute(params![
                        repository_id.0,
                        commit.oid.as_str(),
                        parent.as_str(),
                        index as i64,
                    ])?;
                }
                insert_position.execute(params![
                    run.0,
                    position as i64,
                    repository_id.0,
                    commit.oid.as_str(),
                ])?;
            }

            let mut insert_identity = transaction.prepare(
                "INSERT INTO file_identities
                     (analysis_run_id, run_index, first_seen_position, canonical_label)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            let mut insert_path = transaction.prepare(
                "INSERT INTO file_paths
                     (file_identity_id, path, valid_from_position, valid_to_position)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            let mut row_ids = Vec::with_capacity(history.identities.len());
            for (index, identity) in history.identities.iter().enumerate() {
                let label = identity.spans.last().map(|span| span.path.as_str());
                insert_identity.execute(params![
                    run.0,
                    index as i64,
                    identity.first_seen,
                    label
                ])?;
                let row_id = transaction.last_insert_rowid();
                row_ids.push(row_id);
                for span in &identity.spans {
                    insert_path.execute(params![
                        row_id,
                        span.path.as_str(),
                        span.valid_from,
                        span.valid_to,
                    ])?;
                }
            }

            let mut insert_change = transaction.prepare(
                "INSERT INTO file_changes (analysis_run_id, position, file_identity_id, old_path,
                     new_path, status, added_lines, deleted_lines, is_merge, exclusion)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )?;
            for change in &history.changes {
                let commit = &history.commits[change.position as usize];
                let file = &change.change;
                insert_change.execute(params![
                    run.0,
                    change.position,
                    row_ids[change.identity],
                    file.old_path.as_ref().map(RepoPath::as_str),
                    file.new_path.as_ref().map(RepoPath::as_str),
                    file.status.as_str(),
                    file.added_lines,
                    file.deleted_lines,
                    commit.parents.len() > 1,
                    file.exclusion.map(ExclusionReason::as_str),
                ])?;
            }

            transaction.execute(
                "UPDATE analysis_runs
                 SET status = ?1, completed_at = unixepoch(), is_shallow = ?2, is_truncated = ?3,
                     unreadable_path_count = ?4, tip_file_count = ?5,
                     tip_typescript_file_count = ?6
                 WHERE id = ?7",
                params![
                    AnalysisStatus::Completed.as_str(),
                    history.is_shallow,
                    history.is_truncated,
                    history.unreadable_paths,
                    history.tree.file_count,
                    history.tree.typescript_file_count,
                    run.0,
                ],
            )?;
            transaction.execute(
                "UPDATE repositories SET head_oid = (
                     SELECT target_oid FROM analysis_runs WHERE id = ?1
                 ), analyzed_at = unixepoch() WHERE id = ?2",
                params![run.0, repository_id.0],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// The most recently completed run of a repository.
    pub fn latest_run(&self, repository_id: RepositoryId) -> Result<Option<RunRow>, StorageError> {
        let run = self
            .connection
            .query_row(
                &format!(
                    "SELECT {RUN_COLUMNS} FROM analysis_runs
                     WHERE repository_id = ?1 AND status = 'completed'
                     ORDER BY completed_at DESC, id DESC LIMIT 1"
                ),
                [repository_id.0],
                run_row,
            )
            .optional()?;
        Ok(run)
    }

    /// Commit counts for a run: all, and merges.
    pub fn commit_counts(&self, run: AnalysisRunId) -> Result<(u32, u32), StorageError> {
        let counts = self.connection.query_row(
            "SELECT count(*), coalesce(sum(c.parent_count > 1), 0)
             FROM run_commits r
             JOIN commits c ON c.repository_id = r.repository_id AND c.oid = r.commit_oid
             WHERE r.analysis_run_id = ?1",
            [run.0],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok(counts)
    }

    /// The commit at a first-parent position.
    pub fn commit_at(
        &self,
        run: AnalysisRunId,
        position: u32,
    ) -> Result<Option<CommitSummary>, StorageError> {
        let commit = self
            .connection
            .query_row(
                "SELECT c.oid, c.committed_at, c.message_subject
                 FROM run_commits r
                 JOIN commits c ON c.repository_id = r.repository_id AND c.oid = r.commit_oid
                 WHERE r.analysis_run_id = ?1 AND r.position = ?2",
                params![run.0, position],
                |row| {
                    Ok(CommitSummary {
                        oid: oid_column(row, 0)?,
                        committed_at: row.get(1)?,
                        subject: row.get(2)?,
                    })
                },
            )
            .optional()?;
        Ok(commit)
    }

    /// A page of the first-parent timeline, newest first. `cursor` is the
    /// position to start from.
    pub fn commit_page(
        &self,
        run: AnalysisRunId,
        cursor: Option<u32>,
        limit: u32,
    ) -> Result<CommitPage, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT r.position, c.oid, c.committed_at, c.message_subject, c.parent_count
             FROM run_commits r
             JOIN commits c ON c.repository_id = r.repository_id AND c.oid = r.commit_oid
             WHERE r.analysis_run_id = ?1 AND r.position <= ?2
             ORDER BY r.position DESC LIMIT ?3",
        )?;
        let rows = statement.query_map(
            params![run.0, cursor.unwrap_or(u32::MAX), limit + 1],
            |row| {
                Ok(CommitEntry {
                    position: row.get(0)?,
                    oid: oid_column(row, 1)?,
                    committed_at: row.get(2)?,
                    subject: row.get(3)?,
                    parent_count: row.get(4)?,
                })
            },
        )?;
        let mut commits: Vec<_> = rows.collect::<Result<_, _>>()?;
        let next_cursor = if commits.len() > limit as usize {
            commits.pop().map(|entry| entry.position)
        } else {
            None
        };
        Ok(CommitPage {
            commits,
            next_cursor,
        })
    }

    /// Every identity of a run with its paths.
    pub fn identities(&self, run: AnalysisRunId) -> Result<Vec<IdentityRow>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT i.run_index, p.path, p.valid_to_position
             FROM file_identities i
             JOIN file_paths p ON p.file_identity_id = i.id
             WHERE i.analysis_run_id = ?1
             ORDER BY i.run_index, p.valid_from_position",
        )?;
        let rows = statement.query_map([run.0], |row| {
            Ok((
                row.get::<_, i64>(0)? as usize,
                row.get::<_, String>(1)?,
                row.get::<_, Option<u32>>(2)?,
            ))
        })?;

        let mut identities: Vec<IdentityRow> = Vec::new();
        for row in rows {
            let (run_index, path, valid_to) = row?;
            let path = path
                .parse()
                .map_err(|_| StorageError::Corrupt(format!("invalid stored path {path:?}")))?;
            if identities.last().is_none_or(|i| i.run_index != run_index) {
                identities.push(IdentityRow {
                    run_index,
                    paths: Vec::new(),
                    exists_at_tip: false,
                });
            }
            if let Some(identity) = identities.last_mut() {
                identity.paths.push(path);
                identity.exists_at_tip = valid_to.is_none();
            }
        }
        Ok(identities)
    }

    /// Every change of a run in position order.
    pub fn changes(&self, run: AnalysisRunId) -> Result<Vec<StoredChange>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT i.run_index, c.position, c.status, c.added_lines, c.deleted_lines, c.exclusion
             FROM file_changes c
             JOIN file_identities i ON i.id = c.file_identity_id
             WHERE c.analysis_run_id = ?1
             ORDER BY c.position, c.id",
        )?;
        let rows = statement.query_map([run.0], |row| {
            let status: String = row.get(2)?;
            let exclusion: Option<String> = row.get(5)?;
            Ok((
                row.get::<_, i64>(0)? as usize,
                row.get(1)?,
                status,
                row.get(3)?,
                row.get(4)?,
                exclusion,
            ))
        })?;
        rows.map(|row| {
            let (identity, position, status, added_lines, deleted_lines, exclusion) = row?;
            Ok(StoredChange {
                identity,
                position,
                status: ChangeStatus::parse(&status)
                    .ok_or_else(|| StorageError::Corrupt(format!("invalid status {status:?}")))?,
                added_lines,
                deleted_lines,
                exclusion: exclusion.as_deref().and_then(ExclusionReason::parse),
            })
        })
        .collect()
    }

    /// Excluded file changes of a run, by reason.
    pub fn exclusion_counts(
        &self,
        run: AnalysisRunId,
    ) -> Result<Vec<ExclusionCount>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT exclusion, count(*) FROM file_changes
             WHERE analysis_run_id = ?1 AND exclusion IS NOT NULL
             GROUP BY exclusion ORDER BY exclusion",
        )?;
        let rows = statement.query_map([run.0], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
        })?;
        rows.map(|row| {
            let (reason, count) = row?;
            let reason = ExclusionReason::parse(&reason)
                .ok_or_else(|| StorageError::Corrupt(format!("invalid reason {reason:?}")))?;
            Ok(ExclusionCount { reason, count })
        })
        .collect()
    }
}
