//! Background analysis jobs.
//!
//! Jobs live in memory and disappear on restart; their results are in SQLite.
//! Each runs on a blocking thread with its own database connection, so reads keep
//! working during an analysis. Cancellation arrives in Phase 6.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use atlas_analysis::{AnalyzeOptions, analyze};
use atlas_model::{AtlasError, ErrorCode, JobResponse, JobStatus, RepositoryId};
use atlas_storage::Database;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;

use crate::{ApiError, AppState};

#[derive(Debug, Default)]
pub struct JobRegistry {
    next_id: AtomicU32,
    jobs: Mutex<HashMap<u32, JobResponse>>,
}

impl JobRegistry {
    fn create(&self, repository_id: RepositoryId) -> JobResponse {
        let job = JobResponse {
            id: self.next_id.fetch_add(1, Ordering::Relaxed) + 1,
            repository_id,
            status: JobStatus::Queued,
            error: None,
        };
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.insert(job.id, job.clone());
        }
        job
    }

    fn update(&self, id: u32, status: JobStatus, error: Option<String>) {
        if let Ok(mut jobs) = self.jobs.lock()
            && let Some(job) = jobs.get_mut(&id)
        {
            job.status = status;
            job.error = error;
        }
    }

    fn get(&self, id: u32) -> Option<JobResponse> {
        self.jobs.lock().ok()?.get(&id).cloned()
    }
}

/// Queues an analysis and returns immediately.
pub fn start(
    state: &AppState,
    repository_id: RepositoryId,
    path: PathBuf,
    options: AnalyzeOptions,
) -> JobResponse {
    let job = state.jobs.create(repository_id);
    let id = job.id;
    let state = state.clone();

    tokio::spawn(async move {
        // The semaphore lives as long as the state and is never closed.
        let Ok(_slot) = Arc::clone(&state.analysis_slots).acquire_owned().await else {
            return;
        };
        state.jobs.update(id, JobStatus::Running, None);

        let database_path = state.database_path.clone();
        let config = Arc::clone(&state.config);
        let result = tokio::task::spawn_blocking(move || {
            let mut database = Database::open(&database_path)?;
            analyze(&mut database, &path, &options, &config)?;
            Ok::<_, AtlasError>(())
        })
        .await;

        match result {
            Ok(Ok(())) => state.jobs.update(id, JobStatus::Completed, None),
            Ok(Err(error)) => {
                tracing::warn!(%error, "analysis failed");
                state
                    .jobs
                    .update(id, JobStatus::Failed, Some(error.to_string()));
            }
            Err(_) => {
                let message = "analysis stopped unexpectedly".to_owned();
                state.jobs.update(id, JobStatus::Failed, Some(message));
            }
        }
    });
    job
}

/// `GET /jobs/:id`
pub async fn get_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<JobResponse>, ApiError> {
    id.parse()
        .ok()
        .and_then(|id| state.jobs.get(id))
        .map(Json)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, ErrorCode::NotFound, "no such job"))
}
