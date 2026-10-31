//! The Architecture Time Machine: what changed in the import graph between two
//! commits. See `docs/algorithms.md` for the definitions.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use atlas_git::{ExclusionRules, GitRepository};
use atlas_model::history::ChangeStatus;
use atlas_model::{
    ArchitectureSummary, AtlasConfig, AtlasError, CommitRelation, CompareCounts, CompareRequest,
    CompareResponse, Cycle, CycleChange, DependencyEdgeType, FileMetricChange, FileRename,
    GraphEdge, GraphNode, ImportChange, Oid, ParseStatus, ParserCoverage, RenameEvidence, RepoPath,
    RepositoryEntry,
};
use atlas_storage::Database;

use crate::graph_view::{Architecture, load};
use crate::lineage::assign_identities;

const DEFAULT_LIST_LIMIT: u32 = 500;
const MAX_LIST_LIMIT: u32 = 5000;

/// Commits walked to follow renames between two commits.
const MAX_RENAME_WALK: u32 = 2000;

/// The root `tsconfig.json` decides which aliases resolve.
const TSCONFIG: &str = "tsconfig.json";

/// Diffs the import graphs of two commits.
pub fn compare(
    database: &Database,
    repository: &RepositoryEntry,
    request: &CompareRequest,
    config: &AtlasConfig,
) -> Result<CompareResponse, AtlasError> {
    let limit = request.limit.unwrap_or(DEFAULT_LIST_LIMIT);
    if !(1..=MAX_LIST_LIMIT).contains(&limit) {
        return Err(AtlasError::InvalidInput(format!(
            "limit must be between 1 and {MAX_LIST_LIMIT}"
        )));
    }

    let git = GitRepository::open(Path::new(&repository.path))?;
    let base_oid = git.resolve(&request.base)?;
    let target_oid = git.resolve(&request.target)?;
    let base = load(database, repository, Some(base_oid.as_str()), config)?;
    let target = load(database, repository, Some(target_oid.as_str()), config)?;

    let rules = ExclusionRules::from_config(&config.analysis, &config.limits)?;
    let relation = relation(&git, &base_oid, &target_oid)?;
    let direct = git.renames_between(&base_oid, &target_oid, &rules)?;
    let history = match relation {
        CommitRelation::Forward => history_renames(&git, &base_oid, &target_oid, &rules)?,
        CommitRelation::Backward => history_renames(&git, &target_oid, &base_oid, &rules)?
            .map(|pairs| pairs.into_iter().map(|(from, to)| (to, from)).collect()),
        CommitRelation::Same | CommitRelation::Diverged => Some(Vec::new()),
    };
    let unwalked = history.is_none();

    let direct_pairs: HashSet<(RepoPath, RepoPath)> = direct.iter().cloned().collect();
    let mut renames = direct;
    renames.extend(history.unwrap_or_default());
    let mut diff = diff_architectures(&base.architecture, &target.architecture, renames);
    for rename in &mut diff.renamed_files {
        if !direct_pairs.contains(&(rename.from.clone(), rename.to.clone())) {
            rename.evidence = RenameEvidence::History;
        }
    }

    let base_coverage = database.snapshot_coverage(base.id)?;
    let target_coverage = database.snapshot_coverage(target.id)?;

    let mut notes = relation_note(relation).into_iter().collect::<Vec<_>>();
    if unwalked {
        notes.push(format!(
            "The commits are not within {MAX_RENAME_WALK} first-parent commits of each other, so \
             renames were matched between the two commits only."
        ));
    }
    if tsconfig_blob(&git, &base_oid)? != tsconfig_blob(&git, &target_oid)? {
        notes.push(
            "The root tsconfig.json differs, so an import can gain or lose its edge without its \
             source changing."
                .to_owned(),
        );
    }
    notes.extend(coverage_notes(&base_coverage, &target_coverage));
    if diff.reclassified_cycles > 0 {
        notes.push(format!(
            "{} cycle(s) kept their members but changed between type-only and runtime imports.",
            diff.reclassified_cycles
        ));
    }

    let limit = limit as usize;
    diff.added_files.truncate(limit);
    diff.removed_files.truncate(limit);
    diff.renamed_files.truncate(limit);
    diff.added_imports.truncate(limit);
    diff.removed_imports.truncate(limit);
    diff.new_cycles.truncate(limit);
    diff.resolved_cycles.truncate(limit);
    diff.changed_cycles.truncate(limit);
    diff.metric_changes.truncate(limit);

    Ok(CompareResponse {
        base: summary(&git, &base_oid, &base.architecture, base_coverage)?,
        target: summary(&git, &target_oid, &target.architecture, target_coverage)?,
        relation,
        counts: diff.counts,
        added_files: diff.added_files,
        removed_files: diff.removed_files,
        renamed_files: diff.renamed_files,
        added_imports: diff.added_imports,
        removed_imports: diff.removed_imports,
        new_cycles: diff.new_cycles,
        resolved_cycles: diff.resolved_cycles,
        changed_cycles: diff.changed_cycles,
        metric_changes: diff.metric_changes,
        notes,
    })
}

/// Renames made step by step along the first-parent chain from `earlier` to
/// `later`, or `None` if that chain does not connect them within the walk limit.
fn history_renames(
    git: &GitRepository,
    earlier: &Oid,
    later: &Oid,
    rules: &ExclusionRules,
) -> Result<Option<Vec<(RepoPath, RepoPath)>>, AtlasError> {
    let walk = git.first_parent_history(later, MAX_RENAME_WALK + 1)?;
    let Some(start) = walk.oids.iter().position(|oid| oid == earlier) else {
        return Ok(None);
    };
    let commits = walk.oids[start + 1..]
        .iter()
        .map(|oid| git.commit_changes(oid, rules))
        .collect::<Result<Vec<_>, _>>()?;
    let (identities, changes) = assign_identities(&commits);

    // A file added after `earlier` is not the file that had its path then, even
    // if a deleted file used to live there.
    let mut seen = HashSet::new();
    let born_later: HashSet<usize> = changes
        .iter()
        .filter(|change| seen.insert(change.identity))
        .filter(|change| change.change.status == ChangeStatus::Added)
        .map(|change| change.identity)
        .collect();

    Ok(Some(
        identities
            .iter()
            .enumerate()
            .filter(|(index, _)| !born_later.contains(index))
            .filter_map(|(_, identity)| {
                let (first, last) = (identity.spans.first()?, identity.spans.last()?);
                (last.valid_to.is_none() && first.path != last.path)
                    .then(|| (first.path.clone(), last.path.clone()))
            })
            .collect(),
    ))
}

fn relation(git: &GitRepository, base: &Oid, target: &Oid) -> Result<CommitRelation, AtlasError> {
    Ok(if base == target {
        CommitRelation::Same
    } else if git.is_ancestor(base, target)? {
        CommitRelation::Forward
    } else if git.is_ancestor(target, base)? {
        CommitRelation::Backward
    } else {
        CommitRelation::Diverged
    })
}

fn relation_note(relation: CommitRelation) -> Option<String> {
    match relation {
        CommitRelation::Same => Some("Both sides are the same commit.".to_owned()),
        CommitRelation::Forward => None,
        CommitRelation::Backward => Some(
            "The target is older than the base, so \"added\" means removed over time.".to_owned(),
        ),
        CommitRelation::Diverged => Some(
            "Neither commit is an ancestor of the other, so this compares two lines of work \
             rather than one change over time."
                .to_owned(),
        ),
    }
}

fn tsconfig_blob(git: &GitRepository, commit: &Oid) -> Result<Option<Oid>, AtlasError> {
    Ok(git
        .tree_files(commit)?
        .files
        .into_iter()
        .find(|file| file.path.as_str() == TSCONFIG)
        .map(|file| file.blob))
}

/// Differences in what each snapshot could see, which limit what its numbers mean.
fn coverage_notes(base: &ParserCoverage, target: &ParserCoverage) -> Vec<String> {
    let mut notes = Vec::new();
    if base.tsconfig != target.tsconfig {
        notes.push(format!(
            "tsconfig.json was {:?} at the base and {:?} at the target, so alias resolution is \
             not the same on both sides.",
            base.tsconfig, target.tsconfig
        ));
    }
    let skipped = base.skipped_file_count + target.skipped_file_count;
    let partial = base.partial_file_count + target.partial_file_count;
    if skipped > 0 || partial > 0 {
        notes.push(format!(
            "Parsing was incomplete: {} skipped and {} partial at the base, {} skipped and {} \
             partial at the target. Imports from those files are marked uncertain.",
            base.skipped_file_count,
            base.partial_file_count,
            target.skipped_file_count,
            target.partial_file_count
        ));
    }
    if base.unresolved_import_count != target.unresolved_import_count {
        notes.push(format!(
            "Unresolved imports went from {} to {}. They have no edges, so dependency totals may \
             not compare like with like.",
            base.unresolved_import_count, target.unresolved_import_count
        ));
    }
    let hidden = |coverage: &ParserCoverage| {
        coverage.nonliteral_dynamic_import_count + coverage.require_call_count
    };
    if hidden(base) != hidden(target) {
        notes.push(format!(
            "Imports the graph cannot see (non-literal import() and require()) went from {} to \
             {}.",
            hidden(base),
            hidden(target)
        ));
    }
    notes
}

fn summary(
    git: &GitRepository,
    oid: &Oid,
    architecture: &Architecture,
    coverage: ParserCoverage,
) -> Result<ArchitectureSummary, AtlasError> {
    let cyclic: HashSet<&RepoPath> = architecture
        .cycles
        .iter()
        .flat_map(|cycle| cycle.members.iter())
        .collect();
    Ok(ArchitectureSummary {
        commit: git.commit_summary(oid)?,
        file_count: architecture.nodes.len() as u32,
        import_count: architecture.edges.len() as u32,
        cycle_count: architecture.cycles.len() as u32,
        cyclic_file_count: cyclic.len() as u32,
        largest_cycle_size: architecture
            .cycles
            .iter()
            .map(|cycle| cycle.members.len() as u32)
            .max()
            .unwrap_or(0),
        coverage,
    })
}

#[derive(Debug, Default)]
pub(crate) struct ArchitectureDiff {
    pub counts: CompareCounts,
    pub added_files: Vec<GraphNode>,
    pub removed_files: Vec<GraphNode>,
    pub renamed_files: Vec<FileRename>,
    pub added_imports: Vec<ImportChange>,
    pub removed_imports: Vec<ImportChange>,
    pub new_cycles: Vec<Cycle>,
    pub resolved_cycles: Vec<Cycle>,
    pub changed_cycles: Vec<CycleChange>,
    pub metric_changes: Vec<FileMetricChange>,
    pub reclassified_cycles: u32,
}

type EdgeKey<'a> = (&'a RepoPath, &'a RepoPath, DependencyEdgeType);

fn sort_key(change: &ImportChange) -> (&RepoPath, &RepoPath, u32) {
    (&change.edge.source, &change.edge.target, change.edge.line)
}

/// Compares two graphs. Base paths are carried through `renames` first, so a
/// rename alone changes no import and no cycle.
pub(crate) fn diff_architectures(
    base: &Architecture,
    target: &Architecture,
    renames: Vec<(RepoPath, RepoPath)>,
) -> ArchitectureDiff {
    let base_nodes: HashMap<&RepoPath, &GraphNode> =
        base.nodes.iter().map(|node| (&node.path, node)).collect();
    let target_nodes: HashMap<&RepoPath, &GraphNode> =
        target.nodes.iter().map(|node| (&node.path, node)).collect();

    // A rename counts only between two graph files, and only if the old path
    // left the graph and the new one is new to it. The first claim on a path wins.
    let mut claimed: HashSet<RepoPath> = HashSet::new();
    let renames: Vec<(RepoPath, RepoPath)> = renames
        .into_iter()
        .filter(|(from, to)| {
            base_nodes.contains_key(from)
                && target_nodes.contains_key(to)
                && !target_nodes.contains_key(from)
                && !base_nodes.contains_key(to)
                && !claimed.contains(from)
                && !claimed.contains(to)
                && claimed.insert(from.clone())
                && claimed.insert(to.clone())
        })
        .collect();
    let moved: HashMap<&RepoPath, &RepoPath> =
        renames.iter().map(|(from, to)| (from, to)).collect();
    let carry = |path: &'_ RepoPath| -> RepoPath { (*moved.get(path).unwrap_or(&path)).clone() };

    let arrived: HashSet<&RepoPath> = renames.iter().map(|(_, to)| to).collect();
    let mut added_files: Vec<GraphNode> = target
        .nodes
        .iter()
        .filter(|node| !base_nodes.contains_key(&node.path) && !arrived.contains(&node.path))
        .cloned()
        .collect();
    let mut removed_files: Vec<GraphNode> = base
        .nodes
        .iter()
        .filter(|node| !target_nodes.contains_key(&node.path) && !moved.contains_key(&node.path))
        .cloned()
        .collect();
    added_files.sort_by(|a, b| a.path.cmp(&b.path));
    removed_files.sort_by(|a, b| a.path.cmp(&b.path));

    let mut renamed_files: Vec<FileRename> = renames
        .iter()
        .map(|(from, to)| FileRename {
            from: from.clone(),
            to: to.clone(),
            evidence: RenameEvidence::Direct,
        })
        .collect();
    renamed_files.sort_by(|a, b| a.from.cmp(&b.from));

    // Files whose imports cannot be trusted at one of the commits, in target terms.
    let doubtful: HashSet<RepoPath> = base
        .nodes
        .iter()
        .filter(|node| node.parse_status != ParseStatus::Parsed)
        .map(|node| carry(&node.path))
        .chain(
            target
                .nodes
                .iter()
                .filter(|node| node.parse_status != ParseStatus::Parsed)
                .map(|node| node.path.clone()),
        )
        .collect();

    let carried: Vec<(RepoPath, RepoPath, &GraphEdge)> = base
        .edges
        .iter()
        .map(|edge| (carry(&edge.source), carry(&edge.target), edge))
        .collect();
    let base_keys: HashSet<EdgeKey> = carried
        .iter()
        .map(|(source, target, edge)| (source, target, edge.edge_type))
        .collect();
    let target_keys: HashSet<EdgeKey> = target
        .edges
        .iter()
        .map(|edge| (&edge.source, &edge.target, edge.edge_type))
        .collect();

    let mut added_imports: Vec<ImportChange> = target
        .edges
        .iter()
        .filter(|edge| !base_keys.contains(&(&edge.source, &edge.target, edge.edge_type)))
        .map(|edge| ImportChange {
            edge: edge.clone(),
            uncertain: doubtful.contains(&edge.source),
        })
        .collect();
    let mut removed_imports = Vec::new();
    let mut imports_following_renames = 0;
    for (source, destination, edge) in &carried {
        if !target_keys.contains(&(source, destination, edge.edge_type)) {
            removed_imports.push(ImportChange {
                edge: (*edge).clone(),
                uncertain: doubtful.contains(source),
            });
        } else if source != &edge.source || destination != &edge.target {
            imports_following_renames += 1;
        }
    }
    added_imports.sort_by(|a, b| sort_key(a).cmp(&sort_key(b)));
    removed_imports.sort_by(|a, b| sort_key(a).cmp(&sort_key(b)));

    let cycles = diff_cycles(&base.cycles, &target.cycles, &carry);
    let affected = affected_files(base, target, &added_imports, &removed_imports, &carry);
    let metric_changes = metric_changes(base, target, &target_nodes, &moved, &affected);

    ArchitectureDiff {
        counts: CompareCounts {
            added_files: added_files.len() as u32,
            removed_files: removed_files.len() as u32,
            renamed_files: renamed_files.len() as u32,
            added_imports: added_imports.len() as u32,
            removed_imports: removed_imports.len() as u32,
            uncertain_imports: added_imports
                .iter()
                .chain(&removed_imports)
                .filter(|change| change.uncertain)
                .count() as u32,
            imports_following_renames,
            new_cycles: cycles.new.len() as u32,
            resolved_cycles: cycles.resolved.len() as u32,
            changed_cycles: cycles.changed.len() as u32,
            unchanged_cycles: cycles.unchanged,
            metric_changes: metric_changes.len() as u32,
        },
        added_files,
        removed_files,
        renamed_files,
        added_imports,
        removed_imports,
        new_cycles: cycles.new,
        resolved_cycles: cycles.resolved,
        changed_cycles: cycles.changed,
        metric_changes,
        reclassified_cycles: cycles.reclassified,
    }
}

struct CycleDiff {
    new: Vec<Cycle>,
    resolved: Vec<Cycle>,
    changed: Vec<CycleChange>,
    unchanged: u32,
    reclassified: u32,
}

/// Cycles with the same member set are the same cycle. Of the rest, those sharing
/// members are paired, largest overlap first, as one cycle that changed; anything
/// left over is new or resolved.
fn diff_cycles(
    base: &[Cycle],
    target: &[Cycle],
    carry: &impl Fn(&RepoPath) -> RepoPath,
) -> CycleDiff {
    let base_sets: BTreeMap<Vec<RepoPath>, &Cycle> = base
        .iter()
        .map(|cycle| {
            let mut members: Vec<RepoPath> = cycle.members.iter().map(carry).collect();
            members.sort();
            (members, cycle)
        })
        .collect();
    let target_sets: HashSet<&Vec<RepoPath>> = target.iter().map(|cycle| &cycle.members).collect();

    let mut unchanged = 0;
    let mut reclassified = 0;
    let mut unmatched_target = Vec::new();
    for cycle in target {
        match base_sets.get(&cycle.members) {
            Some(before) => {
                unchanged += 1;
                reclassified += u32::from(before.type_only != cycle.type_only);
            }
            None => unmatched_target.push(cycle),
        }
    }
    let unmatched_base: Vec<(&Vec<RepoPath>, &Cycle)> = base_sets
        .iter()
        .filter(|(members, _)| !target_sets.contains(members))
        .map(|(members, cycle)| (members, *cycle))
        .collect();

    let mut pairs = Vec::new();
    for (before, _) in unmatched_base.iter().enumerate() {
        for (after, cycle) in unmatched_target.iter().enumerate() {
            let shared = unmatched_base[before]
                .0
                .iter()
                .filter(|member| cycle.members.contains(member))
                .count();
            if shared > 0 {
                pairs.push((shared, before, after));
            }
        }
    }
    // The order must not depend on which commit is the base.
    pairs.sort_by_key(|&(shared, before, after)| {
        let (a, b) = (unmatched_base[before].0, &unmatched_target[after].members);
        (
            std::cmp::Reverse(shared),
            a.min(b).clone(),
            a.max(b).clone(),
        )
    });

    let (mut used_before, mut used_after) = (HashSet::new(), HashSet::new());
    let mut changed = Vec::new();
    for (_, before, after) in pairs {
        if used_before.contains(&before) || used_after.contains(&after) {
            continue;
        }
        used_before.insert(before);
        used_after.insert(after);
        let (carried, old) = unmatched_base[before];
        let new = unmatched_target[after];
        changed.push(CycleChange {
            joined: new
                .members
                .iter()
                .filter(|member| !carried.contains(member))
                .cloned()
                .collect(),
            left: old
                .members
                .iter()
                .filter(|member| !new.members.contains(&carry(member)))
                .cloned()
                .collect(),
            before: old.clone(),
            after: new.clone(),
        });
    }
    changed.sort_by(|a, b| {
        b.after
            .members
            .len()
            .cmp(&a.after.members.len())
            .then_with(|| a.after.members.cmp(&b.after.members))
    });

    let new = unmatched_target
        .iter()
        .enumerate()
        .filter(|(index, _)| !used_after.contains(index))
        .map(|(_, cycle)| (*cycle).clone())
        .collect();
    let mut resolved: Vec<Cycle> = unmatched_base
        .iter()
        .enumerate()
        .filter(|(index, _)| !used_before.contains(index))
        .map(|(_, (_, cycle))| (*cycle).clone())
        .collect();
    resolved.sort_by(|a, b| {
        b.members
            .len()
            .cmp(&a.members.len())
            .then_with(|| a.members.cmp(&b.members))
    });
    CycleDiff {
        new,
        resolved,
        changed,
        unchanged,
        reclassified,
    }
}

/// Files whose dependents may differ, in target terms. A file's dependents change
/// only through a changed import, and only if that import's target reaches the
/// file, so everything else can skip a traversal.
fn affected_files(
    base: &Architecture,
    target: &Architecture,
    added: &[ImportChange],
    removed: &[ImportChange],
    carry: &impl Fn(&RepoPath) -> RepoPath,
) -> HashSet<RepoPath> {
    let mut affected: HashSet<RepoPath> = base
        .graph
        .reachable_from(removed.iter().map(|change| &change.edge.target))
        .iter()
        .map(carry)
        .collect();
    affected.extend(
        target
            .graph
            .reachable_from(added.iter().map(|change| &change.edge.target)),
    );
    affected
}

fn metric_changes(
    base: &Architecture,
    target: &Architecture,
    target_nodes: &HashMap<&RepoPath, &GraphNode>,
    moved: &HashMap<&RepoPath, &RepoPath>,
    affected: &HashSet<RepoPath>,
) -> Vec<FileMetricChange> {
    let mut changes: Vec<FileMetricChange> = base
        .nodes
        .iter()
        .filter_map(|before| {
            let renamed_to = moved.get(&before.path).copied();
            let path = renamed_to.unwrap_or(&before.path);
            let after = target_nodes.get(path)?;
            let same_degrees =
                before.in_degree == after.in_degree && before.out_degree == after.out_degree;
            if same_degrees && !affected.contains(path) {
                return None;
            }
            let dependents_before = base.graph.dependent_count(&before.path);
            let dependents_after = target.graph.dependent_count(path);
            (!same_degrees || dependents_before != dependents_after).then(|| FileMetricChange {
                path: path.clone(),
                previous_path: renamed_to.map(|_| before.path.clone()),
                in_degree_before: before.in_degree,
                in_degree_after: after.in_degree,
                out_degree_before: before.out_degree,
                out_degree_after: after.out_degree,
                dependents_before,
                dependents_after,
            })
        })
        .collect();
    changes.sort_by_key(|change| (std::cmp::Reverse(magnitude(change)), change.path.clone()));
    changes
}

fn magnitude(change: &FileMetricChange) -> u32 {
    change.in_degree_before.abs_diff(change.in_degree_after)
        + change.out_degree_before.abs_diff(change.out_degree_after)
        + change.dependents_before.abs_diff(change.dependents_after)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use atlas_model::{DependencyEdgeType, GraphEdge, GraphNode, Language, ParseStatus};
    use atlas_model::{RepoPath, ResolutionKind};
    use proptest::prelude::*;

    use super::{ArchitectureDiff, diff_architectures};
    use crate::graph_view::Architecture;

    fn p(name: &str) -> RepoPath {
        name.parse().unwrap()
    }

    fn node(path: &str, parse_status: ParseStatus) -> GraphNode {
        GraphNode {
            path: p(path),
            language: Language::Typescript,
            loc: 1,
            parse_status,
            in_degree: 0,
            out_degree: 0,
            in_cycle: false,
        }
    }

    fn edge(source: &str, target: &str, edge_type: DependencyEdgeType) -> GraphEdge {
        GraphEdge {
            source: p(source),
            target: p(target),
            edge_type,
            resolution: ResolutionKind::Relative,
            specifier: format!("./{target}"),
            line: 1,
        }
    }

    fn architecture(nodes: &[&str], edges: &[(&str, &str)]) -> Architecture {
        Architecture::new(
            nodes.iter().map(|n| node(n, ParseStatus::Parsed)).collect(),
            edges
                .iter()
                .map(|(s, t)| edge(s, t, DependencyEdgeType::StaticImport))
                .collect(),
        )
    }

    fn pairs(changes: &[atlas_model::ImportChange]) -> Vec<(String, String)> {
        changes
            .iter()
            .map(|c| (c.edge.source.to_string(), c.edge.target.to_string()))
            .collect()
    }

    fn rename(from: &str, to: &str) -> Vec<(RepoPath, RepoPath)> {
        vec![(p(from), p(to))]
    }

    #[test]
    fn a_rename_alone_changes_no_import_and_no_cycle() {
        let base = architecture(&["a", "b"], &[("a", "b"), ("b", "a")]);
        let target = architecture(&["a2", "b"], &[("a2", "b"), ("b", "a2")]);
        let diff = diff_architectures(&base, &target, rename("a", "a2"));

        assert_eq!(diff.renamed_files.len(), 1);
        assert!(diff.added_files.is_empty() && diff.removed_files.is_empty());
        assert!(diff.added_imports.is_empty() && diff.removed_imports.is_empty());
        assert!(diff.new_cycles.is_empty() && diff.resolved_cycles.is_empty());
        assert!(diff.metric_changes.is_empty());
        assert_eq!(diff.counts.imports_following_renames, 2);
        assert_eq!(diff.counts.unchanged_cycles, 1);
    }

    #[test]
    fn without_a_detected_rename_the_move_is_a_removal_and_an_addition() {
        let base = architecture(&["a", "b"], &[("b", "a")]);
        let target = architecture(&["a2", "b"], &[("b", "a2")]);
        let diff = diff_architectures(&base, &target, Vec::new());

        assert_eq!(diff.added_files.len(), 1);
        assert_eq!(diff.removed_files.len(), 1);
        assert_eq!(pairs(&diff.added_imports), [("b".into(), "a2".into())]);
        assert_eq!(pairs(&diff.removed_imports), [("b".into(), "a".into())]);
    }

    #[test]
    fn a_rename_whose_old_path_still_exists_is_ignored() {
        let base = architecture(&["a"], &[]);
        let target = architecture(&["a", "a2"], &[]);
        let diff = diff_architectures(&base, &target, rename("a", "a2"));
        assert!(diff.renamed_files.is_empty());
        assert_eq!(diff.added_files.len(), 1);
    }

    #[test]
    fn a_grown_cycle_is_one_changed_cycle() {
        let base = architecture(&["a", "b", "c"], &[("a", "b"), ("b", "a")]);
        let target = architecture(
            &["a", "b", "c"],
            &[("a", "b"), ("b", "c"), ("c", "a"), ("b", "a")],
        );
        let diff = diff_architectures(&base, &target, Vec::new());
        assert!(diff.new_cycles.is_empty() && diff.resolved_cycles.is_empty());
        assert_eq!(diff.counts.changed_cycles, 1);
        assert_eq!(diff.changed_cycles[0].joined, [p("c")]);
        assert!(diff.changed_cycles[0].left.is_empty());
        assert_eq!(diff.counts.unchanged_cycles, 0);
    }

    #[test]
    fn a_cycle_that_swaps_a_renamed_member_reports_real_changes_only() {
        let base = architecture(&["a", "b", "c"], &[("a", "b"), ("b", "c"), ("c", "a")]);
        let target = architecture(&["a2", "b", "d"], &[("a2", "b"), ("b", "d"), ("d", "a2")]);
        let diff = diff_architectures(&base, &target, rename("a", "a2"));
        assert_eq!(diff.changed_cycles[0].joined, [p("d")]);
        assert_eq!(diff.changed_cycles[0].left, [p("c")]);
    }

    #[test]
    fn a_split_cycle_pairs_with_its_largest_part() {
        let base = architecture(
            &["a", "b", "c", "d", "e"],
            &[("a", "b"), ("b", "c"), ("c", "d"), ("d", "e"), ("e", "a")],
        );
        let target = architecture(
            &["a", "b", "c", "d", "e"],
            &[("a", "b"), ("b", "c"), ("c", "a"), ("d", "e"), ("e", "d")],
        );
        let diff = diff_architectures(&base, &target, Vec::new());
        assert_eq!(diff.changed_cycles.len(), 1);
        assert_eq!(diff.changed_cycles[0].after.members.len(), 3);
        assert_eq!(diff.changed_cycles[0].left, [p("d"), p("e")]);
        assert_eq!(diff.new_cycles[0].members, [p("d"), p("e")]);
        assert!(diff.resolved_cycles.is_empty());
    }

    #[test]
    fn unrelated_cycles_stay_new_and_resolved() {
        let base = architecture(&["a", "b", "c", "d"], &[("a", "b"), ("b", "a")]);
        let target = architecture(&["a", "b", "c", "d"], &[("c", "d"), ("d", "c")]);
        let diff = diff_architectures(&base, &target, Vec::new());
        assert!(diff.changed_cycles.is_empty());
        assert_eq!((diff.new_cycles.len(), diff.resolved_cycles.len()), (1, 1));
    }

    #[test]
    fn two_renames_cannot_claim_one_path() {
        let base = architecture(&["a", "b"], &[]);
        let target = architecture(&["c"], &[]);
        let renames = vec![(p("a"), p("c")), (p("b"), p("c"))];
        let diff = diff_architectures(&base, &target, renames);
        assert_eq!(diff.renamed_files.len(), 1);
        assert_eq!(diff.renamed_files[0].from, p("a"));
        assert_eq!(diff.removed_files.len(), 1);
    }

    #[test]
    fn an_import_changing_kind_is_removed_and_added() {
        let nodes = || {
            vec![
                node("a", ParseStatus::Parsed),
                node("b", ParseStatus::Parsed),
            ]
        };
        let base = Architecture::new(
            nodes(),
            vec![edge("a", "b", DependencyEdgeType::StaticImport)],
        );
        let target = Architecture::new(
            nodes(),
            vec![edge("a", "b", DependencyEdgeType::TypeOnlyImport)],
        );
        let diff = diff_architectures(&base, &target, Vec::new());

        assert_eq!(diff.added_imports.len(), 1);
        assert_eq!(diff.removed_imports.len(), 1);
        assert!(diff.metric_changes.is_empty(), "degrees count files");
    }

    #[test]
    fn a_moved_import_line_is_not_a_change() {
        let base = architecture(&["a", "b"], &[("a", "b")]);
        let mut target = architecture(&["a", "b"], &[("a", "b")]);
        target.edges[0].line = 40;
        target.edges[0].specifier = "./b.ts".to_owned();
        let diff = diff_architectures(&base, &target, Vec::new());
        assert!(diff.added_imports.is_empty() && diff.removed_imports.is_empty());
    }

    #[test]
    fn imports_from_an_unparsed_file_are_uncertain_on_either_side() {
        let base = Architecture::new(
            vec![
                node("a", ParseStatus::Parsed),
                node("b", ParseStatus::Parsed),
            ],
            vec![edge("a", "b", DependencyEdgeType::StaticImport)],
        );
        let target = Architecture::new(
            vec![
                node("a", ParseStatus::Failed),
                node("b", ParseStatus::Parsed),
            ],
            Vec::new(),
        );
        let diff = diff_architectures(&base, &target, Vec::new());
        assert!(diff.removed_imports[0].uncertain);
        assert_eq!(diff.counts.uncertain_imports, 1);

        let diff = diff_architectures(&target, &base, Vec::new());
        assert!(diff.added_imports[0].uncertain);
    }

    #[test]
    fn dependents_count_transitive_files_and_can_change_with_equal_degrees() {
        let base = architecture(&["a", "b", "c", "d"], &[("b", "a"), ("c", "d")]);
        let target = architecture(&["a", "b", "c", "d"], &[("b", "a"), ("c", "d"), ("d", "b")]);
        let diff = diff_architectures(&base, &target, Vec::new());

        let a = diff
            .metric_changes
            .iter()
            .find(|c| c.path.as_str() == "a")
            .unwrap();
        assert_eq!((a.in_degree_before, a.in_degree_after), (1, 1));
        assert_eq!((a.dependents_before, a.dependents_after), (1, 3));
    }

    #[test]
    fn a_renamed_file_keeps_its_metric_history() {
        let base = architecture(&["a", "b", "c"], &[("b", "a")]);
        let target = architecture(&["a2", "b", "c"], &[("b", "a2"), ("c", "a2")]);
        let diff = diff_architectures(&base, &target, rename("a", "a2"));

        let change = &diff.metric_changes[0];
        assert_eq!(change.path.as_str(), "a2");
        assert_eq!(
            change.previous_path.as_ref().map(RepoPath::as_str),
            Some("a")
        );
        assert_eq!((change.in_degree_before, change.in_degree_after), (1, 2));
        assert_eq!((change.dependents_before, change.dependents_after), (1, 2));
    }

    #[test]
    fn a_cycle_turning_type_only_is_unchanged_but_counted() {
        let nodes = || {
            vec![
                node("a", ParseStatus::Parsed),
                node("b", ParseStatus::Parsed),
            ]
        };
        let edges = |kind| {
            vec![
                edge("a", "b", DependencyEdgeType::StaticImport),
                edge("b", "a", kind),
            ]
        };
        let base = Architecture::new(nodes(), edges(DependencyEdgeType::StaticImport));
        let target = Architecture::new(nodes(), edges(DependencyEdgeType::TypeOnlyImport));
        let diff = diff_architectures(&base, &target, Vec::new());

        assert_eq!(diff.counts.unchanged_cycles, 1);
        assert_eq!(diff.reclassified_cycles, 1);
        assert!(diff.new_cycles.is_empty() && diff.resolved_cycles.is_empty());
    }

    #[test]
    fn a_renamed_file_downstream_of_a_changed_import_is_recounted() {
        let base = architecture(&["a", "x", "y"], &[("x", "a"), ("y", "x")]);
        let target = architecture(&["a2", "x", "y"], &[("x", "a2")]);
        let diff = diff_architectures(&base, &target, rename("a", "a2"));

        let change = diff
            .metric_changes
            .iter()
            .find(|c| c.path == p("a2"))
            .unwrap();
        assert_eq!((change.in_degree_before, change.in_degree_after), (1, 1));
        assert_eq!((change.dependents_before, change.dependents_after), (2, 1));
    }

    #[test]
    fn a_rejected_rename_does_not_block_a_valid_one() {
        let base = architecture(&["a", "b"], &[]);
        let target = architecture(&["c", "d"], &[]);
        let renames = vec![(p("a"), p("c")), (p("b"), p("c")), (p("b"), p("d"))];
        let diff = diff_architectures(&base, &target, renames);
        let pairs: Vec<_> = diff
            .renamed_files
            .iter()
            .map(|r| (r.from.as_str(), r.to.as_str()))
            .collect();
        assert_eq!(pairs, [("a", "c"), ("b", "d")]);
    }

    fn edge_set(diff_side: &[atlas_model::ImportChange]) -> BTreeSet<(String, String)> {
        pairs(diff_side).into_iter().collect()
    }

    fn apply(base: &Architecture, diff: &ArchitectureDiff) -> BTreeSet<(String, String)> {
        let mut edges: BTreeSet<(String, String)> = base
            .edges
            .iter()
            .map(|e| (e.source.to_string(), e.target.to_string()))
            .collect();
        for gone in edge_set(&diff.removed_imports) {
            edges.remove(&gone);
        }
        edges.extend(edge_set(&diff.added_imports));
        edges
    }

    fn random_graph() -> impl Strategy<Value = Vec<(u8, u8)>> {
        prop::collection::vec((0u8..6, 0u8..6), 0..14)
    }

    fn build(edges: &[(u8, u8)]) -> Architecture {
        let names: Vec<String> = (0..6).map(|i| format!("f{i}")).collect();
        let nodes: Vec<&str> = names.iter().map(String::as_str).collect();
        let edges: BTreeSet<_> = edges.iter().copied().collect();
        let edges: Vec<(&str, &str)> = edges
            .iter()
            .map(|&(s, t)| (names[s as usize].as_str(), names[t as usize].as_str()))
            .collect();
        architecture(&nodes, &edges)
    }

    proptest! {
        /// Applying a diff to the base edges reproduces the target edges.
        #[test]
        fn a_diff_replays_to_the_target(a in random_graph(), b in random_graph()) {
            let (base, target) = (build(&a), build(&b));
            let diff = diff_architectures(&base, &target, Vec::new());
            let expected: BTreeSet<(String, String)> = target
                .edges
                .iter()
                .map(|e| (e.source.to_string(), e.target.to_string()))
                .collect();
            prop_assert_eq!(apply(&base, &diff), expected);
        }

        /// Skipping files that no changed import can reach loses nothing.
        #[test]
        fn metrics_match_a_full_recount(a in random_graph(), b in random_graph()) {
            let (base, target) = (build(&a), build(&b));
            let diff = diff_architectures(&base, &target, Vec::new());
            let reported: BTreeSet<String> =
                diff.metric_changes.iter().map(|m| m.path.to_string()).collect();
            let target_nodes: std::collections::HashMap<_, _> =
                target.nodes.iter().map(|n| (n.path.clone(), n)).collect();
            let expected: BTreeSet<String> = base
                .nodes
                .iter()
                .filter(|before| {
                    let after = target_nodes[&before.path];
                    before.in_degree != after.in_degree
                        || before.out_degree != after.out_degree
                        || base.graph.reverse_distances(&before.path, u32::MAX).len()
                            != target.graph.reverse_distances(&before.path, u32::MAX).len()
                })
                .map(|node| node.path.to_string())
                .collect();
            prop_assert_eq!(reported, expected);
        }

        /// Swapping the commits swaps every list and nothing else.
        #[test]
        fn swapping_sides_inverts_the_diff(a in random_graph(), b in random_graph()) {
            let (base, target) = (build(&a), build(&b));
            let forward = diff_architectures(&base, &target, Vec::new());
            let backward = diff_architectures(&target, &base, Vec::new());
            prop_assert_eq!(pairs(&forward.added_imports), pairs(&backward.removed_imports));
            prop_assert_eq!(pairs(&forward.removed_imports), pairs(&backward.added_imports));
            prop_assert_eq!(&forward.new_cycles, &backward.resolved_cycles);
            prop_assert_eq!(&forward.resolved_cycles, &backward.new_cycles);
            prop_assert_eq!(forward.changed_cycles.len(), backward.changed_cycles.len());
            for one in &forward.changed_cycles {
                let other = backward
                    .changed_cycles
                    .iter()
                    .find(|c| c.before == one.after && c.after == one.before);
                prop_assert!(other.is_some());
                prop_assert_eq!(&one.joined, &other.unwrap().left);
                prop_assert_eq!(&one.left, &other.unwrap().joined);
            }
            prop_assert_eq!(forward.metric_changes.len(), backward.metric_changes.len());
        }

        /// A graph compared with itself has no differences.
        #[test]
        fn a_graph_equals_itself(a in random_graph()) {
            let diff = diff_architectures(&build(&a), &build(&a), Vec::new());
            prop_assert!(diff.added_imports.is_empty() && diff.removed_imports.is_empty());
            prop_assert!(diff.new_cycles.is_empty() && diff.resolved_cycles.is_empty());
            prop_assert!(diff.metric_changes.is_empty());
        }
    }
}
