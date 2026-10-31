//! Read models over stored snapshots, shared by the CLI and the API.

use std::collections::HashSet;
use std::path::Path;

use atlas_git::GitRepository;
use atlas_graph::DependencyGraph;
use atlas_model::{
    AtlasConfig, AtlasError, FileDetail, GraphEdge, GraphNode, GraphResponse, Oid, RepoPath,
    RepositoryEntry,
};
use atlas_storage::{Database, SnapshotId};

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
}

struct Snapshot {
    id: SnapshotId,
    commit: Oid,
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    cycles: Vec<atlas_model::Cycle>,
}

fn load(
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

    let mut nodes = database.snapshot_nodes(id)?;
    let edges = database.snapshot_edges(id)?;
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
    Ok(Snapshot {
        id,
        commit,
        nodes,
        edges,
        cycles,
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
    let total_node_count = snapshot.nodes.len() as u32;
    let total_edge_count = snapshot.edges.len() as u32;

    let mut nodes: Vec<GraphNode> = snapshot
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
    let edges = snapshot
        .edges
        .into_iter()
        .filter(|edge| kept.contains(&edge.source) && kept.contains(&edge.target))
        .collect();

    Ok(GraphResponse {
        coverage: database.snapshot_coverage(snapshot.id)?,
        commit: snapshot.commit,
        cycles: snapshot.cycles,
        nodes,
        edges,
        matching_node_count,
        total_node_count,
        total_edge_count,
    })
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
    let node = snapshot
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
        snapshot
            .edges
            .iter()
            .filter(|edge| &edge.source == path)
            .cloned()
            .collect(),
        snapshot
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
