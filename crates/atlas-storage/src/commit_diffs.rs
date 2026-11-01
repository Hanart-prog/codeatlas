//! Cached commit diffs. See `0006_commit_diffs.sql`.

use atlas_model::history::CommitChanges;
use atlas_model::{Oid, RepositoryId};
use rusqlite::{OptionalExtension, params};

use crate::{Database, StorageError};

impl Database {
    /// The cached diff of a commit, if any.
    pub fn commit_diff(
        &self,
        repository_id: RepositoryId,
        cache_key: &str,
        commit: &Oid,
    ) -> Result<Option<CommitChanges>, StorageError> {
        let json: Option<String> = self
            .connection
            .prepare_cached(
                "SELECT changes_json FROM commit_diffs
                 WHERE repository_id = ?1 AND cache_key = ?2 AND commit_oid = ?3",
            )?
            .query_row(
                params![repository_id.0, cache_key, commit.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        json.map(|json| {
            serde_json::from_str(&json)
                .map_err(|error| StorageError::Corrupt(format!("commit diff: {error}")))
        })
        .transpose()
    }

    /// Caches diffs in one transaction. Diffs never change, so an entry another
    /// process wrote first stands.
    pub fn store_commit_diffs(
        &self,
        repository_id: RepositoryId,
        cache_key: &str,
        diffs: &[&CommitChanges],
    ) -> Result<(), StorageError> {
        let transaction = self.connection.unchecked_transaction()?;
        {
            let mut insert = transaction.prepare_cached(
                "INSERT OR IGNORE INTO commit_diffs
                     (repository_id, commit_oid, cache_key, changes_json)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for diff in diffs {
                let json = serde_json::to_string(diff)
                    .map_err(|error| StorageError::Corrupt(format!("commit diff: {error}")))?;
                insert.execute(params![
                    repository_id.0,
                    diff.commit.oid.as_str(),
                    cache_key,
                    json
                ])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }
}
