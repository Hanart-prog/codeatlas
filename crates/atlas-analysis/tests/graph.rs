//! Compares import graphs on generated repositories with hand-derived facts.

use std::path::Path;

use atlas_analysis::{AnalyzeOptions, GraphQuery, analyze, file_detail, graph};
use atlas_fixtures::{FixtureRepo, exports, generate_main_fixture};
use atlas_model::DependencyEdgeType::{DynamicImport, ReExport, StaticImport, TypeOnlyImport};
use atlas_model::{
    AtlasConfig, GraphResponse, ParseStatus, RepositoryEntry, ResolutionKind, TsconfigStatus,
    UnresolvedReason,
};
use atlas_storage::Database;
use serde::Deserialize;

#[derive(Deserialize)]
struct ExpectedFile {
    snapshots: Vec<ExpectedSnapshot>,
}

#[derive(Deserialize)]
struct ExpectedSnapshot {
    commit_subject: String,
    nodes: Vec<String>,
    edges: Vec<(String, String)>,
    cycles: Vec<Vec<String>>,
}

fn setup(path: &Path) -> (Database, RepositoryEntry, AtlasConfig) {
    let config = AtlasConfig::default();
    let mut database = Database::open_in_memory().unwrap();
    let outcome = analyze(&mut database, path, &AnalyzeOptions::default(), &config).unwrap();
    (database, outcome.repository, config)
}

fn query(git_ref: Option<String>) -> GraphQuery {
    GraphQuery {
        git_ref,
        directory: None,
        search: None,
        limit: 1000,
        include_cochange: false,
    }
}

fn at(
    database: &Database,
    repository: &RepositoryEntry,
    config: &AtlasConfig,
    git_ref: Option<String>,
) -> GraphResponse {
    graph(database, repository, &query(git_ref), config).unwrap()
}

fn edge_pairs(response: &GraphResponse) -> Vec<(String, String)> {
    let mut pairs: Vec<_> = response
        .edges
        .iter()
        .map(|e| (e.source.to_string(), e.target.to_string()))
        .collect();
    pairs.sort();
    pairs
}

fn oid_of(database: &Database, subject: &str) -> String {
    database
        .connection()
        .query_row(
            "SELECT oid FROM commits WHERE message_subject = ?1",
            [subject],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn snapshots_match_hand_derived_graphs_at_each_commit() {
    let dir = tempfile::tempdir().unwrap();
    generate_main_fixture(dir.path()).unwrap();
    let (database, repository, config) = setup(dir.path());
    let expected: ExpectedFile =
        serde_json::from_str(include_str!("../../../fixtures/expected/graph.json")).unwrap();

    for snapshot in expected.snapshots {
        let oid = oid_of(&database, &snapshot.commit_subject);
        let response = at(&database, &repository, &config, Some(oid.clone()));
        assert_eq!(response.commit.as_str(), oid);

        let mut nodes: Vec<_> = response.nodes.iter().map(|n| n.path.to_string()).collect();
        nodes.sort();
        assert_eq!(nodes, snapshot.nodes, "{}", snapshot.commit_subject);
        assert_eq!(
            edge_pairs(&response),
            snapshot.edges,
            "{}",
            snapshot.commit_subject
        );

        let cycles: Vec<Vec<String>> = response
            .cycles
            .iter()
            .map(|c| c.members.iter().map(ToString::to_string).collect())
            .collect();
        assert_eq!(cycles, snapshot.cycles, "{}", snapshot.commit_subject);
    }
}

#[test]
fn history_does_not_see_later_files() {
    let dir = tempfile::tempdir().unwrap();
    generate_main_fixture(dir.path()).unwrap();
    let (database, repository, config) = setup(dir.path());
    let early = oid_of(&database, "Create auth, API and users modules");
    let response = at(&database, &repository, &config, Some(early));
    assert!(
        response
            .nodes
            .iter()
            .all(|n| !n.path.as_str().contains("billing"))
    );
}

#[test]
fn degrees_and_cycle_flags_describe_the_cycle_commit() {
    let dir = tempfile::tempdir().unwrap();
    generate_main_fixture(dir.path()).unwrap();
    let (database, repository, config) = setup(dir.path());
    let oid = oid_of(&database, "Introduce session and token cycle");
    let response = at(&database, &repository, &config, Some(oid));

    let session = response
        .nodes
        .iter()
        .find(|n| n.path.as_str() == "src/auth/session.ts")
        .unwrap();
    assert_eq!((session.in_degree, session.out_degree), (4, 1));
    assert!(session.in_cycle);
    let account = response
        .nodes
        .iter()
        .find(|n| n.path.as_str() == "src/users/account.ts")
        .unwrap();
    assert!(!account.in_cycle);
    assert_eq!(response.nodes[0].path.as_str(), "src/auth/session.ts");
}

/// A repository covering each resolution path, with facts stated inline.
fn import_repository(path: &Path) -> FixtureRepo {
    let mut repo = FixtureRepo::init(path).unwrap();
    repo.change()
        .unwrap()
        .write(
            "tsconfig.json",
            r#"{ // aliases
              "compilerOptions": { "paths": { "@/*": ["./src/*"] } }
            }"#,
        )
        .write("src/lib.ts", exports("lib", 1))
        .write("src/types.ts", "export type T = string;\n")
        .write("src/esm.ts", exports("esm", 1))
        .write("src/page.tsx", "export const page = <div>hi</div>;\n")
        .write("src/style.css", "a {}\n")
        .write(
            "src/app.tsx",
            "import { lib0 } from '@/lib';\n\
             import type { T } from './types';\n\
             import { esm0 } from './esm.js';\n\
             export * from './lib';\n\
             import './style.css';\n\
             import React from 'react';\n\
             import { x } from '@/missing';\n\
             import { y } from './gone';\n\
             const page = () => import('./page');\n\
             const dynamic = (n: string) => import(n);\n\
             const old = require('./lib');\n\
             export const app = <div />;\n",
        )
        .write("src/broken.ts", "import './lib';\nfunction ( {{{\n")
        .write(
            "src/pointer.ts",
            "version https://git-lfs.github.com/spec/v1\noid sha256:a\nsize 1\n",
        )
        .write("dist/bundle.ts", "import './x';\n")
        .commit("Add imports")
        .unwrap();
    repo
}

#[test]
fn every_import_form_is_classified() {
    let dir = tempfile::tempdir().unwrap();
    import_repository(dir.path());
    let (database, repository, config) = setup(dir.path());
    let response = at(&database, &repository, &config, None);

    let from_app: Vec<_> = response
        .edges
        .iter()
        .filter(|e| e.source.as_str() == "src/app.tsx")
        .map(|e| (e.target.as_str(), e.edge_type, e.resolution))
        .collect();
    assert_eq!(
        from_app,
        [
            ("src/esm.ts", StaticImport, ResolutionKind::Relative),
            ("src/lib.ts", ReExport, ResolutionKind::Relative),
            ("src/lib.ts", StaticImport, ResolutionKind::Alias),
            ("src/page.tsx", DynamicImport, ResolutionKind::Relative),
            ("src/types.ts", TypeOnlyImport, ResolutionKind::Relative),
        ]
    );

    let coverage = &response.coverage;
    assert_eq!(coverage.tsconfig, TsconfigStatus::Applied);
    assert_eq!(coverage.parsed_file_count, 6);
    assert_eq!(coverage.partial_file_count, 1);
    assert_eq!(coverage.skipped_file_count, 1);
    assert_eq!(coverage.excluded_file_count, 1);
    assert_eq!(coverage.unsupported_file_count, 2);
    assert_eq!(coverage.resolved_import_count, 6);
    assert_eq!(coverage.external_import_count, 1);
    assert_eq!(coverage.unresolved_import_count, 3);
    assert_eq!(coverage.nonliteral_dynamic_import_count, 1);
    assert_eq!(coverage.require_call_count, 1);

    let path = "src/app.tsx".parse().unwrap();
    let detail = file_detail(&database, &repository, None, &path, &config).unwrap();
    assert_eq!(detail.external_import_count, 1);
    let reasons: Vec<_> = detail
        .unresolved
        .iter()
        .map(|u| (u.line, u.reason))
        .collect();
    assert_eq!(
        reasons,
        [
            (5, UnresolvedReason::NonSourceTarget),
            (7, UnresolvedReason::AliasTargetMissing),
            (8, UnresolvedReason::NotFound),
        ]
    );

    let pointer = response
        .nodes
        .iter()
        .find(|n| n.path.as_str() == "src/pointer.ts")
        .unwrap();
    assert_eq!(pointer.parse_status, ParseStatus::LfsPointer);
    assert!(
        response
            .nodes
            .iter()
            .all(|n| !n.path.as_str().starts_with("dist/"))
    );
}

#[test]
fn a_type_only_cycle_is_labelled() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = FixtureRepo::init(dir.path()).unwrap();
    repo.change()
        .unwrap()
        .write(
            "a.ts",
            "import { b } from './b';\nexport type A = string;\n",
        )
        .write(
            "b.ts",
            "import type { A } from './a';\nexport const b = 1;\n",
        )
        .commit("one")
        .unwrap();
    let (database, repository, config) = setup(dir.path());
    let response = at(&database, &repository, &config, None);
    assert_eq!(response.cycles.len(), 1);
    assert!(response.cycles[0].type_only);
}

#[test]
fn identical_blobs_are_parsed_once_across_commits() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = import_repository(dir.path());
    let first = repo.head().unwrap();
    repo.change()
        .unwrap()
        .write("src/new.ts", "export const n = 1;\n")
        .commit("two")
        .unwrap();
    let (database, repository, config) = setup(dir.path());
    at(&database, &repository, &config, Some(first.to_string()));

    let blobs: i64 = database
        .connection()
        .query_row("SELECT count(*) FROM parsed_blobs", [], |row| row.get(0))
        .unwrap();
    assert_eq!(blobs, 8, "7 sources plus src/new.ts");
}

#[test]
fn identical_content_is_parsed_per_grammar() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = FixtureRepo::init(dir.path()).unwrap();
    let source = "const x = <string>y;\n";
    repo.change()
        .unwrap()
        .write("cast.ts", source)
        .write("cast.tsx", source)
        .commit("one")
        .unwrap();
    let (database, repository, config) = setup(dir.path());
    let response = at(&database, &repository, &config, None);

    let status = |path: &str| {
        let node = response
            .nodes
            .iter()
            .find(|n| n.path.as_str() == path)
            .unwrap();
        node.parse_status
    };
    assert_eq!(status("cast.ts"), ParseStatus::Parsed);
    assert_eq!(status("cast.tsx"), ParseStatus::Partial);
}

#[test]
fn snapshots_are_reused() {
    let dir = tempfile::tempdir().unwrap();
    generate_main_fixture(dir.path()).unwrap();
    let (database, repository, config) = setup(dir.path());
    let first = at(&database, &repository, &config, None);
    let second = at(&database, &repository, &config, None);
    assert_eq!(first, second);
    let count: i64 = database
        .connection()
        .query_row("SELECT count(*) FROM snapshots", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn directory_and_limit_filter_nodes_but_not_totals_or_degrees() {
    let dir = tempfile::tempdir().unwrap();
    generate_main_fixture(dir.path()).unwrap();
    let (database, repository, config) = setup(dir.path());
    let query = GraphQuery {
        git_ref: None,
        directory: Some("src/auth".to_owned()),
        search: None,
        limit: 1,
        include_cochange: false,
    };
    let response = graph(&database, &repository, &query, &config).unwrap();
    assert_eq!(response.nodes.len(), 1);
    assert_eq!(response.matching_node_count, 2);
    assert_eq!(response.total_node_count, 8);
    assert_eq!(response.total_edge_count, 4);
    assert_eq!(response.nodes[0].path.as_str(), "src/auth/session.ts");
    assert_eq!(response.nodes[0].in_degree, 2);
    assert!(response.edges.is_empty());
}

#[test]
fn file_detail_shows_both_directions_and_churn() {
    let dir = tempfile::tempdir().unwrap();
    generate_main_fixture(dir.path()).unwrap();
    let (database, repository, config) = setup(dir.path());
    let path = "src/auth/session.ts".parse().unwrap();
    let detail = file_detail(&database, &repository, None, &path, &config).unwrap();

    let imports: Vec<_> = detail.imports.iter().map(|e| e.target.as_str()).collect();
    assert_eq!(imports, ["src/auth/token.ts"]);
    let importers: Vec<_> = detail
        .imported_by
        .iter()
        .map(|e| e.source.as_str())
        .collect();
    assert_eq!(importers, ["src/api/routes.ts", "src/billing/invoice.ts"]);
    assert_eq!(detail.churn.unwrap().commit_count, 4);
}

#[test]
fn unknown_files_and_refs_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    generate_main_fixture(dir.path()).unwrap();
    let (database, repository, config) = setup(dir.path());
    let path = "src/nope.ts".parse().unwrap();
    assert!(file_detail(&database, &repository, None, &path, &config).is_err());
    assert!(
        graph(
            &database,
            &repository,
            &query(Some("no-such-ref".to_owned())),
            &config
        )
        .is_err()
    );
}

#[test]
fn graph_limits_are_enforced() {
    let dir = tempfile::tempdir().unwrap();
    generate_main_fixture(dir.path()).unwrap();
    let mut config = AtlasConfig::default();
    config.limits.max_graph_nodes = 2;
    let mut database = Database::open_in_memory().unwrap();
    let outcome = analyze(
        &mut database,
        dir.path(),
        &AnalyzeOptions::default(),
        &config,
    )
    .unwrap();
    assert!(outcome.graph_skipped.unwrap().contains("max_graph_nodes"));

    let query = query(None);
    let error = graph(&database, &outcome.repository, &query, &config).unwrap_err();
    assert!(error.to_string().contains("max_graph_nodes"), "{error}");
}

#[test]
fn oversized_files_are_counted_not_parsed() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = FixtureRepo::init(dir.path()).unwrap();
    repo.change()
        .unwrap()
        .write("big.ts", exports("a", 200))
        .write("small.ts", "export {};\n")
        .commit("one")
        .unwrap();
    let mut config = AtlasConfig::default();
    config.limits.max_file_bytes = 100;
    let mut database = Database::open_in_memory().unwrap();
    let outcome = analyze(
        &mut database,
        dir.path(),
        &AnalyzeOptions::default(),
        &config,
    )
    .unwrap();
    let response = graph(&database, &outcome.repository, &query(None), &config).unwrap();
    assert_eq!(response.coverage.skipped_file_count, 1);
    let big = response
        .nodes
        .iter()
        .find(|n| n.path.as_str() == "big.ts")
        .unwrap();
    assert_eq!(big.parse_status, ParseStatus::Oversized);
}

#[test]
fn working_tree_files_are_ignored() {
    let dir = tempfile::tempdir().unwrap();
    generate_main_fixture(dir.path()).unwrap();
    std::fs::write(dir.path().join("planted.ts"), "import './x';\n").unwrap();
    let (database, repository, config) = setup(dir.path());
    let response = at(&database, &repository, &config, None);
    assert!(
        response
            .nodes
            .iter()
            .all(|n| n.path.as_str() != "planted.ts")
    );
}
