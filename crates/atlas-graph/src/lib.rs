//! Directed dependency graphs and their algorithms.
//!
//! Edge convention, shared by engine, API, docs and UI: `A -> B` means file `A`
//! imports file `B`, so reverse traversal from `B` finds what `B` may impact.
//! Import edges show dependencies in source, not what executes.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use atlas_model::{Cycle, RepoPath};
use petgraph::Direction;
use petgraph::algo::tarjan_scc;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeFiltered;

/// An import graph with at most one edge per ordered pair of files.
#[derive(Debug, Clone, Default)]
pub struct DependencyGraph {
    graph: DiGraph<RepoPath, EdgeKind>,
    indices: HashMap<RepoPath, NodeIndex>,
}

#[derive(Debug, Clone, Copy)]
struct EdgeKind {
    /// Every import between the pair is erased at compile time.
    type_only: bool,
}

impl DependencyGraph {
    /// Creates a graph from its nodes.
    pub fn new(nodes: impl IntoIterator<Item = RepoPath>) -> Self {
        let mut result = Self::default();
        for path in nodes {
            result
                .indices
                .entry(path.clone())
                .or_insert_with(|| result.graph.add_node(path));
        }
        result
    }

    /// Records that `source` imports `target`. Edges naming unknown nodes are
    /// ignored, and a pair is type-only only if all its imports are.
    pub fn add_edge(&mut self, source: &RepoPath, target: &RepoPath, type_only: bool) {
        let (Some(&from), Some(&to)) = (self.indices.get(source), self.indices.get(target)) else {
            return;
        };
        match self.graph.find_edge(from, to) {
            Some(edge) => self.graph[edge].type_only &= type_only,
            None => {
                self.graph.add_edge(from, to, EdgeKind { type_only });
            }
        }
    }

    /// Distinct files importing `path`.
    pub fn in_degree(&self, path: &RepoPath) -> u32 {
        self.degree(path, Direction::Incoming)
    }

    /// Distinct files that `path` imports.
    pub fn out_degree(&self, path: &RepoPath) -> u32 {
        self.degree(path, Direction::Outgoing)
    }

    fn degree(&self, path: &RepoPath, direction: Direction) -> u32 {
        self.indices.get(path).map_or(0, |&node| {
            self.graph.neighbors_directed(node, direction).count() as u32
        })
    }

    /// All cycles: strongly connected components with more than one file, or one
    /// file importing itself. Largest first, then by first member.
    pub fn cycles(&self) -> Vec<Cycle> {
        let value_edges = EdgeFiltered::from_fn(&self.graph, |edge| !edge.weight().type_only);
        let runtime_members: HashMap<NodeIndex, usize> = tarjan_scc(&value_edges)
            .into_iter()
            .enumerate()
            .filter(|(_, component)| self.is_cycle(component, false))
            .flat_map(|(id, component)| component.into_iter().map(move |node| (node, id)))
            .collect();

        let mut cycles: Vec<Cycle> = tarjan_scc(&self.graph)
            .into_iter()
            .filter(|component| self.is_cycle(component, true))
            .map(|component| {
                let type_only = !component
                    .iter()
                    .any(|node| runtime_members.contains_key(node));
                let mut members: Vec<RepoPath> = component
                    .iter()
                    .map(|&node| self.graph[node].clone())
                    .collect();
                members.sort();
                Cycle { members, type_only }
            })
            .collect();
        cycles.sort_by(|a, b| {
            b.members
                .len()
                .cmp(&a.members.len())
                .then_with(|| a.members.cmp(&b.members))
        });
        cycles
    }

    fn is_cycle(&self, component: &[NodeIndex], include_type_only: bool) -> bool {
        match component {
            [node] => self
                .graph
                .find_edge(*node, *node)
                .is_some_and(|edge| include_type_only || !self.graph[edge].type_only),
            _ => true,
        }
    }

    /// Files that depend on `start`, directly or transitively, within
    /// `max_depth` hops, with their shortest distance. `start` is left out.
    pub fn reverse_distances(&self, start: &RepoPath, max_depth: u32) -> BTreeMap<RepoPath, u32> {
        self.reverse_paths(start, max_depth)
            .into_iter()
            .map(|(path, chain)| (path, chain.len() as u32 - 1))
            .collect()
    }

    /// Like [`Self::reverse_distances`], but gives each dependent's import chain:
    /// the dependent first and `start` last, so each file imports the next.
    pub fn reverse_paths(
        &self,
        start: &RepoPath,
        max_depth: u32,
    ) -> BTreeMap<RepoPath, Vec<RepoPath>> {
        let mut chains = BTreeMap::new();
        let Some(&origin) = self.indices.get(start) else {
            return chains;
        };
        let mut seen = HashSet::from([origin]);
        let mut queue = VecDeque::from([(origin, vec![start.clone()])]);
        while let Some((node, chain)) = queue.pop_front() {
            if chain.len() as u32 > max_depth {
                continue;
            }
            let mut dependents: Vec<_> = self
                .graph
                .neighbors_directed(node, Direction::Incoming)
                .collect();
            dependents.sort_by(|a, b| self.graph[*a].cmp(&self.graph[*b]));
            for dependent in dependents {
                if seen.insert(dependent) {
                    let path = self.graph[dependent].clone();
                    let mut extended = Vec::with_capacity(chain.len() + 1);
                    extended.push(path.clone());
                    extended.extend(chain.iter().cloned());
                    chains.insert(path, extended.clone());
                    queue.push_back((dependent, extended));
                }
            }
        }
        chains
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use atlas_model::RepoPath;
    use proptest::prelude::*;

    use super::DependencyGraph;

    fn p(name: &str) -> RepoPath {
        name.parse().unwrap()
    }

    fn graph(nodes: &[&str], edges: &[(&str, &str)]) -> DependencyGraph {
        let mut graph = DependencyGraph::new(nodes.iter().map(|n| p(n)));
        for (source, target) in edges {
            graph.add_edge(&p(source), &p(target), false);
        }
        graph
    }

    fn names(members: &[RepoPath]) -> Vec<&str> {
        members.iter().map(RepoPath::as_str).collect()
    }

    #[test]
    fn edges_point_from_importer_to_imported() {
        let graph = graph(&["a", "b"], &[("a", "b")]);
        assert_eq!(graph.out_degree(&p("a")), 1);
        assert_eq!(graph.in_degree(&p("a")), 0);
        assert_eq!(graph.in_degree(&p("b")), 1);
    }

    #[test]
    fn repeated_imports_count_once() {
        let graph = graph(&["a", "b"], &[("a", "b"), ("a", "b")]);
        assert_eq!(graph.out_degree(&p("a")), 1);
    }

    #[test]
    fn reverse_traversal_finds_dependents_not_dependencies() {
        let graph = graph(&["a", "b", "c", "d"], &[("a", "b"), ("b", "c"), ("c", "d")]);
        let from_c = graph.reverse_distances(&p("c"), 10);
        assert_eq!(from_c.len(), 2);
        assert_eq!(from_c[&p("b")], 1);
        assert_eq!(from_c[&p("a")], 2);
    }

    #[test]
    fn reverse_traversal_respects_depth_and_shortest_paths() {
        let graph = graph(
            &["a", "b", "c", "d"],
            &[("a", "b"), ("b", "c"), ("c", "d"), ("a", "d")],
        );
        let from_d = graph.reverse_distances(&p("d"), 1);
        assert_eq!(from_d.len(), 2);
        assert_eq!(from_d[&p("a")], 1);
        assert_eq!(from_d[&p("c")], 1);
    }

    #[test]
    fn reverse_traversal_terminates_on_cycles_and_excludes_the_start() {
        let graph = graph(&["a", "b"], &[("a", "b"), ("b", "a")]);
        let from_a = graph.reverse_distances(&p("a"), 10);
        assert_eq!(from_a.len(), 1);
        assert_eq!(from_a[&p("b")], 1);
    }

    #[test]
    fn reverse_paths_list_the_import_chain() {
        let graph = graph(&["a", "b", "c"], &[("a", "b"), ("b", "c")]);
        let chains = graph.reverse_paths(&p("c"), 5);
        assert_eq!(names(&chains[&p("a")]), ["a", "b", "c"]);
        assert_eq!(names(&chains[&p("b")]), ["b", "c"]);
    }

    #[test]
    fn unknown_files_have_no_dependents() {
        assert!(graph(&["a"], &[]).reverse_distances(&p("z"), 3).is_empty());
    }

    #[test]
    fn cycles_are_normalized_and_acyclic_nodes_are_left_out() {
        let graph = graph(
            &["c", "b", "a", "d"],
            &[("c", "b"), ("b", "a"), ("a", "c"), ("d", "a")],
        );
        let cycles = graph.cycles();
        assert_eq!(cycles.len(), 1);
        assert_eq!(names(&cycles[0].members), ["a", "b", "c"]);
        assert!(!cycles[0].type_only);
    }

    #[test]
    fn a_chain_has_no_cycles() {
        assert!(
            graph(&["a", "b", "c"], &[("a", "b"), ("b", "c")])
                .cycles()
                .is_empty()
        );
    }

    #[test]
    fn a_self_import_is_a_cycle() {
        let cycles = graph(&["a"], &[("a", "a")]).cycles();
        assert_eq!(names(&cycles[0].members), ["a"]);
    }

    #[test]
    fn largest_cycles_come_first() {
        let graph = graph(
            &["a", "b", "x", "y", "z"],
            &[("a", "b"), ("b", "a"), ("x", "y"), ("y", "z"), ("z", "x")],
        );
        let sizes: Vec<_> = graph.cycles().iter().map(|c| c.members.len()).collect();
        assert_eq!(sizes, [3, 2]);
    }

    #[test]
    fn type_only_cycles_are_marked() {
        let mut graph = graph(&["a", "b"], &[("a", "b")]);
        graph.add_edge(&p("b"), &p("a"), true);
        assert!(graph.cycles()[0].type_only);
    }

    #[test]
    fn a_value_import_makes_a_pair_a_runtime_edge() {
        let mut graph = graph(&["a", "b"], &[("a", "b")]);
        graph.add_edge(&p("b"), &p("a"), true);
        graph.add_edge(&p("b"), &p("a"), false);
        assert!(!graph.cycles()[0].type_only);
    }

    proptest! {
        /// Checks reverse distances against a slow fixed-point computation.
        #[test]
        fn reverse_distances_match_brute_force(
            edges in prop::collection::vec((0usize..8, 0usize..8), 0..30),
            depth in 1u32..6,
        ) {
            let name = |i: usize| p(&format!("f{i}"));
            let mut graph = DependencyGraph::new((0..8).map(name));
            for (a, b) in &edges {
                graph.add_edge(&name(*a), &name(*b), false);
            }

            let mut best = [u32::MAX; 8];
            best[0] = 0;
            for _ in 0..8 {
                for (a, b) in &edges {
                    if best[*b] != u32::MAX {
                        best[*a] = best[*a].min(best[*b] + 1);
                    }
                }
            }

            let found = graph.reverse_distances(&name(0), depth);
            for (i, &distance) in best.iter().enumerate().skip(1) {
                let expected = (distance <= depth).then_some(distance);
                prop_assert_eq!(found.get(&name(i)).copied(), expected);
            }
        }

        /// Members of a reported cycle reach each other; nodes outside do not
        /// reach and get reached by all of them.
        #[test]
        fn cycles_are_mutually_reachable(edges in prop::collection::vec((0usize..7, 0usize..7), 0..20)) {
            let name = |i: usize| p(&format!("f{i}"));
            let mut graph = DependencyGraph::new((0..7).map(name));
            for (a, b) in &edges {
                graph.add_edge(&name(*a), &name(*b), false);
            }
            for cycle in graph.cycles() {
                let members: BTreeSet<_> = cycle.members.iter().cloned().collect();
                for member in &members {
                    let reachers = graph.reverse_distances(member, 100);
                    for other in members.iter().filter(|m| *m != member) {
                        prop_assert!(reachers.contains_key(other));
                    }
                }
            }
        }
    }
}
