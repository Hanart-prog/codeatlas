//! How the import graph changed over the latest analysis.

use std::collections::BTreeSet;

use atlas_model::{
    AtlasConfig, AtlasError, RepoPath, RepositoryEntry, TimelinePoint, TimelineResponse,
};
use atlas_storage::Database;

use crate::graph_view::load;

/// Commits reported unless a caller asks otherwise.
pub const DEFAULT_TIMELINE_POINTS: u32 = 40;
const MAX_TIMELINE_POINTS: u32 = 200;

/// `count` positions spread evenly over `total`, always with the first and last.
fn sample(total: u32, count: u32) -> Vec<u32> {
    if total <= count {
        return (0..total).collect();
    }
    let span = u64::from(total - 1);
    let steps = u64::from(count.max(2) - 1);
    (0..=steps)
        .map(|step| (step * span / steps) as u32)
        .collect()
}

/// Architecture metrics at evenly spaced commits of the latest analysis. Each
/// commit reads its cached snapshot, or builds it the first time.
pub fn timeline(
    database: &Database,
    repository: &RepositoryEntry,
    points: u32,
    config: &AtlasConfig,
) -> Result<TimelineResponse, AtlasError> {
    if !(1..=MAX_TIMELINE_POINTS).contains(&points) {
        return Err(AtlasError::InvalidInput(format!(
            "points must be between 1 and {MAX_TIMELINE_POINTS}"
        )));
    }
    let run = database.latest_run(repository.id)?.ok_or_else(|| {
        AtlasError::NotFound(format!("{} has not been analyzed yet", repository.path))
    })?;
    let newest = database.commit_page(run.id, None, 1)?;
    let commit_count = newest
        .commits
        .first()
        .map_or(0, |commit| commit.position + 1);

    let mut timeline = Vec::new();
    for position in sample(commit_count, points) {
        let Some(commit) = database.commit_at(run.id, position)? else {
            continue;
        };
        let snapshot = load(database, repository, Some(commit.oid.as_str()), config)?;
        let architecture = snapshot.architecture;
        let cyclic: BTreeSet<&RepoPath> = architecture
            .cycles
            .iter()
            .flat_map(|cycle| cycle.members.iter())
            .collect();
        timeline.push(TimelinePoint {
            position,
            file_count: architecture.nodes.len() as u32,
            import_count: architecture.edges.len() as u32,
            cycle_count: architecture.cycles.len() as u32,
            cyclic_file_count: cyclic.len() as u32,
            skipped_file_count: database.snapshot_coverage(snapshot.id)?.skipped_file_count,
            commit,
        });
    }
    Ok(TimelineResponse {
        points: timeline,
        commit_count,
    })
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::sample;

    #[test]
    fn small_histories_are_returned_whole() {
        assert_eq!(sample(3, 40), [0, 1, 2]);
        assert!(sample(0, 40).is_empty());
    }

    #[test]
    fn samples_are_evenly_spaced() {
        assert_eq!(sample(11, 3), [0, 5, 10]);
    }

    proptest! {
        #[test]
        fn samples_keep_both_ends_and_never_repeat(total in 2u32..5000, count in 2u32..200) {
            let positions = sample(total, count);
            prop_assert_eq!(positions[0], 0);
            prop_assert_eq!(*positions.last().unwrap(), total - 1);
            prop_assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
            prop_assert!(positions.len() <= count as usize);
        }
    }
}
