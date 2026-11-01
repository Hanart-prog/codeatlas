//! Indexes a repository into storage. Shared by the CLI and the API.

use std::path::Path;

use atlas_git::{ExclusionRules, GitRepository};
use atlas_model::history::IndexedHistory;
use atlas_model::{
    ALGORITHM_VERSION, AnalysisRunId, AtlasConfig, AtlasError, DEFAULT_HISTORY_LIMIT,
    PARSER_VERSION, RepositoryEntry,
};
use atlas_storage::{Database, RunKey};

use crate::lineage::assign_identities;
use crate::snapshot::ensure_snapshot;

/// What to analyze.
#[derive(Debug, Clone)]
pub struct AnalyzeOptions {
    /// Git reference to analyze.
    pub git_ref: String,
    /// Maximum first-parent commits to index.
    pub history_limit: u32,
}

impl Default for AnalyzeOptions {
    fn default() -> Self {
        Self {
            git_ref: "HEAD".to_owned(),
            history_limit: DEFAULT_HISTORY_LIMIT,
        }
    }
}

/// The result of an analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyzeOutcome {
    /// The repository.
    pub repository: RepositoryEntry,
    /// The run holding the results.
    pub run_id: AnalysisRunId,
    /// True when an identical completed run was reused.
    pub cached: bool,
    /// Why no import graph was built, if the repository is over a limit.
    pub graph_skipped: Option<String>,
}

/// Validates a local path as a repository and registers it.
pub fn open_repository(database: &Database, path: &Path) -> Result<RepositoryEntry, AtlasError> {
    let git = GitRepository::open(path)?;
    Ok(database.upsert_repository(&git.canonical_path())?)
}

/// Finds an already registered repository from a local path.
pub fn find_repository(database: &Database, path: &Path) -> Result<RepositoryEntry, AtlasError> {
    let git = GitRepository::open(path)?;
    database
        .repository_by_path(&git.canonical_path())?
        .ok_or_else(|| {
            AtlasError::NotFound(format!(
                "{} has not been analyzed yet; run `codeatlas analyze` first",
                path.display()
            ))
        })
}

/// Indexes a repository, reusing an identical completed run when one exists.
///
/// Results are written in one transaction, so a failure leaves a failed run with
/// no partial history.
pub fn analyze(
    database: &mut Database,
    path: &Path,
    options: &AnalyzeOptions,
    config: &AtlasConfig,
) -> Result<AnalyzeOutcome, AtlasError> {
    if options.history_limit == 0 {
        return Err(AtlasError::InvalidInput(
            "history limit must be positive".to_owned(),
        ));
    }
    let git = GitRepository::open(path)?;
    let target_oid = git.resolve(&options.git_ref)?;
    let repository = database.upsert_repository(&git.canonical_path())?;
    // Deepening a shallow clone changes what the same commit's history holds.
    let config_hash = match git.is_shallow() {
        true => format!("{}-shallow", config.analysis_hash()),
        false => config.analysis_hash(),
    };
    let key = RunKey {
        repository_id: repository.id,
        target_oid: &target_oid,
        history_limit: options.history_limit,
        parser_version: PARSER_VERSION,
        algorithm_version: ALGORITHM_VERSION,
        config_hash: &config_hash,
    };

    if let Some(run) = database.cached_run(&key)? {
        database.mark_run_latest(run.id)?;
        let graph_skipped = snapshot_or_reason(database, &git, &target_oid, config)?;
        return Ok(AnalyzeOutcome {
            repository,
            run_id: run.id,
            cached: true,
            graph_skipped,
        });
    }

    let run_id = database.begin_run(&key)?;
    let result = index_history(&git, &target_oid, options.history_limit, config)
        .and_then(|history| Ok(database.complete_run(repository.id, run_id, &history)?));
    if let Err(error) = result {
        database.fail_run(run_id, &error.to_string())?;
        // Another process may have finished this exact analysis first, in which
        // case the unique cache key rejected this run and its result stands.
        return match database.cached_run(&key)? {
            Some(run) => Ok(AnalyzeOutcome {
                repository,
                run_id: run.id,
                cached: true,
                graph_skipped: None,
            }),
            None => Err(error),
        };
    }

    database.mark_run_latest(run_id)?;
    let graph_skipped = snapshot_or_reason(database, &git, &target_oid, config)?;
    let repository = database.upsert_repository(&git.canonical_path())?;
    Ok(AnalyzeOutcome {
        repository,
        run_id,
        cached: false,
        graph_skipped,
    })
}

/// Builds the graph. A repository over a limit keeps its history analysis, so that
/// is a reason to report rather than a failure.
fn snapshot_or_reason(
    database: &Database,
    git: &GitRepository,
    target: &atlas_model::Oid,
    config: &AtlasConfig,
) -> Result<Option<String>, AtlasError> {
    match ensure_snapshot(database, git, target, config) {
        Ok(_) => Ok(None),
        Err(AtlasError::TooLarge(reason)) => Ok(Some(reason)),
        Err(error) => Err(error),
    }
}

fn index_history(
    git: &GitRepository,
    target: &atlas_model::Oid,
    history_limit: u32,
    config: &AtlasConfig,
) -> Result<IndexedHistory, AtlasError> {
    let rules = ExclusionRules::from_config(&config.analysis, &config.limits)?;
    let walk = git.first_parent_history(target, history_limit)?;
    let commits = walk
        .oids
        .iter()
        .map(|oid| git.commit_changes(oid, &rules))
        .collect::<Result<Vec<_>, _>>()?;

    let (identities, changes) = assign_identities(&commits);
    Ok(IndexedHistory {
        unreadable_paths: commits.iter().map(|c| c.unreadable_paths).sum(),
        commits: commits.into_iter().map(|c| c.commit).collect(),
        identities,
        changes,
        is_shallow: git.is_shallow(),
        is_truncated: walk.is_truncated,
        tree: git.tree_summary(target, &rules)?,
    })
}
