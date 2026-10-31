//! Compares impact rankings on the fixture with hand-derived facts.

use std::path::Path;

use atlas_analysis::{AnalyzeOptions, analyze, impact};
use atlas_fixtures::{FixtureRepo, exports, generate_main_fixture};
use atlas_model::{
    AtlasConfig, EvidenceKind, ImpactRecommendation, ImpactRequest, ImpactResponse,
    RepositoryEntry, SelectedFileStatus,
};
use atlas_storage::Database;
use serde::Deserialize;

#[derive(Deserialize)]
struct Expected {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    cutoff_subject: String,
    selected: Vec<String>,
    expected: Vec<Row>,
}

#[derive(Deserialize)]
struct Row {
    path: String,
    score: f64,
    static_score: f64,
    historical_score: f64,
    distance: Option<u32>,
    shared: Option<u32>,
    selected_count: Option<u32>,
    candidate_count: Option<u32>,
}

fn setup(path: &Path) -> (Database, RepositoryEntry, AtlasConfig) {
    let config = AtlasConfig::default();
    let mut database = Database::open_in_memory().unwrap();
    let outcome = analyze(&mut database, path, &AnalyzeOptions::default(), &config).unwrap();
    (database, outcome.repository, config)
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

fn request(paths: &[&str], cutoff: Option<String>) -> ImpactRequest {
    ImpactRequest {
        selected_paths: paths.iter().map(|p| (*p).to_owned()).collect(),
        cutoff,
        ..ImpactRequest::default()
    }
}

fn paths(response: &ImpactResponse) -> Vec<&str> {
    response
        .recommendations
        .iter()
        .map(|r| r.path.as_str())
        .collect()
}

fn close(actual: f64, expected: f64, what: &str) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "{what}: {actual} != {expected}"
    );
}

fn check(row: &Row, item: &ImpactRecommendation, name: &str) {
    assert_eq!(item.path.as_str(), row.path, "{name}");
    close(item.score, row.score, name);
    close(item.static_score, row.static_score, name);
    close(item.historical_score, row.historical_score, name);
    assert_eq!(
        item.static_evidence.as_ref().map(|e| e.distance),
        row.distance,
        "{name}"
    );
    let history = item.historical_evidence.as_ref();
    assert_eq!(history.map(|e| e.shared_commit_count), row.shared, "{name}");
    assert_eq!(
        history.map(|e| e.selected_commit_count),
        row.selected_count,
        "{name}"
    );
    assert_eq!(
        history.map(|e| e.candidate_commit_count),
        row.candidate_count,
        "{name}"
    );
}

#[test]
fn rankings_match_hand_derived_expectations() {
    let dir = tempfile::tempdir().unwrap();
    generate_main_fixture(dir.path()).unwrap();
    let (database, repository, config) = setup(dir.path());
    let expected: Expected =
        serde_json::from_str(include_str!("../../../fixtures/expected/impact.json")).unwrap();

    for case in expected.cases {
        let selected: Vec<&str> = case.selected.iter().map(String::as_str).collect();
        let cutoff = oid_of(&database, &case.cutoff_subject);
        let response = impact(
            &database,
            &repository,
            &request(&selected, Some(cutoff)),
            &config,
        )
        .unwrap();

        assert_eq!(
            response.recommendations.len(),
            case.expected.len(),
            "{}: {:?}",
            case.name,
            paths(&response)
        );
        for (index, (row, item)) in case
            .expected
            .iter()
            .zip(&response.recommendations)
            .enumerate()
        {
            check(row, item, &case.name);
            assert_eq!(item.rank as usize, index + 1, "{}", case.name);
        }
    }
}

#[test]
fn evidence_is_explained() {
    let dir = tempfile::tempdir().unwrap();
    generate_main_fixture(dir.path()).unwrap();
    let (database, repository, config) = setup(dir.path());
    let response = impact(
        &database,
        &repository,
        &request(&["src/auth/session.ts"], None),
        &config,
    )
    .unwrap();

    let top = &response.recommendations[0];
    assert_eq!(
        top.evidence,
        [
            EvidenceKind::StaticDependency,
            EvidenceKind::HistoricalCoChange
        ]
    );
    let path: Vec<_> = top
        .static_evidence
        .as_ref()
        .unwrap()
        .path
        .iter()
        .map(|p| p.as_str())
        .collect();
    assert_eq!(path, ["src/billing/invoice.ts", "src/auth/session.ts"]);

    let history = top.historical_evidence.as_ref().unwrap();
    let subjects: Vec<_> = history.commits.iter().map(|c| c.subject.as_str()).collect();
    assert_eq!(
        subjects,
        [
            "Track device sessions on invoices",
            "Prorate expired sessions",
            "Bill by session seat"
        ]
    );
    close(history.jaccard, 3.0 / 6.0, "jaccard");
    close(history.base_rate, 5.0 / 13.0, "base rate");
    close(history.lift, 0.75 / (5.0 / 13.0), "lift");

    assert_eq!(response.history.commit_count, 13);
    assert_eq!(response.history.eligible_commit_count, 13);
    assert_eq!(response.history.merge_commit_count, 1);

    let routes = &response.recommendations[1];
    assert_eq!(routes.evidence, [EvidenceKind::StaticDependency]);
    assert!(
        routes
            .caveats
            .iter()
            .any(|c| c.contains("1 time(s), below the minimum of 3")),
        "{:?}",
        routes.caveats
    );
}

#[test]
fn history_only_candidates_are_marked_as_correlation() {
    let dir = tempfile::tempdir().unwrap();
    generate_main_fixture(dir.path()).unwrap();
    let (database, repository, config) = setup(dir.path());
    let response = impact(
        &database,
        &repository,
        &request(&["src/billing/invoice.ts"], None),
        &config,
    )
    .unwrap();
    let session = &response.recommendations[1];
    assert_eq!(session.evidence, [EvidenceKind::HistoricalCoChange]);
    assert!(session.caveats[0].contains("correlation"));
}

#[test]
fn future_commits_cannot_change_a_historical_ranking() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = FixtureRepo::init(dir.path()).unwrap();
    repo.change()
        .unwrap()
        .write("a.ts", exports("a", 3))
        .write("b.ts", exports("b", 3))
        .commit("one")
        .unwrap();
    let cutoff = repo.head().unwrap().to_string();

    let (mut database, repository, config) = setup(dir.path());
    let before = impact(
        &database,
        &repository,
        &request(&["a.ts"], Some(cutoff.clone())),
        &config,
    )
    .unwrap();
    assert!(before.recommendations.is_empty());

    // Everything after the cutoff tries to change the answer: repeated
    // co-change, a new importer, and a new import in an existing file.
    for i in 0..5 {
        repo.change()
            .unwrap()
            .append("a.ts", &exports(&format!("x{i}"), 1))
            .append("b.ts", &exports(&format!("y{i}"), 1))
            .commit("together")
            .unwrap();
    }
    repo.change()
        .unwrap()
        .write("c.ts", "import './a';\n")
        .commit("importer")
        .unwrap();
    repo.change()
        .unwrap()
        .append("b.ts", "import './a';\n")
        .commit("import")
        .unwrap();
    analyze(
        &mut database,
        dir.path(),
        &AnalyzeOptions::default(),
        &config,
    )
    .unwrap();

    let after = impact(
        &database,
        &repository,
        &request(&["a.ts"], Some(cutoff)),
        &config,
    )
    .unwrap();
    assert_eq!(before, after);

    let latest = impact(&database, &repository, &request(&["a.ts"], None), &config).unwrap();
    assert_eq!(
        paths(&latest),
        ["b.ts", "c.ts"],
        "the later ranking sees all of it"
    );
}

#[test]
fn head_imports_do_not_leak_into_a_historical_cutoff() {
    let dir = tempfile::tempdir().unwrap();
    generate_main_fixture(dir.path()).unwrap();
    let (database, repository, config) = setup(dir.path());
    let early = oid_of(&database, "Create auth, API and users modules");
    let response = impact(
        &database,
        &repository,
        &request(&["src/auth/session.ts"], Some(early)),
        &config,
    )
    .unwrap();
    assert_eq!(paths(&response), ["src/api/routes.ts"]);
}

#[test]
fn files_resolve_by_their_path_at_the_cutoff() {
    let dir = tempfile::tempdir().unwrap();
    generate_main_fixture(dir.path()).unwrap();
    let (database, repository, config) = setup(dir.path());
    let before_rename = oid_of(&database, "Track device sessions on invoices");

    let old = "src/users/profile.ts";
    let new = "src/users/account.ts";
    assert!(
        impact(
            &database,
            &repository,
            &request(&[old], Some(before_rename.clone())),
            &config
        )
        .is_ok()
    );
    assert!(
        impact(
            &database,
            &repository,
            &request(&[new], Some(before_rename)),
            &config
        )
        .is_err()
    );
    assert!(impact(&database, &repository, &request(&[old], None), &config).is_err());

    let response = impact(&database, &repository, &request(&[new], None), &config).unwrap();
    assert_eq!(
        response.selected[0].commit_count, 3,
        "history follows the rename"
    );
}

#[test]
fn deleted_files_are_never_recommended() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = FixtureRepo::init(dir.path()).unwrap();
    for i in 0..3 {
        let change = repo.change().unwrap();
        let change = if i == 0 {
            change
                .write("a.ts", exports("a", 1))
                .write("gone.ts", exports("g", 1))
        } else {
            change
                .append("a.ts", &exports(&format!("a{i}"), 1))
                .append("gone.ts", &exports(&format!("g{i}"), 1))
        };
        change.commit("both").unwrap();
    }
    repo.change()
        .unwrap()
        .remove("gone.ts")
        .commit("delete")
        .unwrap();
    let (database, repository, config) = setup(dir.path());
    let response = impact(&database, &repository, &request(&["a.ts"], None), &config).unwrap();
    assert!(response.recommendations.is_empty());
}

#[test]
fn bulk_and_non_source_commits_are_excluded_and_reported() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = FixtureRepo::init(dir.path()).unwrap();
    let mut change = repo.change().unwrap();
    for i in 0..4 {
        change = change.write(format!("f{i}.ts"), "x\n");
    }
    change.commit("bulk").unwrap();
    repo.change()
        .unwrap()
        .write("README.md", "x\n")
        .commit("docs")
        .unwrap();

    let mut config = AtlasConfig::default();
    config.analysis.bulk_commit_file_threshold = 3;
    let mut database = Database::open_in_memory().unwrap();
    let outcome = analyze(
        &mut database,
        dir.path(),
        &AnalyzeOptions::default(),
        &config,
    )
    .unwrap();
    let response = impact(
        &database,
        &outcome.repository,
        &request(&["f0.ts"], None),
        &config,
    )
    .unwrap();
    assert_eq!(response.history.bulk_commit_count, 1);
    assert_eq!(response.history.no_eligible_file_commit_count, 1);
    assert_eq!(response.history.eligible_commit_count, 0);
    assert!(
        response
            .notes
            .iter()
            .any(|n| n.contains("no eligible commits"))
    );
}

#[test]
fn missing_evidence_is_explained_rather_than_shown_as_safe() {
    let dir = tempfile::tempdir().unwrap();
    generate_main_fixture(dir.path()).unwrap();
    let (database, repository, config) = setup(dir.path());

    let readme = impact(
        &database,
        &repository,
        &request(&["README.md"], None),
        &config,
    )
    .unwrap();
    assert_eq!(
        readme.selected[0].status,
        SelectedFileStatus::UnsupportedLanguage
    );
    assert!(readme.notes.iter().any(|n| n.contains("not TS or TSX")));
    assert!(
        readme
            .notes
            .iter()
            .any(|n| n.contains("not a sign that the change is safe"))
    );

    let excluded = impact(
        &database,
        &repository,
        &request(&["dist/app.js"], None),
        &config,
    )
    .unwrap();
    assert_eq!(excluded.selected[0].status, SelectedFileStatus::Excluded);

    let lone = impact(
        &database,
        &repository,
        &request(&["src/billing/tax.ts"], None),
        &config,
    )
    .unwrap();
    assert!(lone.notes.iter().any(|n| n.contains("No file imports")));
}

#[test]
fn the_window_limits_history_and_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    generate_main_fixture(dir.path()).unwrap();
    let (database, repository, config) = setup(dir.path());
    let request = ImpactRequest {
        history_window: Some(2),
        ..request(&["src/auth/session.ts"], None)
    };
    let response = impact(&database, &repository, &request, &config).unwrap();
    assert_eq!(response.history.commit_count, 2);
    assert!(response.history.is_truncated);
    assert!(
        response
            .recommendations
            .iter()
            .all(|r| r.historical_score == 0.0)
    );
}

#[test]
fn top_k_and_depth_are_applied_after_ranking() {
    let dir = tempfile::tempdir().unwrap();
    generate_main_fixture(dir.path()).unwrap();
    let (database, repository, config) = setup(dir.path());

    let top_one = ImpactRequest {
        top_k: Some(1),
        ..request(&["src/auth/token.ts"], None)
    };
    let response = impact(&database, &repository, &top_one, &config).unwrap();
    assert_eq!(paths(&response), ["src/auth/session.ts"]);
    assert_eq!(response.candidate_count, 3);

    let shallow = ImpactRequest {
        max_depth: Some(1),
        ..request(&["src/auth/token.ts"], None)
    };
    let response = impact(&database, &repository, &shallow, &config).unwrap();
    assert_eq!(paths(&response), ["src/auth/session.ts"]);
}

#[test]
fn custom_weights_change_the_order() {
    let dir = tempfile::tempdir().unwrap();
    generate_main_fixture(dir.path()).unwrap();
    let (database, repository, config) = setup(dir.path());
    let history_only = ImpactRequest {
        static_weight: Some(0.0),
        historical_weight: Some(1.0),
        ..request(&["src/billing/invoice.ts"], None)
    };
    let response = impact(&database, &repository, &history_only, &config).unwrap();
    assert_eq!(
        response.recommendations[0].path.as_str(),
        "src/auth/session.ts"
    );
}

#[test]
fn a_shallow_clone_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = FixtureRepo::init(dir.path()).unwrap();
    repo.change()
        .unwrap()
        .write("a.ts", "x\n")
        .commit("one")
        .unwrap();
    let boundary = repo
        .change()
        .unwrap()
        .write("b.ts", "x\n")
        .commit("two")
        .unwrap();
    std::fs::write(dir.path().join(".git/shallow"), format!("{boundary}\n")).unwrap();
    let (database, repository, config) = setup(dir.path());
    let response = impact(&database, &repository, &request(&["b.ts"], None), &config).unwrap();
    assert!(response.history.is_shallow);
}
