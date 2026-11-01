//! Read models over stored snapshots, shared by the CLI and the API.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use atlas_git::GitRepository;
use atlas_graph::DependencyGraph;
use atlas_model::{
    AtlasConfig, AtlasError, CoChangeEdge, DirectoryGroup, DirectoryGroupKind, DirectoryLink,
    DirectoryMap, FileDetail, GraphEdge, GraphNode, GraphResponse, Oid, RepoPath, RepositoryEntry,
};
use atlas_storage::{Database, SnapshotId};

use crate::cochange::load_scoped_history;
use crate::impact::DEFAULT_HISTORY_WINDOW;
use crate::overview::run_churn;
use crate::snapshot::{is_type_only, snapshot_for};

/// Which part of a graph to return.
#[derive(Debug, Clone)]
pub struct GraphQuery {
    /// Commit to read; defaults to the latest analyzed commit.
    pub git_ref: Option<String>,
    /// Only files under this directory.
    pub directory: Option<String>,
    /// Only files whose path contains this, ignoring case.
    pub search: Option<String>,
    /// Most connected files to return.
    pub limit: u32,
    /// Also return co-change pairs among the returned files.
    pub include_cochange: bool,
}

/// A commit's import graph with degrees and cycles filled in.
pub(crate) struct Architecture {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub cycles: Vec<atlas_model::Cycle>,
    pub graph: DependencyGraph,
}

impl Architecture {
    pub fn new(mut nodes: Vec<GraphNode>, edges: Vec<GraphEdge>) -> Self {
        let mut graph = DependencyGraph::new(nodes.iter().map(|node| node.path.clone()));
        for edge in &edges {
            graph.add_edge(&edge.source, &edge.target, is_type_only(edge.edge_type));
        }

        let cycles = graph.cycles();
        let cyclic: HashSet<&RepoPath> = cycles.iter().flat_map(|c| c.members.iter()).collect();
        for node in &mut nodes {
            node.in_degree = graph.in_degree(&node.path);
            node.out_degree = graph.out_degree(&node.path);
            node.in_cycle = cyclic.contains(&node.path);
        }
        Self {
            nodes,
            edges,
            cycles,
            graph,
        }
    }
}

pub(crate) struct Snapshot {
    pub id: SnapshotId,
    pub commit: Oid,
    pub architecture: Architecture,
}

pub(crate) fn load(
    database: &Database,
    repository: &RepositoryEntry,
    git_ref: Option<&str>,
    config: &AtlasConfig,
) -> Result<Snapshot, AtlasError> {
    let git = GitRepository::open(Path::new(&repository.path))?;
    let commit = match git_ref {
        Some(reference) => git.resolve(reference)?,
        None => {
            let run = database.latest_run(repository.id)?.ok_or_else(|| {
                AtlasError::NotFound(format!("{} has not been analyzed yet", repository.path))
            })?;
            run.target_oid
        }
    };
    let id = snapshot_for(database, &git, repository, &commit, config)?;

    let architecture =
        Architecture::new(database.snapshot_nodes(id)?, database.snapshot_edges(id)?);
    Ok(Snapshot {
        id,
        commit,
        architecture,
    })
}

fn in_directory(path: &RepoPath, directory: &str) -> bool {
    let directory = directory.trim_end_matches('/');
    directory.is_empty()
        || path
            .as_str()
            .strip_prefix(directory)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// The import graph at a commit, reduced to the most connected matching files.
pub fn graph(
    database: &Database,
    repository: &RepositoryEntry,
    query: &GraphQuery,
    config: &AtlasConfig,
) -> Result<GraphResponse, AtlasError> {
    let snapshot = load(database, repository, query.git_ref.as_deref(), config)?;
    let architecture = snapshot.architecture;
    let total_node_count = architecture.nodes.len() as u32;
    let total_edge_count = architecture.edges.len() as u32;
    let search = query.search.as_deref().map(str::to_lowercase);

    let mut nodes: Vec<GraphNode> = architecture
        .nodes
        .into_iter()
        .filter(|node| {
            query
                .directory
                .as_deref()
                .is_none_or(|directory| in_directory(&node.path, directory))
                && search
                    .as_deref()
                    .is_none_or(|term| node.path.as_str().to_lowercase().contains(term))
        })
        .collect();
    let matching_node_count = nodes.len() as u32;
    nodes.sort_by(|a, b| {
        (b.in_degree + b.out_degree)
            .cmp(&(a.in_degree + a.out_degree))
            .then_with(|| a.path.cmp(&b.path))
    });
    nodes.truncate(query.limit as usize);

    let kept: HashSet<&RepoPath> = nodes.iter().map(|node| &node.path).collect();
    let cochange = if query.include_cochange {
        cochange_edges(database, repository, &snapshot.commit, &kept, config)?
    } else {
        Vec::new()
    };
    let edges = architecture
        .edges
        .into_iter()
        .filter(|edge| kept.contains(&edge.source) && kept.contains(&edge.target))
        .collect();

    Ok(GraphResponse {
        coverage: database.snapshot_coverage(snapshot.id)?,
        commit: snapshot.commit,
        cycles: architecture.cycles,
        cochange,
        nodes,
        edges,
        matching_node_count,
        total_node_count,
        total_edge_count,
    })
}

/// The group a file falls in on a map of `directory`: the next path segment below
/// it, or `None` for a file directly in it.
fn group_of<'a>(path: &'a str, directory: &str) -> Option<&'a str> {
    let rest = match directory {
        "" => path,
        directory => &path[directory.len() + 1..],
    };
    let end = rest.find('/')?;
    Some(&path[..path.len() - rest.len() + end])
}

/// The import graph at a commit, grouped by directory, so that a graph too large
/// to draw still has a readable shape. Larger groups are opened by asking for
/// their directory.
pub fn directory_map(
    database: &Database,
    repository: &RepositoryEntry,
    git_ref: Option<&str>,
    directory: &str,
    limit: u32,
    config: &AtlasConfig,
) -> Result<DirectoryMap, AtlasError> {
    let snapshot = load(database, repository, git_ref, config)?;
    let architecture = snapshot.architecture;
    Ok(group_by_directory(
        snapshot.commit,
        &architecture.nodes,
        &architecture.edges,
        directory,
        limit,
    ))
}

/// A group's path, and whether it stands for loose files rather than a directory
/// of the same name.
type GroupKey<'a> = (&'a str, bool);

fn group_by_directory(
    commit: Oid,
    nodes: &[GraphNode],
    edges: &[GraphEdge],
    directory: &str,
    limit: u32,
) -> DirectoryMap {
    let mut root = directory.trim_matches('/').to_owned();
    let under_root: Vec<&GraphNode> = nodes
        .iter()
        .filter(|node| in_directory(&node.path, &root))
        .collect();

    // A directory holding one subdirectory has nothing to show, so go straight
    // to the first level where the files divide.
    loop {
        let mut groups = under_root
            .iter()
            .map(|node| group_of(node.path.as_str(), &root));
        match groups.next() {
            Some(Some(only)) if groups.all(|group| group == Some(only)) => root = only.to_owned(),
            _ => break,
        }
    }

    let mut groups: BTreeMap<GroupKey<'_>, DirectoryGroup> = BTreeMap::new();
    let mut group_key: HashMap<&RepoPath, GroupKey<'_>> = HashMap::new();
    for node in &under_root {
        let group = group_of(node.path.as_str(), &root);
        let key = (group.unwrap_or(&root), group.is_none());
        group_key.insert(&node.path, key);
        let entry = groups.entry(key).or_insert_with(|| DirectoryGroup {
            path: key.0.to_owned(),
            kind: match group {
                Some(_) => DirectoryGroupKind::Directory,
                None => DirectoryGroupKind::LooseFiles,
            },
            file_count: 0,
            cyclic_file_count: 0,
            internal_import_count: 0,
        });
        entry.file_count += 1;
        entry.cyclic_file_count += u32::from(node.in_cycle);
    }

    let mut counts: BTreeMap<(GroupKey<'_>, GroupKey<'_>), u32> = BTreeMap::new();
    for edge in edges {
        let (Some(&source), Some(&target)) =
            (group_key.get(&edge.source), group_key.get(&edge.target))
        else {
            continue;
        };
        if source == target {
            if let Some(group) = groups.get_mut(&source) {
                group.internal_import_count += 1;
            }
        } else {
            *counts.entry((source, target)).or_default() += 1;
        }
    }

    let mut ordered: Vec<(GroupKey<'_>, DirectoryGroup)> = groups.into_iter().collect();
    ordered.sort_by(|a, b| {
        b.1.file_count
            .cmp(&a.1.file_count)
            .then_with(|| a.0.cmp(&b.0))
    });
    let hidden = ordered.split_off(ordered.len().min(limit as usize));
    let position: HashMap<GroupKey<'_>, u32> = ordered
        .iter()
        .enumerate()
        .map(|(index, (key, _))| (*key, index as u32))
        .collect();

    let mut links: Vec<DirectoryLink> = counts
        .into_iter()
        .filter_map(|((source, target), import_count)| {
            Some(DirectoryLink {
                source: *position.get(&source)?,
                target: *position.get(&target)?,
                import_count,
            })
        })
        .collect();
    links.sort_by_key(|link| (link.source, link.target));

    DirectoryMap {
        commit,
        hidden_group_count: hidden.len() as u32,
        hidden_file_count: hidden.iter().map(|(_, group)| group.file_count).sum(),
        groups: ordered.into_iter().map(|(_, group)| group).collect(),
        links,
        matching_node_count: under_root.len() as u32,
        total_node_count: nodes.len() as u32,
        total_edge_count: edges.len() as u32,
        directory: root,
    }
}

/// Pairs meeting minimum support as of `commit`, among `kept` files.
fn cochange_edges(
    database: &Database,
    repository: &RepositoryEntry,
    commit: &Oid,
    kept: &HashSet<&RepoPath>,
    config: &AtlasConfig,
) -> Result<Vec<CoChangeEdge>, AtlasError> {
    let git = GitRepository::open(Path::new(&repository.path))?;
    let history = load_scoped_history(
        database,
        &git,
        repository.id,
        commit,
        DEFAULT_HISTORY_WINDOW,
        config,
    )?;
    let alive = history.alive_paths();

    let mut edges = Vec::new();
    for (a, b, shared) in history.index.pairs() {
        if (shared.len() as u32) < config.analysis.min_shared_commits {
            continue;
        }
        let (Some(&path_a), Some(&path_b)) = (alive.get(&a), alive.get(&b)) else {
            continue;
        };
        if !kept.contains(path_a) || !kept.contains(path_b) {
            continue;
        }
        let ordered = if path_a < path_b {
            [(path_a, a), (path_b, b)]
        } else {
            [(path_b, b), (path_a, a)]
        };
        edges.push(CoChangeEdge {
            file_a: ordered[0].0.clone(),
            file_b: ordered[1].0.clone(),
            shared_commit_count: shared.len() as u32,
            file_a_commit_count: history.index.commits_of(ordered[0].1).len() as u32,
            file_b_commit_count: history.index.commits_of(ordered[1].1).len() as u32,
        });
    }
    edges.sort_by(|x, y| (&x.file_a, &x.file_b).cmp(&(&y.file_a, &y.file_b)));
    Ok(edges)
}

/// One file at a commit, with its edges in both directions.
pub fn file_detail(
    database: &Database,
    repository: &RepositoryEntry,
    git_ref: Option<String>,
    path: &RepoPath,
    config: &AtlasConfig,
) -> Result<FileDetail, AtlasError> {
    let snapshot = load(database, repository, git_ref.as_deref(), config)?;
    let architecture = &snapshot.architecture;
    let node = architecture
        .nodes
        .iter()
        .find(|node| &node.path == path)
        .cloned()
        .ok_or_else(|| {
            AtlasError::NotFound(format!("{path} is not a source file at this commit"))
        })?;

    // Activity is known for the analyzed commit. At any other commit it would
    // include work from after that commit.
    let churn = match database.latest_run(repository.id)? {
        Some(run) if run.target_oid == snapshot.commit => {
            run_churn(&database.changes(run.id)?, database.identities(run.id)?)
                .into_iter()
                .find(|file| &file.path == path && file.exists_at_tip)
        }
        _ => None,
    };

    let (imports, imported_by) = (
        architecture
            .edges
            .iter()
            .filter(|edge| &edge.source == path)
            .cloned()
            .collect(),
        architecture
            .edges
            .iter()
            .filter(|edge| &edge.target == path)
            .cloned()
            .collect(),
    );
    Ok(FileDetail {
        unresolved: database.unresolved_for_file(snapshot.id, path)?,
        external_import_count: database.external_imports_for_file(snapshot.id, path)?,
        commit: snapshot.commit,
        node,
        imports,
        imported_by,
        churn,
    })
}

#[cfg(test)]
mod tests {
    use atlas_model::{
        DependencyEdgeType, DirectoryGroupKind, GraphEdge, GraphNode, Language, ParseStatus,
        ResolutionKind,
    };
    use proptest::prelude::*;

    use super::in_directory;

    fn group_by_directory(
        nodes: &[GraphNode],
        edges: &[GraphEdge],
        directory: &str,
        limit: u32,
    ) -> atlas_model::DirectoryMap {
        let commit = "a".repeat(40).parse().unwrap();
        super::group_by_directory(commit, nodes, edges, directory, limit)
    }

    fn node(path: &str, in_cycle: bool) -> GraphNode {
        GraphNode {
            path: path.parse().unwrap(),
            language: Language::Typescript,
            loc: 1,
            parse_status: ParseStatus::Parsed,
            in_degree: 0,
            out_degree: 0,
            in_cycle,
        }
    }

    fn edge(source: &str, target: &str) -> GraphEdge {
        GraphEdge {
            source: source.parse().unwrap(),
            target: target.parse().unwrap(),
            edge_type: DependencyEdgeType::StaticImport,
            resolution: ResolutionKind::Relative,
            specifier: "./x".to_owned(),
            line: 1,
        }
    }

    #[test]
    fn groups_count_files_cycles_and_imports() {
        let nodes = [
            node("src/a/one.ts", true),
            node("src/a/two.ts", false),
            node("src/b/deep/three.ts", false),
            node("src/index.ts", false),
        ];
        let edges = [
            edge("src/a/one.ts", "src/a/two.ts"),
            edge("src/a/one.ts", "src/b/deep/three.ts"),
            edge("src/a/two.ts", "src/b/deep/three.ts"),
            edge("src/index.ts", "src/a/one.ts"),
        ];
        let map = group_by_directory(&nodes, &edges, "", 10);
        assert_eq!(map.directory, "src");
        let paths: Vec<_> = map.groups.iter().map(|group| group.path.as_str()).collect();
        assert_eq!(paths, ["src/a", "src", "src/b"]);
        assert_eq!(map.groups[0].file_count, 2);
        assert_eq!(map.groups[0].cyclic_file_count, 1);
        assert_eq!(map.groups[0].internal_import_count, 1);
        assert_eq!(map.groups[1].kind, DirectoryGroupKind::LooseFiles);
        assert_eq!(map.links.len(), 2);
        assert_eq!(
            (
                map.links[0].source,
                map.links[0].target,
                map.links[0].import_count
            ),
            (0, 2, 2)
        );
    }

    #[test]
    fn a_directory_can_be_opened_and_keeps_only_its_own_imports() {
        let nodes = [
            node("src/a/one.ts", false),
            node("src/a/x/two.ts", false),
            node("src/b/three.ts", false),
        ];
        let edges = [
            edge("src/a/one.ts", "src/a/x/two.ts"),
            edge("src/a/one.ts", "src/b/three.ts"),
        ];
        let map = group_by_directory(&nodes, &edges, "src/a/", 10);
        assert_eq!(map.directory, "src/a");
        assert_eq!(map.matching_node_count, 2);
        assert_eq!(map.groups.len(), 2);
        assert_eq!(map.links.len(), 1);
        assert_eq!(map.links[0].import_count, 1);
    }

    #[test]
    fn the_limit_keeps_the_largest_groups_and_says_what_it_hid() {
        let nodes = [
            node("a/1.ts", false),
            node("a/2.ts", false),
            node("b/1.ts", false),
            node("c/1.ts", false),
        ];
        let edges = [edge("a/1.ts", "c/1.ts")];
        let map = group_by_directory(&nodes, &edges, "", 2);
        assert_eq!(map.groups.len(), 2);
        assert_eq!(map.hidden_group_count, 1);
        assert_eq!(map.hidden_file_count, 1);
        assert!(map.links.is_empty());
    }

    #[test]
    fn a_missing_directory_is_empty_rather_than_an_error() {
        let map = group_by_directory(&[node("a/1.ts", false)], &[], "nowhere", 10);
        assert!(map.groups.is_empty());
        assert_eq!(map.matching_node_count, 0);
        assert_eq!(map.total_node_count, 1);
    }

    proptest! {
        /// Every file lands in exactly one group, and every import between files
        /// under the map is counted exactly once.
        #[test]
        fn grouping_loses_nothing(
            files in proptest::collection::btree_set("[ab]/[cd]/[ef]\\.ts|[ab]/[ef]\\.ts|[ef]\\.ts", 1..12),
            picks in proptest::collection::vec((0usize..12, 0usize..12), 0..20),
        ) {
            let files: Vec<String> = files.into_iter().collect();
            let nodes: Vec<GraphNode> = files.iter().map(|file| node(file, false)).collect();
            let mut pairs: Vec<(usize, usize)> = picks
                .into_iter()
                .map(|(a, b)| (a % files.len(), b % files.len()))
                .filter(|(a, b)| a != b)
                .collect();
            pairs.sort_unstable();
            pairs.dedup();
            let edges: Vec<GraphEdge> =
                pairs.iter().map(|&(a, b)| edge(&files[a], &files[b])).collect();

            let map = group_by_directory(&nodes, &edges, "", 100);
            let files_seen: u32 = map.groups.iter().map(|group| group.file_count).sum();
            let inside: u32 = map.groups.iter().map(|group| group.internal_import_count).sum();
            let across: u32 = map.links.iter().map(|link| link.import_count).sum();
            prop_assert_eq!(files_seen as usize, nodes.len());
            prop_assert_eq!((inside + across) as usize, edges.len());
        }
    }

    #[test]
    fn directories_match_whole_segments() {
        let path = "src/auth/session.ts".parse().unwrap();
        assert!(in_directory(&path, "src"));
        assert!(in_directory(&path, "src/auth/"));
        assert!(in_directory(&path, ""));
        assert!(!in_directory(&path, "src/au"));
        assert!(!in_directory(&path, "auth"));
    }
}
