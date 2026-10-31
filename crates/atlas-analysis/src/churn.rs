//! Churn per file identity. See `docs/algorithms.md` for the exact definition.

use std::collections::HashSet;

use atlas_model::history::ExclusionReason;
use atlas_model::{FileChurn, RepoPath};

/// One change as churn needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChurnInput {
    /// File identity.
    pub identity: usize,
    /// First-parent position of the commit.
    pub position: u32,
    /// Lines added.
    pub added_lines: u32,
    /// Lines deleted.
    pub deleted_lines: u32,
    /// Excluded changes are not counted.
    pub exclusion: Option<ExclusionReason>,
}

/// Computes churn per identity, skipping excluded changes.
///
/// `identity_paths[i]` lists identity `i`'s paths, oldest first, with whether it
/// exists at the tip. Identities with no eligible change are omitted. The result
/// is ordered by commit count descending, then path, then line count, so ties
/// are deterministic even for identities that shared a path.
pub fn compute_churn(
    changes: &[ChurnInput],
    identity_paths: &[(Vec<RepoPath>, bool)],
) -> Vec<FileChurn> {
    #[derive(Clone, Default)]
    struct Totals {
        commits: HashSet<u32>,
        added: u32,
        deleted: u32,
    }

    let mut totals = vec![Totals::default(); identity_paths.len()];
    for change in changes.iter().filter(|change| change.exclusion.is_none()) {
        let entry = &mut totals[change.identity];
        entry.commits.insert(change.position);
        entry.added = entry.added.saturating_add(change.added_lines);
        entry.deleted = entry.deleted.saturating_add(change.deleted_lines);
    }

    let mut churn: Vec<FileChurn> = totals
        .into_iter()
        .zip(identity_paths)
        .filter(|(totals, _)| !totals.commits.is_empty())
        .filter_map(|(totals, (paths, exists_at_tip))| {
            let (path, previous) = paths.split_last()?;
            Some(FileChurn {
                path: path.clone(),
                exists_at_tip: *exists_at_tip,
                commit_count: totals.commits.len() as u32,
                added_lines: totals.added,
                deleted_lines: totals.deleted,
                previous_paths: previous.to_vec(),
            })
        })
        .collect();

    churn.sort_by(|a, b| {
        b.commit_count
            .cmp(&a.commit_count)
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| b.added_lines.cmp(&a.added_lines))
    });
    churn
}

#[cfg(test)]
mod tests {
    use atlas_model::history::ExclusionReason;

    use super::{ChurnInput, compute_churn};

    fn input(identity: usize, position: u32, added: u32, deleted: u32) -> ChurnInput {
        ChurnInput {
            identity,
            position,
            added_lines: added,
            deleted_lines: deleted,
            exclusion: None,
        }
    }

    fn identity(paths: &[&str], live: bool) -> (Vec<atlas_model::RepoPath>, bool) {
        (paths.iter().map(|p| p.parse().unwrap()).collect(), live)
    }

    #[test]
    fn totals_follow_an_identity_across_paths() {
        let churn = compute_churn(
            &[input(0, 0, 10, 0), input(0, 1, 0, 0), input(0, 2, 2, 1)],
            &[identity(&["old.ts", "new.ts"], true)],
        );
        assert_eq!(churn.len(), 1);
        assert_eq!(churn[0].path.as_str(), "new.ts");
        assert_eq!(churn[0].previous_paths[0].as_str(), "old.ts");
        assert_eq!(
            (
                churn[0].commit_count,
                churn[0].added_lines,
                churn[0].deleted_lines
            ),
            (3, 12, 1)
        );
    }

    #[test]
    fn a_commit_counts_once_per_identity() {
        let churn = compute_churn(
            &[input(0, 5, 1, 0), input(0, 5, 1, 0)],
            &[identity(&["a.ts"], true)],
        );
        assert_eq!(churn[0].commit_count, 1);
        assert_eq!(churn[0].added_lines, 2);
    }

    #[test]
    fn excluded_changes_do_not_count() {
        let excluded = ChurnInput {
            exclusion: Some(ExclusionReason::Binary),
            ..input(0, 0, 99, 99)
        };
        let churn = compute_churn(&[excluded], &[identity(&["a.bin"], true)]);
        assert!(churn.is_empty());
    }

    #[test]
    fn ties_break_by_path() {
        let churn = compute_churn(
            &[
                input(0, 0, 1, 0),
                input(1, 0, 1, 0),
                input(2, 0, 1, 0),
                input(2, 1, 1, 0),
            ],
            &[
                identity(&["z.ts"], true),
                identity(&["a.ts"], true),
                identity(&["m.ts"], true),
            ],
        );
        let order: Vec<_> = churn.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(order, ["m.ts", "a.ts", "z.ts"]);
    }
}
