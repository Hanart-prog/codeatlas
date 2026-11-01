//! Reads commit diffs, from the cache where it can.
//!
//! A commit's diff against its first parent depends only on the two commits and
//! the settings, so a new tip reads just its new commits from Git. File identities
//! depend on the whole walk, so they are always assigned afresh from these diffs.

use atlas_git::{ExclusionRules, GitRepository};
use atlas_model::history::CommitChanges;
use atlas_model::{ALGORITHM_VERSION, AtlasConfig, AtlasError, Oid, ProgressUnit, RepositoryId};
use atlas_storage::Database;

/// Diffs written to the cache at a time, so that stopping keeps most of the work.
const FLUSH_EVERY: usize = 100;

/// Settings an analysis depends on. Deepening a shallow clone changes what the
/// same commit's history holds.
pub(crate) fn config_hash(git: &GitRepository, config: &AtlasConfig) -> String {
    match git.is_shallow() {
        true => format!("{}-shallow", config.analysis_hash()),
        false => config.analysis_hash(),
    }
}

/// Commits with their diffs, and how many had to be read from Git.
pub(crate) struct LoadedCommits {
    pub commits: Vec<CommitChanges>,
    pub read_from_git: u32,
}

/// Loads `oids` in order. Stopping partway keeps every diff already read.
pub(crate) fn load_commits(
    database: &Database,
    git: &GitRepository,
    repository_id: RepositoryId,
    oids: &[Oid],
    config: &AtlasConfig,
) -> Result<LoadedCommits, AtlasError> {
    let rules = ExclusionRules::from_config(&config.analysis, &config.limits)?;
    let key = format!("{ALGORITHM_VERSION}:{}", config_hash(git, config));
    let total = oids.len() as u32;

    let mut commits = Vec::with_capacity(oids.len());
    let mut unsaved = Vec::new();
    let mut read_from_git = 0;
    let mut save = |commits: &[CommitChanges], unsaved: &mut Vec<usize>| {
        let diffs: Vec<&CommitChanges> = unsaved.iter().map(|&index| &commits[index]).collect();
        read_from_git += diffs.len() as u32;
        let saved = database.store_commit_diffs(repository_id, &key, &diffs);
        unsaved.clear();
        saved
    };

    for (index, oid) in oids.iter().enumerate() {
        let step = (|| {
            match database.commit_diff(repository_id, &key, oid)? {
                Some(diff) => commits.push(diff),
                None => {
                    commits.push(git.commit_changes(oid, &rules)?);
                    unsaved.push(index);
                }
            }
            database
                .work()
                .step(index as u32 + 1, total, ProgressUnit::Commits)
        })();
        if let Err(error) = step {
            save(&commits, &mut unsaved)?;
            return Err(error);
        }
        if unsaved.len() == FLUSH_EVERY {
            save(&commits, &mut unsaved)?;
        }
    }
    save(&commits, &mut unsaved)?;
    Ok(LoadedCommits {
        commits,
        read_from_git,
    })
}
