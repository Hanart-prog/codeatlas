//! The repository overview, shared by the CLI and the API.

use std::collections::HashSet;

use atlas_model::history::ChangeStatus;
use atlas_model::{
    AtlasError, FileSummary, HistorySummary, OverviewResponse, RepositoryId, RunSummary,
};
use atlas_storage::Database;

use crate::churn::{ChurnInput, compute_churn};

/// Files listed in `top_churn`.
pub const TOP_CHURN_LIMIT: usize = 20;

/// Summarizes the latest completed analysis of a repository.
pub fn overview(
    database: &Database,
    repository_id: RepositoryId,
) -> Result<OverviewResponse, AtlasError> {
    let repository = database
        .repository(repository_id)?
        .ok_or_else(|| AtlasError::NotFound("no such repository".to_owned()))?;
    let run = database.latest_run(repository_id)?.ok_or_else(|| {
        AtlasError::NotFound(format!("{} has not been analyzed yet", repository.path))
    })?;

    let (commit_count, merge_commit_count) = database.commit_counts(run.id)?;
    let changes = database.changes(run.id)?;
    let identities = database.identities(run.id)?;

    let eligible_identities: HashSet<_> = changes
        .iter()
        .filter(|change| change.exclusion.is_none())
        .map(|change| change.identity)
        .collect();
    let rename_count = changes
        .iter()
        .filter(|change| change.status == ChangeStatus::Renamed)
        .count() as u32;

    let inputs: Vec<_> = changes
        .iter()
        .map(|change| ChurnInput {
            identity: change.identity,
            position: change.position,
            added_lines: change.added_lines,
            deleted_lines: change.deleted_lines,
            exclusion: change.exclusion,
        })
        .collect();
    let paths: Vec<_> = identities
        .into_iter()
        .map(|identity| (identity.paths, identity.exists_at_tip))
        .collect();
    let mut top_churn = compute_churn(&inputs, &paths);
    top_churn.truncate(TOP_CHURN_LIMIT);

    Ok(OverviewResponse {
        history: HistorySummary {
            commit_count,
            merge_commit_count,
            first_commit: database.commit_at(run.id, 0)?,
            last_commit: match commit_count {
                0 => None,
                count => database.commit_at(run.id, count - 1)?,
            },
            is_shallow: run.is_shallow,
            is_truncated: run.is_truncated,
        },
        files: FileSummary {
            tip_file_count: run.tip_file_count,
            tip_typescript_file_count: run.tip_typescript_file_count,
            changed_file_count: eligible_identities.len() as u32,
            rename_count,
            unreadable_path_count: run.unreadable_path_count,
        },
        exclusions: database.exclusion_counts(run.id)?,
        top_churn,
        repository,
        run: RunSummary {
            id: run.id,
            target_oid: run.target_oid,
            history_limit: run.history_limit,
            completed_at: run.completed_at,
            parser_version: run.parser_version,
            algorithm_version: run.algorithm_version,
            config_hash: run.config_hash,
        },
    })
}
