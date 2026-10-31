//! Historical co-change along the first-parent chain ending at a cutoff.
//!
//! The walk starts at the cutoff, so later commits are never read. That makes
//! look-ahead impossible by construction rather than by filtering.

use std::collections::{BTreeMap, HashMap};

use atlas_git::{ExclusionRules, GitRepository, is_typescript};
use atlas_model::history::{CommitChanges, IdentifiedChange, IdentityIndex};
use atlas_model::{AtlasConfig, AtlasError, CoChangeStats, Oid, RepoPath};

use crate::lineage::assign_identities;

/// Eligible commit positions by file identity and by pair. Pair counting is
/// sparse: only pairs that actually share a commit exist, and commits over the
/// bulk threshold never reach it, which bounds the work per commit.
#[derive(Debug, Default)]
pub struct CoChangeIndex {
    commits: HashMap<IdentityIndex, Vec<u32>>,
    pairs: BTreeMap<(IdentityIndex, IdentityIndex), Vec<u32>>,
    eligible: u32,
    bulk: u32,
    no_eligible: u32,
    merges: u32,
}

impl CoChangeIndex {
    /// Builds the index. `sets[i]` holds the distinct eligible identities of the
    /// commit at position `i`, each counted once however it changed.
    pub fn build(sets: &[Vec<IdentityIndex>], merges: &[bool], bulk_threshold: u32) -> Self {
        let mut index = Self::default();
        for (position, set) in sets.iter().enumerate() {
            let position = position as u32;
            if set.is_empty() {
                index.no_eligible += 1;
                continue;
            }
            if set.len() > bulk_threshold as usize {
                index.bulk += 1;
                continue;
            }
            index.eligible += 1;
            index.merges += u32::from(merges.get(position as usize) == Some(&true));
            for (offset, &identity) in set.iter().enumerate() {
                index.commits.entry(identity).or_default().push(position);
                for &other in &set[offset + 1..] {
                    let key = (identity.min(other), identity.max(other));
                    index.pairs.entry(key).or_default().push(position);
                }
            }
        }
        index
    }

    /// Eligible commits touching `identity`, oldest first.
    pub fn commits_of(&self, identity: IdentityIndex) -> &[u32] {
        self.commits.get(&identity).map_or(&[], Vec::as_slice)
    }

    /// Eligible commits touching both, oldest first.
    pub fn shared(&self, a: IdentityIndex, b: IdentityIndex) -> &[u32] {
        self.pairs
            .get(&(a.min(b), a.max(b)))
            .map_or(&[], Vec::as_slice)
    }

    /// Every pair with its shared commits.
    pub fn pairs(&self) -> impl Iterator<Item = (IdentityIndex, IdentityIndex, &[u32])> {
        self.pairs
            .iter()
            .map(|(&(a, b), commits)| (a, b, commits.as_slice()))
    }

    /// Commits that passed eligibility.
    pub fn eligible_commit_count(&self) -> u32 {
        self.eligible
    }
}

/// History as of a cutoff, resolved to the file identities of that moment.
pub struct ScopedHistory {
    /// Commits oldest first; an index is a position.
    pub commits: Vec<CommitChanges>,
    /// Co-change counts.
    pub index: CoChangeIndex,
    /// Files alive at the cutoff, by path.
    pub alive: HashMap<RepoPath, IdentityIndex>,
    /// Counts behind the index.
    pub stats: CoChangeStats,
}

impl ScopedHistory {
    /// The path of an identity alive at the cutoff.
    pub fn alive_paths(&self) -> HashMap<IdentityIndex, &RepoPath> {
        self.alive.iter().map(|(path, &id)| (id, path)).collect()
    }
}

/// Reads up to `window` first-parent commits ending at `cutoff`.
pub fn load_scoped_history(
    git: &GitRepository,
    cutoff: &Oid,
    window: u32,
    config: &AtlasConfig,
) -> Result<ScopedHistory, AtlasError> {
    let rules = ExclusionRules::from_config(&config.analysis, &config.limits)?;
    let walk = git.first_parent_history(cutoff, window)?;
    let commits = walk
        .oids
        .iter()
        .map(|oid| git.commit_changes(oid, &rules))
        .collect::<Result<Vec<_>, _>>()?;

    let (identities, changes) = assign_identities(&commits);
    let sets = eligible_sets(commits.len(), &changes);
    let merges: Vec<bool> = commits.iter().map(|c| c.commit.parents.len() > 1).collect();
    let threshold = config.analysis.bulk_commit_file_threshold;
    let index = CoChangeIndex::build(&sets, &merges, threshold);

    let mut alive = HashMap::new();
    for (identity, file) in identities.iter().enumerate() {
        if let Some(span) = file.spans.last()
            && span.valid_to.is_none()
        {
            alive.insert(span.path.clone(), identity);
        }
    }

    let stats = CoChangeStats {
        commit_count: commits.len() as u32,
        eligible_commit_count: index.eligible,
        bulk_commit_count: index.bulk,
        no_eligible_file_commit_count: index.no_eligible,
        merge_commit_count: index.merges,
        bulk_threshold: threshold,
        min_shared_commits: config.analysis.min_shared_commits,
        is_shallow: git.is_shallow(),
        is_truncated: walk.is_truncated,
    };
    Ok(ScopedHistory {
        commits,
        index,
        alive,
        stats,
    })
}

/// The distinct eligible identities of each commit, sorted. Eligible means not
/// excluded and TS/TSX, the same files the import graph covers.
fn eligible_sets(commit_count: usize, changes: &[IdentifiedChange]) -> Vec<Vec<IdentityIndex>> {
    let mut sets = vec![Vec::new(); commit_count];
    for change in changes {
        let file = &change.change;
        let path = file.new_path.as_ref().or(file.old_path.as_ref());
        if file.exclusion.is_none() && path.is_some_and(is_typescript) {
            sets[change.position as usize].push(change.identity);
        }
    }
    for set in &mut sets {
        set.sort_unstable();
        set.dedup();
    }
    sets
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::CoChangeIndex;

    #[test]
    fn counts_each_pair_once_per_commit() {
        let index = CoChangeIndex::build(&[vec![0, 1], vec![0, 1, 2], vec![1]], &[false; 3], 50);
        assert_eq!(index.shared(0, 1), [0, 1]);
        assert_eq!(index.shared(1, 0), [0, 1]);
        assert_eq!(index.shared(0, 2), [1]);
        assert_eq!(index.commits_of(0), [0, 1]);
        assert_eq!(index.commits_of(1), [0, 1, 2]);
        assert_eq!(index.eligible, 3);
    }

    #[test]
    fn a_commit_at_the_threshold_counts_and_one_over_does_not() {
        let index = CoChangeIndex::build(&[vec![0, 1], vec![0, 1, 2]], &[false; 2], 2);
        assert_eq!(index.shared(0, 1), [0]);
        assert!(index.shared(0, 2).is_empty());
        assert_eq!(index.commits_of(2), [] as [u32; 0]);
        assert_eq!((index.eligible, index.bulk), (1, 1));
    }

    #[test]
    fn empty_commits_are_counted_apart() {
        let index = CoChangeIndex::build(&[vec![], vec![1]], &[false, true], 50);
        assert_eq!((index.eligible, index.no_eligible), (1, 1));
        assert_eq!(index.merges, 1);
    }

    #[test]
    fn a_bulk_merge_is_not_counted_as_a_merge() {
        let index = CoChangeIndex::build(&[vec![0, 1, 2]], &[true], 2);
        assert_eq!(index.merges, 0);
    }

    proptest! {
        /// Checks sparse counts against counting every commit for a pair.
        #[test]
        fn counts_match_brute_force(
            sets in prop::collection::vec(prop::collection::btree_set(0usize..8, 0..6), 0..30),
            threshold in 1u32..6,
        ) {
            let sets: Vec<Vec<usize>> = sets.into_iter().map(|s| s.into_iter().collect()).collect();
            let index = CoChangeIndex::build(&sets, &vec![false; sets.len()], threshold);
            let counted = |set: &&Vec<usize>| !set.is_empty() && set.len() <= threshold as usize;

            for a in 0..8 {
                let expected = sets.iter().filter(counted).filter(|s| s.contains(&a)).count();
                prop_assert_eq!(index.commits_of(a).len(), expected);
                for b in a + 1..8 {
                    let expected = sets
                        .iter()
                        .filter(counted)
                        .filter(|s| s.contains(&a) && s.contains(&b))
                        .count();
                    prop_assert_eq!(index.shared(a, b).len(), expected);
                    prop_assert!(index.shared(a, b).len() <= index.commits_of(a).len());
                }
            }
        }
    }
}
