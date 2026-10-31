//! Read models over stored snapshots, shared by the CLI and the API.

use std::collections::HashSet;
use std::path::Path;

use atlas_git::GitRepository;
use atlas_graph::DependencyGraph;
use atlas_model::{
    AtlasConfig, AtlasError, CoChangeEdge, FileDetail, GraphEdge, GraphNode, GraphResponse, Oid,
    RepoPath, RepositoryEntry,
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

    let mut nodes: Vec<GraphNode> = architecture
        .nodes
        .into_iter()
        .filter(|node| {
            query
                .directory
                .as_deref()
                .is_none_or(|directory| in_directory(&node.path, directory))
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
        cochange_edges(repository, &snapshot.commit, &kept, config)?
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

/// Pairs meeting minimum support as of `commit`, among `kept` files.
fn cochange_edges(
    repository: &RepositoryEntry,
    commit: &Oid,
    kept: &HashSet<&RepoPath>,
    config: &AtlasConfig,
) -> Result<Vec<CoChangeEdge>, AtlasError> {
    let git = GitRepository::open(Path::new(&repository.path))?;
    let history = load_scoped_history(&git, commit, DEFAULT_HISTORY_WINDOW, config)?;
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

    let churn = match database.latest_run(repository.id)? {
        Some(run) => run_churn(&database.changes(run.id)?, database.identities(run.id)?)
            .into_iter()
            .find(|file| &file.path == path && file.exists_at_tip),
        None => None,
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
    use super::in_directory;

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
