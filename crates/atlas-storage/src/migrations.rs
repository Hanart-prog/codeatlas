//! Forward-only schema migrations.
//!
//! All pending migrations apply in one immediate transaction, so a database is
//! never left between versions and concurrent processes cannot both migrate.
//! Each applied migration's checksum is recorded, which catches a shipped
//! migration being edited after the fact.

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use sha2::{Digest, Sha256};

use crate::StorageError;

/// One schema change. Versions must be contiguous, starting at 1.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    /// Position in the sequence.
    pub version: u32,
    /// Short description.
    pub name: &'static str,
    /// The SQL to run.
    pub sql: &'static str,
}

impl Migration {
    fn checksum(&self) -> String {
        Sha256::digest(self.sql.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

/// Every migration, in order. Add to the end; never edit a shipped entry.
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "repositories_and_runs",
        sql: include_str!("../migrations/0001_repositories_and_runs.sql"),
    },
    Migration {
        version: 2,
        name: "git_history",
        sql: include_str!("../migrations/0002_git_history.sql"),
    },
    Migration {
        version: 3,
        name: "import_snapshots",
        sql: include_str!("../migrations/0003_import_snapshots.sql"),
    },
    Migration {
        version: 4,
        name: "backtests",
        sql: include_str!("../migrations/0004_backtests.sql"),
    },
    Migration {
        version: 5,
        name: "run_order",
        sql: include_str!("../migrations/0005_run_order.sql"),
    },
];

/// What a migration pass did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MigrationReport {
    /// Schema version before the pass.
    pub from_version: u32,
    /// Schema version after the pass.
    pub to_version: u32,
}

/// The latest version this build knows about.
pub fn latest_version() -> u32 {
    MIGRATIONS.last().map_or(0, |migration| migration.version)
}

const CREATE_LEDGER: &str = "CREATE TABLE IF NOT EXISTS schema_migrations (
    version    INTEGER PRIMARY KEY,
    name       TEXT    NOT NULL,
    checksum   TEXT    NOT NULL,
    applied_at INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT";

pub(crate) fn current_version(connection: &Connection) -> Result<u32, StorageError> {
    let version = connection.query_row(
        "SELECT coalesce(max(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;
    Ok(version)
}

/// Whether every migration is applied unchanged. This takes no write lock, so that
/// opening a connection never waits for, or fails on, a long write elsewhere.
fn is_current(connection: &Connection, migrations: &[Migration]) -> Result<bool, StorageError> {
    let has_ledger: bool = connection.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE name = 'schema_migrations')",
        [],
        |row| row.get(0),
    )?;
    if !has_ledger {
        return Ok(false);
    }
    let mut statement =
        connection.prepare("SELECT version, checksum FROM schema_migrations ORDER BY version")?;
    let applied = statement
        .query_map([], |row| {
            Ok((row.get::<_, u32>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(applied.len() == migrations.len()
        && applied
            .iter()
            .zip(migrations)
            .all(|((version, checksum), migration)| {
                *version == migration.version && *checksum == migration.checksum()
            }))
}

pub(crate) fn apply(
    connection: &mut Connection,
    migrations: &[Migration],
) -> Result<MigrationReport, StorageError> {
    if is_current(connection, migrations)? {
        let version = migrations.last().map_or(0, |migration| migration.version);
        return Ok(MigrationReport {
            from_version: version,
            to_version: version,
        });
    }
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute(CREATE_LEDGER, [])?;

    let from_version = current_version(&transaction)?;
    let supported = migrations.last().map_or(0, |m| m.version);
    if from_version > supported {
        return Err(StorageError::SchemaTooNew {
            found: from_version,
            supported,
        });
    }

    for migration in migrations {
        let recorded: Option<String> = transaction
            .query_row(
                "SELECT checksum FROM schema_migrations WHERE version = ?1",
                [migration.version],
                |row| row.get(0),
            )
            .optional()?;
        let checksum = migration.checksum();
        match recorded {
            Some(recorded) if recorded == checksum => {}
            Some(_) => {
                return Err(StorageError::MigrationChanged {
                    version: migration.version,
                    name: migration.name.to_owned(),
                });
            }
            None => {
                transaction.execute_batch(migration.sql)?;
                transaction.execute(
                    "INSERT INTO schema_migrations (version, name, checksum) VALUES (?1, ?2, ?3)",
                    params![migration.version, migration.name, checksum],
                )?;
            }
        }
    }

    let to_version = current_version(&transaction)?;
    transaction.commit()?;
    Ok(MigrationReport {
        from_version,
        to_version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRST: Migration = Migration {
        version: 1,
        name: "first",
        sql: "CREATE TABLE first (id INTEGER PRIMARY KEY);",
    };
    const SECOND: Migration = Migration {
        version: 2,
        name: "second",
        sql: "CREATE TABLE second (id INTEGER PRIMARY KEY);",
    };
    const FAILS_HALFWAY: Migration = Migration {
        version: 2,
        name: "fails_halfway",
        sql: "CREATE TABLE partial (id INTEGER PRIMARY KEY); SELECT * FROM missing_table;",
    };

    fn table_exists(connection: &Connection, name: &str) -> bool {
        connection
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [name],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
            == 1
    }

    #[test]
    fn shipped_migrations_are_contiguous_and_named() {
        for (index, migration) in MIGRATIONS.iter().enumerate() {
            assert_eq!(migration.version as usize, index + 1);
            assert!(!migration.name.is_empty());
        }
    }

    #[test]
    fn applying_twice_is_a_no_op() {
        let mut connection = Connection::open_in_memory().unwrap();
        let first = apply(&mut connection, &[FIRST, SECOND]).unwrap();
        assert_eq!((first.from_version, first.to_version), (0, 2));

        let second = apply(&mut connection, &[FIRST, SECOND]).unwrap();
        assert_eq!((second.from_version, second.to_version), (2, 2));
    }

    #[test]
    fn only_pending_migrations_run() {
        let mut connection = Connection::open_in_memory().unwrap();
        apply(&mut connection, &[FIRST]).unwrap();
        let report = apply(&mut connection, &[FIRST, SECOND]).unwrap();
        assert_eq!((report.from_version, report.to_version), (1, 2));
        assert!(table_exists(&connection, "second"));
    }

    #[test]
    fn a_failing_migration_rolls_everything_back() {
        let mut connection = Connection::open_in_memory().unwrap();
        assert!(apply(&mut connection, &[FIRST, FAILS_HALFWAY]).is_err());

        assert!(!table_exists(&connection, "first"));
        assert!(!table_exists(&connection, "partial"));
        assert!(!table_exists(&connection, "schema_migrations"));
    }

    #[test]
    fn a_failing_upgrade_keeps_the_existing_version() {
        let mut connection = Connection::open_in_memory().unwrap();
        apply(&mut connection, &[FIRST]).unwrap();
        assert!(apply(&mut connection, &[FIRST, FAILS_HALFWAY]).is_err());

        assert_eq!(current_version(&connection).unwrap(), 1);
        assert!(table_exists(&connection, "first"));
        assert!(!table_exists(&connection, "partial"));
    }

    #[test]
    fn a_database_from_a_newer_build_is_refused() {
        let mut connection = Connection::open_in_memory().unwrap();
        apply(&mut connection, &[FIRST, SECOND]).unwrap();
        let error = apply(&mut connection, &[FIRST]).unwrap_err();
        assert!(matches!(
            error,
            StorageError::SchemaTooNew {
                found: 2,
                supported: 1
            }
        ));
    }

    #[test]
    fn an_edited_migration_is_detected() {
        let mut connection = Connection::open_in_memory().unwrap();
        apply(&mut connection, &[FIRST]).unwrap();
        let edited = Migration {
            sql: "CREATE TABLE first (id INTEGER PRIMARY KEY, extra TEXT);",
            ..FIRST
        };
        let error = apply(&mut connection, &[edited]).unwrap_err();
        assert!(matches!(
            error,
            StorageError::MigrationChanged { version: 1, .. }
        ));
    }
}
