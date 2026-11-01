//! Stored backtest results. They are JSON because nothing queries inside them.

use atlas_model::{AnalysisRunId, BacktestEntry, BacktestResult, RepositoryId};
use rusqlite::{OptionalExtension, params};

use crate::{Database, StorageError};

fn entry(created_at: i64, json: &str) -> Result<BacktestEntry, StorageError> {
    let result: BacktestResult =
        serde_json::from_str(json).map_err(|error| StorageError::Corrupt(error.to_string()))?;
    Ok(BacktestEntry { created_at, result })
}

impl Database {
    /// The stored backtest for exactly these parameters.
    pub fn backtest(
        &self,
        run: AnalysisRunId,
        params_json: &str,
    ) -> Result<Option<BacktestEntry>, StorageError> {
        self.connection
            .query_row(
                "SELECT created_at, metrics_json FROM backtest_runs
                 WHERE analysis_run_id = ?1 AND params_json = ?2",
                params![run.0, params_json],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
            .map(|(created_at, json)| entry(created_at, &json))
            .transpose()
    }

    /// Stores a backtest, keeping the existing row if another process got there
    /// first, and returns what is stored.
    pub fn store_backtest(
        &self,
        run: AnalysisRunId,
        params_json: &str,
        result: &BacktestResult,
        created_at: i64,
    ) -> Result<BacktestEntry, StorageError> {
        let json = serde_json::to_string(result)
            .map_err(|error| StorageError::Corrupt(error.to_string()))?;
        self.connection.execute(
            "INSERT OR IGNORE INTO backtest_runs
                 (analysis_run_id, params_json, metrics_json, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![run.0, params_json, json, created_at],
        )?;
        self.backtest(run, params_json)?
            .ok_or_else(|| StorageError::Corrupt("stored backtest is missing".to_owned()))
    }

    /// The newest backtest of a repository.
    pub fn latest_backtest(
        &self,
        repository: RepositoryId,
    ) -> Result<Option<BacktestEntry>, StorageError> {
        self.connection
            .query_row(
                "SELECT b.created_at, b.metrics_json
                 FROM backtest_runs b
                 JOIN analysis_runs r ON r.id = b.analysis_run_id
                 WHERE r.repository_id = ?1
                 ORDER BY b.id DESC LIMIT 1",
                [repository.0],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
            .map(|(created_at, json)| entry(created_at, &json))
            .transpose()
    }
}
