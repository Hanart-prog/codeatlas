//! SQLite persistence for CodeAtlas.
//!
//! The database holds derived analysis data and lives outside analyzed
//! repositories. It can always be rebuilt, so it is a cache, not a source of truth.

mod backtests;
mod commit_diffs;
mod history;
mod migrations;
mod snapshots;

use std::path::Path;
use std::time::{Duration, Instant};

use atlas_model::{AtlasError, Work};
use rusqlite::{Connection, ErrorCode, OpenFlags};
use thiserror::Error;

pub use history::{IdentityRow, RunKey, RunRow, StoredChange};
pub use migrations::{MIGRATIONS, Migration, MigrationReport, latest_version};
pub use snapshots::{SnapshotId, SnapshotKey};

/// How long a connection waits on a lock held by another connection.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Pause between attempts when SQLite reports busy without waiting.
const BUSY_RETRY_DELAY: Duration = Duration::from_millis(10);

/// Failures from the storage layer.
#[derive(Debug, Error)]
pub enum StorageError {
    /// SQLite reported an error.
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    /// The database was written by a newer build than this one.
    #[error("database schema {found} is newer than the {supported} this build supports")]
    SchemaTooNew {
        /// Version found in the database.
        found: u32,
        /// Latest version this build knows.
        supported: u32,
    },
    /// Stored data failed validation.
    #[error("corrupt database: {0}")]
    Corrupt(String),
    /// An applied migration no longer matches the one in this build.
    #[error("migration {version} ({name}) differs from the one applied to this database")]
    MigrationChanged {
        /// Version of the migration.
        version: u32,
        /// Name of the migration.
        name: String,
    },
}

impl From<StorageError> for AtlasError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error.to_string())
    }
}

/// A migrated SQLite connection.
#[derive(Debug)]
pub struct Database {
    connection: Connection,
    work: Work,
}

impl Database {
    /// Opens or creates the database at `path` and brings it up to date.
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let mut connection = Connection::open_with_flags(path, flags)?;
        connection.busy_timeout(BUSY_TIMEOUT)?;
        retry_while_busy(|| {
            let journal_mode: String =
                connection.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
            tracing::debug!(?path, journal_mode, "opened database");
            Ok(())
        })?;
        retry_while_busy(|| Self::migrate(&mut connection))?;
        Ok(Self {
            connection,
            work: Work::default(),
        })
    }

    /// Opens a private in-memory database, used by tests.
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let mut connection = Connection::open_in_memory()?;
        Self::migrate(&mut connection)?;
        Ok(Self {
            connection,
            work: Work::default(),
        })
    }

    fn migrate(connection: &mut Connection) -> Result<(), StorageError> {
        connection.pragma_update(None, "foreign_keys", true)?;
        let report = migrations::apply(connection, MIGRATIONS)?;
        tracing::debug!(?report, "database migrated");
        Ok(())
    }

    /// The latest migration applied to this database.
    pub fn schema_version(&self) -> Result<u32, StorageError> {
        migrations::current_version(&self.connection)
    }

    /// Everything long passes through a database session, so this is where it
    /// finds out whether to stop and where to report. Defaults to no limits.
    pub fn work(&self) -> &Work {
        &self.work
    }

    /// Sets the controls for work done through this connection.
    pub fn set_work(&mut self, work: Work) {
        self.work = work;
    }

    /// The underlying connection.
    pub fn connection(&self) -> &Connection {
        &self.connection
    }
}

/// Retries `operation` until it stops reporting a locked database. SQLite skips
/// the busy timeout where waiting could deadlock, which includes switching to WAL,
/// so concurrent first opens of a new database need this on top of the timeout.
fn retry_while_busy(
    mut operation: impl FnMut() -> Result<(), StorageError>,
) -> Result<(), StorageError> {
    let started = Instant::now();
    loop {
        match operation() {
            Err(StorageError::Sqlite(rusqlite::Error::SqliteFailure(failure, _)))
                if failure.code == ErrorCode::DatabaseBusy && started.elapsed() < BUSY_TIMEOUT =>
            {
                std::thread::sleep(BUSY_RETRY_DELAY);
            }
            result => return result,
        }
    }
}

#[cfg(test)]
mod tests {
    use atlas_model::AnalysisStatus;
    use rusqlite::params;

    use super::*;

    fn insert_repository(db: &Database) -> i64 {
        db.connection()
            .execute(
                "INSERT INTO repositories (canonical_path, created_at) VALUES ('/repo', 0)",
                [],
            )
            .unwrap();
        db.connection().last_insert_rowid()
    }

    fn insert_run(db: &Database, repository_id: i64, status: &str) -> rusqlite::Result<usize> {
        db.connection().execute(
            "INSERT INTO analysis_runs
                (repository_id, target_oid, status, parser_version, algorithm_version, config_hash,
                 history_limit)
             VALUES (?1, 'oid', ?2, 0, 0, 'hash', 500)",
            params![repository_id, status],
        )
    }

    #[test]
    fn opening_does_not_wait_for_a_writer() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("atlas.sqlite");
        let writer = Database::open(&path).unwrap();
        writer
            .connection()
            .execute_batch("BEGIN IMMEDIATE")
            .unwrap();

        let started = std::time::Instant::now();
        let reader = Database::open(&path).unwrap();
        assert_eq!(reader.schema_version().unwrap(), latest_version());
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    #[test]
    fn new_database_reaches_the_latest_schema() {
        let db = Database::open_in_memory().unwrap();
        assert_eq!(db.schema_version().unwrap(), latest_version());
    }

    #[test]
    fn file_database_uses_wal_and_survives_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("atlas.sqlite");

        let db = Database::open(&path).unwrap();
        let mode: String = db
            .connection()
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
        insert_repository(&db);
        drop(db);

        let reopened = Database::open(&path).unwrap();
        let count: i64 = reopened
            .connection()
            .query_row("SELECT count(*) FROM repositories", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn concurrent_opens_all_succeed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("atlas.sqlite");
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let path = path.clone();
                std::thread::spawn(move || Database::open(&path).map(|db| db.schema_version()))
            })
            .collect();
        for handle in handles {
            assert_eq!(handle.join().unwrap().unwrap().unwrap(), latest_version());
        }
    }

    #[test]
    fn foreign_keys_are_enforced_and_cascade() {
        let db = Database::open_in_memory().unwrap();
        assert!(insert_run(&db, 999, "queued").is_err());

        let repository_id = insert_repository(&db);
        insert_run(&db, repository_id, "queued").unwrap();
        db.connection()
            .execute("DELETE FROM repositories", [])
            .unwrap();
        let runs: i64 = db
            .connection()
            .query_row("SELECT count(*) FROM analysis_runs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(runs, 0);
    }

    #[test]
    fn status_constraint_matches_the_shared_enum() {
        let db = Database::open_in_memory().unwrap();
        let repository_id = insert_repository(&db);
        for status in AnalysisStatus::ALL {
            insert_run(&db, repository_id, status.as_str()).unwrap();
        }
        assert!(insert_run(&db, repository_id, "paused").is_err());
    }

    #[test]
    fn only_one_completed_run_may_exist_per_cache_key() {
        let db = Database::open_in_memory().unwrap();
        let repository_id = insert_repository(&db);
        insert_run(&db, repository_id, "failed").unwrap();
        insert_run(&db, repository_id, "failed").unwrap();
        insert_run(&db, repository_id, "completed").unwrap();
        assert!(insert_run(&db, repository_id, "completed").is_err());
    }
}
