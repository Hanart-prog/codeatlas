//! Persistence of import snapshots and the blob parse cache.

use atlas_model::{
    GraphEdge, GraphNode, Language, Oid, ParsedFile, ParserCoverage, RepoPath, RepositoryId,
    SnapshotData, UnresolvedImport,
};
use rusqlite::{OptionalExtension, Row, params};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::{Database, StorageError};

/// Row id of a snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotId(pub i64);

/// What identifies a snapshot.
#[derive(Debug, Clone, Copy)]
pub struct SnapshotKey<'a> {
    /// Repository.
    pub repository_id: RepositoryId,
    /// Commit.
    pub commit: &'a Oid,
    /// Parser version.
    pub parser_version: u32,
    /// Hash of the settings that affect results.
    pub config_hash: &'a str,
}

/// Enums are stored as their serde names, the same text the API sends.
fn enum_text<T: Serialize>(value: &T) -> Result<String, StorageError> {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(text)) => Ok(text),
        _ => Err(StorageError::Corrupt("expected a string enum".to_owned())),
    }
}

fn parse_column<T: DeserializeOwned>(row: &Row<'_>, index: usize) -> rusqlite::Result<T> {
    let text: String = row.get(index)?;
    serde_json::from_value(serde_json::Value::String(text)).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            error.to_string().into(),
        )
    })
}

fn json_text<T: Serialize>(value: &T) -> Result<String, StorageError> {
    serde_json::to_string(value).map_err(|error| StorageError::Corrupt(error.to_string()))
}

impl Database {
    /// A cached parse of a blob. A row that no longer decodes counts as a miss.
    pub fn parsed_blob(
        &self,
        blob: &Oid,
        parser_version: u32,
        language: Language,
    ) -> Result<Option<ParsedFile>, StorageError> {
        let json: Option<String> = self
            .connection
            .query_row(
                "SELECT parsed_json FROM parsed_blobs
                 WHERE blob_oid = ?1 AND parser_version = ?2 AND language = ?3",
                params![blob.as_str(), parser_version, enum_text(&language)?],
                |row| row.get(0),
            )
            .optional()?;
        Ok(json.and_then(|json| serde_json::from_str(&json).ok()))
    }

    /// Caches a parse of a blob.
    pub fn store_parsed_blob(
        &self,
        blob: &Oid,
        parser_version: u32,
        language: Language,
        parsed: &ParsedFile,
    ) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO parsed_blobs (blob_oid, parser_version, language, parsed_json)
             VALUES (?1, ?2, ?3, ?4) ON CONFLICT DO NOTHING",
            params![
                blob.as_str(),
                parser_version,
                enum_text(&language)?,
                json_text(parsed)?,
            ],
        )?;
        Ok(())
    }

    /// Finds a stored snapshot.
    pub fn snapshot_id(&self, key: &SnapshotKey<'_>) -> Result<Option<SnapshotId>, StorageError> {
        let id = self
            .connection
            .query_row(
                "SELECT id FROM snapshots WHERE repository_id = ?1 AND commit_oid = ?2
                   AND parser_version = ?3 AND config_hash = ?4",
                params![
                    key.repository_id.0,
                    key.commit.as_str(),
                    key.parser_version,
                    key.config_hash,
                ],
                |row| row.get(0),
            )
            .optional()?;
        Ok(id.map(SnapshotId))
    }

    /// Stores a snapshot atomically. If a concurrent writer stored the same
    /// snapshot first, returns that one, since their contents are identical.
    pub fn store_snapshot(
        &self,
        key: &SnapshotKey<'_>,
        data: &SnapshotData,
    ) -> Result<SnapshotId, StorageError> {
        let transaction = self.connection.unchecked_transaction()?;
        let inserted = transaction.execute(
            "INSERT INTO snapshots (repository_id, commit_oid, parser_version, config_hash,
                 coverage_json)
             VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT DO NOTHING",
            params![
                key.repository_id.0,
                key.commit.as_str(),
                key.parser_version,
                key.config_hash,
                json_text(&data.coverage)?,
            ],
        )?;
        if inserted == 0 {
            drop(transaction);
            return self
                .snapshot_id(key)?
                .ok_or_else(|| StorageError::Corrupt("snapshot vanished".to_owned()));
        }
        let id = transaction.last_insert_rowid();

        {
            let mut insert_file = transaction.prepare(
                "INSERT INTO snapshot_files (snapshot_id, path, language, blob_oid, loc,
                     parse_status, external_import_count)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for file in &data.files {
                insert_file.execute(params![
                    id,
                    file.path.as_str(),
                    enum_text(&file.language)?,
                    file.blob.as_str(),
                    file.loc,
                    enum_text(&file.parse_status)?,
                    file.external_import_count,
                ])?;
            }

            let mut insert_edge = transaction.prepare(
                "INSERT INTO dependency_edges (snapshot_id, source_path, target_path, edge_type,
                     resolution, specifier, line)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for edge in &data.edges {
                insert_edge.execute(params![
                    id,
                    edge.source.as_str(),
                    edge.target.as_str(),
                    enum_text(&edge.edge_type)?,
                    enum_text(&edge.resolution)?,
                    edge.specifier,
                    edge.line,
                ])?;
            }

            let mut insert_unresolved = transaction.prepare(
                "INSERT INTO unresolved_imports (snapshot_id, source_path, specifier, reason, line)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for (source, import) in &data.unresolved {
                insert_unresolved.execute(params![
                    id,
                    source.as_str(),
                    import.specifier,
                    enum_text(&import.reason)?,
                    import.line,
                ])?;
            }
        }
        transaction.commit()?;
        Ok(SnapshotId(id))
    }

    /// What a snapshot covers.
    pub fn snapshot_coverage(&self, id: SnapshotId) -> Result<ParserCoverage, StorageError> {
        let json: String = self.connection.query_row(
            "SELECT coverage_json FROM snapshots WHERE id = ?1",
            [id.0],
            |row| row.get(0),
        )?;
        serde_json::from_str(&json).map_err(|error| StorageError::Corrupt(error.to_string()))
    }

    /// Files of a snapshot by path. Degrees and cycle flags are left at zero for
    /// the caller to fill in from the graph.
    pub fn snapshot_nodes(&self, id: SnapshotId) -> Result<Vec<GraphNode>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT path, language, loc, parse_status FROM snapshot_files
             WHERE snapshot_id = ?1 ORDER BY path",
        )?;
        let rows = statement.query_map([id.0], |row| {
            Ok(GraphNode {
                path: parse_column(row, 0)?,
                language: parse_column(row, 1)?,
                loc: row.get(2)?,
                parse_status: parse_column(row, 3)?,
                in_degree: 0,
                out_degree: 0,
                in_cycle: false,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Edges of a snapshot, in a stable order.
    pub fn snapshot_edges(&self, id: SnapshotId) -> Result<Vec<GraphEdge>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT source_path, target_path, edge_type, resolution, specifier, line
             FROM dependency_edges WHERE snapshot_id = ?1
             ORDER BY source_path, target_path, edge_type",
        )?;
        let rows = statement.query_map([id.0], |row| {
            Ok(GraphEdge {
                source: parse_column(row, 0)?,
                target: parse_column(row, 1)?,
                edge_type: parse_column(row, 2)?,
                resolution: parse_column(row, 3)?,
                specifier: row.get(4)?,
                line: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Unresolved imports of one file, in source order.
    pub fn unresolved_for_file(
        &self,
        id: SnapshotId,
        path: &RepoPath,
    ) -> Result<Vec<UnresolvedImport>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT specifier, reason, line FROM unresolved_imports
             WHERE snapshot_id = ?1 AND source_path = ?2 ORDER BY line, specifier",
        )?;
        let rows = statement.query_map(params![id.0, path.as_str()], |row| {
            Ok(UnresolvedImport {
                specifier: row.get(0)?,
                reason: parse_column(row, 1)?,
                line: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// External package imports in one file.
    pub fn external_imports_for_file(
        &self,
        id: SnapshotId,
        path: &RepoPath,
    ) -> Result<u32, StorageError> {
        let count = self.connection.query_row(
            "SELECT external_import_count FROM snapshot_files
             WHERE snapshot_id = ?1 AND path = ?2",
            params![id.0, path.as_str()],
            |row| row.get(0),
        );
        Ok(count.optional()?.unwrap_or(0))
    }
}
