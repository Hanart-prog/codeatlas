//! Follows files through renames along the first-parent chain.
//!
//! Positions on that chain are linear, so lineage needs no DAG reasoning: the
//! chain is replayed oldest first with a map from current path to identity.

use std::collections::HashMap;

use atlas_model::RepoPath;
use atlas_model::history::{
    ChangeStatus, CommitChanges, FileIdentity, IdentifiedChange, IdentityIndex, PathSpan,
};

/// Assigns each change to a file identity.
///
/// - A rename continues the identity at a new path.
/// - A deletion ends the identity's path. A later file at that path is a new
///   identity, because it has no lineage connection to the deleted file.
/// - A file first seen by modification has older history outside the indexed
///   window, and starts an identity at that point.
pub fn assign_identities(commits: &[CommitChanges]) -> (Vec<FileIdentity>, Vec<IdentifiedChange>) {
    let mut identities: Vec<FileIdentity> = Vec::new();
    let mut changes = Vec::new();
    let mut current: HashMap<RepoPath, IdentityIndex> = HashMap::new();

    for (position, commit) in commits.iter().enumerate() {
        let position = position as u32;

        // Release old paths before claiming new ones, so that swaps within one
        // commit (a -> b and b -> a) keep each file's own identity.
        let released: Vec<Option<IdentityIndex>> = commit
            .changes
            .iter()
            .map(|change| match change.status {
                ChangeStatus::Renamed | ChangeStatus::Deleted => change
                    .old_path
                    .as_ref()
                    .and_then(|path| current.remove(path)),
                _ => None,
            })
            .collect();

        for (change, released) in commit.changes.iter().zip(released) {
            let identity = match change.status {
                ChangeStatus::Renamed | ChangeStatus::Deleted => released,
                ChangeStatus::Modified | ChangeStatus::TypeChanged => change
                    .new_path
                    .as_ref()
                    .and_then(|path| current.get(path).copied()),
                ChangeStatus::Added => None,
            };
            let path = change.new_path.as_ref().or(change.old_path.as_ref());
            let Some(path) = path else { continue };

            let identity = identity.unwrap_or_else(|| {
                identities.push(FileIdentity {
                    first_seen: position,
                    spans: vec![PathSpan {
                        path: change.old_path.clone().unwrap_or_else(|| path.clone()),
                        valid_from: position,
                        valid_to: None,
                    }],
                });
                identities.len() - 1
            });

            let spans = &mut identities[identity].spans;
            if matches!(change.status, ChangeStatus::Renamed | ChangeStatus::Deleted)
                && let Some(last) = spans.last_mut()
            {
                last.valid_to = Some(position);
            }
            if change.status == ChangeStatus::Renamed {
                spans.push(PathSpan {
                    path: path.clone(),
                    valid_from: position,
                    valid_to: None,
                });
            }
            if let Some(new_path) = &change.new_path {
                current.insert(new_path.clone(), identity);
            }

            changes.push(IdentifiedChange {
                position,
                identity,
                change: change.clone(),
            });
        }
    }

    (identities, changes)
}

#[cfg(test)]
mod tests {
    use atlas_model::history::{ChangeStatus, CommitChanges, CommitRecord, FileChange};
    use proptest::prelude::*;

    use super::assign_identities;

    fn change(status: ChangeStatus, old: Option<&str>, new: Option<&str>) -> FileChange {
        FileChange {
            old_path: old.map(|p| p.parse().unwrap()),
            new_path: new.map(|p| p.parse().unwrap()),
            status,
            added_lines: 0,
            deleted_lines: 0,
            exclusion: None,
        }
    }

    fn add(path: &str) -> FileChange {
        change(ChangeStatus::Added, None, Some(path))
    }
    fn modify(path: &str) -> FileChange {
        change(ChangeStatus::Modified, Some(path), Some(path))
    }
    fn delete(path: &str) -> FileChange {
        change(ChangeStatus::Deleted, Some(path), None)
    }
    fn rename(old: &str, new: &str) -> FileChange {
        change(ChangeStatus::Renamed, Some(old), Some(new))
    }

    fn commits(changes: Vec<Vec<FileChange>>) -> Vec<CommitChanges> {
        changes
            .into_iter()
            .enumerate()
            .map(|(index, changes)| CommitChanges {
                commit: CommitRecord {
                    oid: format!("{index:040x}").parse().unwrap(),
                    parents: Vec::new(),
                    committed_at: 0,
                    subject: String::new(),
                },
                changes,
                unreadable_paths: 0,
            })
            .collect()
    }

    fn paths(identity: &atlas_model::history::FileIdentity) -> Vec<&str> {
        identity
            .spans
            .iter()
            .map(|span| span.path.as_str())
            .collect()
    }

    #[test]
    fn a_rename_chain_is_one_identity() {
        let (identities, changes) = assign_identities(&commits(vec![
            vec![add("a.ts")],
            vec![rename("a.ts", "b.ts")],
            vec![modify("b.ts")],
            vec![rename("b.ts", "c.ts")],
        ]));
        assert_eq!(identities.len(), 1);
        assert_eq!(paths(&identities[0]), ["a.ts", "b.ts", "c.ts"]);
        assert!(changes.iter().all(|c| c.identity == 0));

        let spans = &identities[0].spans;
        assert_eq!((spans[0].valid_from, spans[0].valid_to), (0, Some(1)));
        assert_eq!((spans[1].valid_from, spans[1].valid_to), (1, Some(3)));
        assert_eq!((spans[2].valid_from, spans[2].valid_to), (3, None));
    }

    #[test]
    fn a_new_file_at_a_deleted_path_is_a_new_identity() {
        let (identities, changes) = assign_identities(&commits(vec![
            vec![add("a.ts")],
            vec![delete("a.ts")],
            vec![add("a.ts")],
        ]));
        assert_eq!(identities.len(), 2);
        assert_eq!(identities[0].spans[0].valid_to, Some(1));
        assert_eq!(identities[1].spans[0].valid_to, None);
        assert_eq!(changes[2].identity, 1);
    }

    #[test]
    fn a_swap_within_one_commit_keeps_each_identity() {
        let (identities, changes) = assign_identities(&commits(vec![
            vec![add("a.ts"), add("b.ts")],
            vec![rename("a.ts", "b.ts"), rename("b.ts", "a.ts")],
            vec![modify("a.ts")],
        ]));
        assert_eq!(identities.len(), 2);
        assert_eq!(paths(&identities[0]), ["a.ts", "b.ts"]);
        assert_eq!(paths(&identities[1]), ["b.ts", "a.ts"]);
        assert_eq!(changes.last().unwrap().identity, 1);
    }

    #[test]
    fn a_rename_onto_a_deleted_path_keeps_the_renamed_identity() {
        let (identities, changes) = assign_identities(&commits(vec![
            vec![add("a.ts"), add("b.ts")],
            vec![rename("a.ts", "b.ts"), delete("b.ts")],
            vec![modify("b.ts")],
        ]));
        assert_eq!(identities.len(), 2);
        assert_eq!(changes.last().unwrap().identity, 0);
    }

    #[test]
    fn history_older_than_the_window_starts_an_identity() {
        let (identities, _) = assign_identities(&commits(vec![
            vec![modify("old.ts")],
            vec![rename("older.ts", "new.ts")],
        ]));
        assert_eq!(identities.len(), 2);
        assert_eq!(identities[0].first_seen, 0);
        assert_eq!(paths(&identities[1]), ["older.ts", "new.ts"]);
    }

    #[test]
    fn modifications_stay_with_their_identity() {
        let (identities, changes) = assign_identities(&commits(vec![
            vec![add("a.ts"), add("b.ts")],
            vec![modify("b.ts")],
            vec![modify("a.ts")],
        ]));
        assert_eq!(identities.len(), 2);
        let ids: Vec<_> = changes.iter().map(|c| c.identity).collect();
        assert_eq!(ids, [0, 1, 1, 0]);
    }

    proptest! {
        /// Replaying random valid histories: every live path maps to exactly one
        /// identity with an open span, and spans never overlap.
        #[test]
        fn lineage_invariants_hold(ops in prop::collection::vec((0u8..4, 0usize..6, 0usize..6), 1..60)) {
            let name = |i: usize| format!("f{i}.ts");
            let mut live = std::collections::BTreeSet::new();
            let mut history = Vec::new();
            for (op, a, b) in ops {
                let (a, b) = (name(a), name(b));
                let step = match op {
                    0 if !live.contains(&a) => { live.insert(a.clone()); add(&a) }
                    1 if live.contains(&a) => modify(&a),
                    2 if live.contains(&a) => { live.remove(&a); delete(&a) }
                    3 if live.contains(&a) && !live.contains(&b) => {
                        live.remove(&a);
                        live.insert(b.clone());
                        rename(&a, &b)
                    }
                    _ => continue,
                };
                history.push(vec![step]);
            }

            let (identities, _) = assign_identities(&commits(history));
            let mut open: Vec<String> = identities
                .iter()
                .flat_map(|i| i.spans.iter())
                .filter(|s| s.valid_to.is_none())
                .map(|s| s.path.to_string())
                .collect();
            open.sort();
            prop_assert_eq!(open, live.into_iter().collect::<Vec<_>>());

            for identity in &identities {
                for pair in identity.spans.windows(2) {
                    prop_assert_eq!(pair[0].valid_to, Some(pair[1].valid_from));
                }
            }
        }
    }
}
