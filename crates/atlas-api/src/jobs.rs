//! Background analysis and backtest jobs.
//!
//! Jobs live in memory and disappear on restart; their results are in SQLite.
//! Each runs on a blocking thread with its own database connection, so reads keep
//! working during an analysis. Cancelling takes effect at the job's next step.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use atlas_analysis::{AnalyzeOptions, analyze, backtest};
use atlas_model::{
    AtlasConfig, AtlasError, BacktestRequest, ErrorCode, JobKind, JobResponse, JobStatus,
    RepositoryEntry, RepositoryId, Work,
};
use atlas_storage::Database;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;

use crate::repositories::parse_id;
use crate::{ApiError, AppState};

/// Finished jobs kept per server run, so that the registry cannot grow forever.
const KEPT_FINISHED_JOBS: usize = 200;

/// Jobs listed for a repository, newest first.
const LISTED_JOBS: usize = 10;

#[derive(Debug)]
struct Entry {
    job: JobResponse,
    work: Work,
}

#[derive(Debug, Default)]
pub struct JobRegistry {
    next_id: AtomicU32,
    jobs: Mutex<HashMap<u32, Entry>>,
}

const fn is_finished(status: JobStatus) -> bool {
    matches!(
        status,
        JobStatus::Completed | JobStatus::Failed | JobStatus::Cancelled
    )
}

impl JobRegistry {
    fn create(self: &Arc<Self>, repository_id: RepositoryId, kind: JobKind) -> (JobResponse, Work) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let job = JobResponse {
            id,
            repository_id,
            kind,
            status: JobStatus::Queued,
            error: None,
            progress: None,
        };
        let registry = Arc::downgrade(self);
        let work = Work::default().reporting(move |progress| {
            if let Some(registry) = registry.upgrade()
                && let Ok(mut jobs) = registry.jobs.lock()
                && let Some(entry) = jobs.get_mut(&id)
            {
                entry.job.progress = Some(progress);
            }
        });
        if let Ok(mut jobs) = self.jobs.lock() {
            let finished = jobs
                .values()
                .filter(|entry| is_finished(entry.job.status))
                .count();
            if finished >= KEPT_FINISHED_JOBS {
                let oldest = jobs
                    .values()
                    .filter(|entry| is_finished(entry.job.status))
                    .map(|entry| entry.job.id)
                    .min();
                if let Some(oldest) = oldest {
                    jobs.remove(&oldest);
                }
            }
            jobs.insert(
                id,
                Entry {
                    job: job.clone(),
                    work: work.clone(),
                },
            );
        }
        (job, work)
    }

    fn update(&self, id: u32, status: JobStatus, error: Option<String>) {
        if let Ok(mut jobs) = self.jobs.lock()
            && let Some(entry) = jobs.get_mut(&id)
        {
            entry.job.status = status;
            entry.job.error = error;
        }
    }

    fn get(&self, id: u32) -> Option<JobResponse> {
        Some(self.jobs.lock().ok()?.get(&id)?.job.clone())
    }

    fn for_repository(&self, repository_id: RepositoryId) -> Vec<JobResponse> {
        let Ok(jobs) = self.jobs.lock() else {
            return Vec::new();
        };
        let mut matching: Vec<JobResponse> = jobs
            .values()
            .filter(|entry| entry.job.repository_id == repository_id)
            .map(|entry| entry.job.clone())
            .collect();
        matching.sort_by_key(|job| std::cmp::Reverse(job.id));
        matching.truncate(LISTED_JOBS);
        matching
    }

    /// Asks a job to stop. One still queued stops at once, because nothing is
    /// running that could notice. Returns the job, or `None` if there is none.
    fn cancel(&self, id: u32) -> Option<JobResponse> {
        let mut jobs = self.jobs.lock().ok()?;
        let entry = jobs.get_mut(&id)?;
        if !is_finished(entry.job.status) {
            entry.work.cancel();
            if entry.job.status == JobStatus::Queued {
                entry.job.status = JobStatus::Cancelled;
            }
        }
        Some(entry.job.clone())
    }
}

/// Queues an analysis and returns immediately.
pub fn start(
    state: &AppState,
    repository_id: RepositoryId,
    path: PathBuf,
    options: AnalyzeOptions,
) -> JobResponse {
    run(
        state,
        repository_id,
        JobKind::Analysis,
        move |database, config| analyze(database, &path, &options, config).map(|_| ()),
    )
}

/// Queues a backtest and returns immediately.
pub fn start_backtest(
    state: &AppState,
    repository: RepositoryEntry,
    request: BacktestRequest,
) -> JobResponse {
    run(
        state,
        repository.id,
        JobKind::Backtest,
        move |database, config| {
            backtest(database, &repository, &request, config, |_, _| {}).map(|_| ())
        },
    )
}

fn run(
    state: &AppState,
    repository_id: RepositoryId,
    kind: JobKind,
    task: impl FnOnce(&mut Database, &AtlasConfig) -> Result<(), AtlasError> + Send + 'static,
) -> JobResponse {
    let (job, work) = state.jobs.create(repository_id, kind);
    let id = job.id;
    let state = state.clone();

    tokio::spawn(async move {
        // The semaphore lives as long as the state and is never closed.
        let Ok(_slot) = Arc::clone(&state.analysis_slots).acquire_owned().await else {
            return;
        };
        if work.is_cancelled() {
            return;
        }
        state.jobs.update(id, JobStatus::Running, None);

        let database_path = state.database_path.clone();
        let config = Arc::clone(&state.config);
        let result = tokio::task::spawn_blocking(move || {
            let mut database = Database::open(&database_path)?;
            database.set_work(work);
            task(&mut database, &config)
        })
        .await;

        match result {
            Ok(Ok(())) => state.jobs.update(id, JobStatus::Completed, None),
            Ok(Err(AtlasError::Cancelled)) => state.jobs.update(id, JobStatus::Cancelled, None),
            Ok(Err(error)) => {
                tracing::warn!(%error, "job failed");
                state
                    .jobs
                    .update(id, JobStatus::Failed, Some(error.to_string()));
            }
            Err(_) => {
                let message = "job stopped unexpectedly".to_owned();
                state.jobs.update(id, JobStatus::Failed, Some(message));
            }
        }
    });
    job
}

fn no_such_job() -> ApiError {
    ApiError::new(StatusCode::NOT_FOUND, ErrorCode::NotFound, "no such job")
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
        .ok_or_else(no_such_job)
}

/// `POST /jobs/:id/cancel`: the response may still say `running`, until the job
/// reaches its next step. Cancelling a finished job changes nothing.
pub async fn cancel_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<JobResponse>, ApiError> {
    id.parse()
        .ok()
        .and_then(|id| state.jobs.cancel(id))
        .map(Json)
        .ok_or_else(no_such_job)
}

/// `GET /repositories/:id/jobs`: this server run's jobs, newest first.
pub async fn repository_jobs(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<JobResponse>>, ApiError> {
    Ok(Json(state.jobs.for_repository(parse_id(&id)?)))
}
