//! Lets a caller watch and stop long work.
//!
//! Engine loops check in between steps, so stopping takes effect at the next step
//! rather than at once, and everything finished before it stays valid.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::dto::{JobProgress, ProgressUnit};
use crate::error::AtlasError;

type Report = Arc<dyn Fn(JobProgress) + Send + Sync>;

/// A cancel flag, a deadline and a progress listener for one piece of work.
/// Clones share the flag, so one can be kept to cancel the work later.
#[derive(Clone, Default)]
pub struct Work {
    cancelled: Arc<AtomicBool>,
    deadline: Option<Instant>,
    report: Option<Report>,
}

impl Work {
    /// Work that fails with [`AtlasError::TimedOut`] once `limit` has passed.
    pub fn within(limit: Duration) -> Self {
        Self {
            deadline: Instant::now().checked_add(limit),
            ..Self::default()
        }
    }

    /// Sends progress to `report`.
    pub fn reporting(mut self, report: impl Fn(JobProgress) + Send + Sync + 'static) -> Self {
        self.report = Some(Arc::new(report));
        self
    }

    /// Asks the work to stop at its next check.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    /// True once [`Work::cancel`] has been called.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// Fails if the work should stop.
    pub fn check(&self) -> Result<(), AtlasError> {
        if self.is_cancelled() {
            return Err(AtlasError::Cancelled);
        }
        if self.deadline.is_some_and(|end| Instant::now() >= end) {
            return Err(AtlasError::TimedOut);
        }
        Ok(())
    }

    /// Reports progress, then fails if the work should stop.
    pub fn step(&self, done: u32, total: u32, unit: ProgressUnit) -> Result<(), AtlasError> {
        if let Some(report) = &self.report {
            report(JobProgress { done, total, unit });
        }
        self.check()
    }
}

impl fmt::Debug for Work {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Work")
            .field("cancelled", &self.is_cancelled())
            .field("deadline", &self.deadline)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[test]
    fn unlimited_work_never_stops() {
        assert!(Work::default().check().is_ok());
    }

    #[test]
    fn a_clone_cancels_the_original() {
        let work = Work::default();
        work.clone().cancel();
        assert!(matches!(work.check(), Err(AtlasError::Cancelled)));
    }

    #[test]
    fn a_passed_deadline_stops_work() {
        let work = Work::within(Duration::ZERO);
        assert!(matches!(work.check(), Err(AtlasError::TimedOut)));
        assert!(Work::within(Duration::from_secs(3600)).check().is_ok());
    }

    #[test]
    fn cancelling_wins_over_a_passed_deadline() {
        let work = Work::within(Duration::ZERO);
        work.cancel();
        assert!(matches!(work.check(), Err(AtlasError::Cancelled)));
    }

    #[test]
    fn a_step_reports_before_it_stops() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let work = Work::default().reporting(move |progress| sink.lock().unwrap().push(progress));
        work.cancel();
        assert!(work.step(1, 2, ProgressUnit::Commits).is_err());
        assert_eq!(
            *seen.lock().unwrap(),
            [JobProgress {
                done: 1,
                total: 2,
                unit: ProgressUnit::Commits
            }]
        );
    }
}
