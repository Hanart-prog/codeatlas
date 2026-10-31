//! Compares architecture diffs on generated repositories with hand-derived facts.

use std::path::Path;

use atlas_analysis::{AnalyzeOptions, analyze, compare, timeline};
use atlas_fixtures::{FixtureRepo, exports, generate_main_fixture};
use atlas_model::{
    AtlasConfig, AtlasError, CommitRelation, CompareRequest, CompareResponse, ImportChange,
    RenameEvidence, RepositoryEntry,
};
use atlas_storage::Database;
use serde::Deserialize;

#[derive(Deserialize)]
struct ExpectedFile {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    base: String,
    target: String,
    relation: CommitRelation,
    files: [u32; 2],
    imports: [u32; 2],
    cycles: [u32; 2],
    added_files: Vec<String>,
    removed_files: Vec<String>,
    renamed_files: Vec<(String, String)>,
    added_imports: Vec<(String, String)>,
    removed_imports: Vec<(String, String)>,
    new_cycles: Vec<Vec<String>>,
    resolved_cycles: Vec<Vec<String>>,
    metric_changes: Vec<Metric>,
}

#[derive(Deserialize)]
struct Metric {
    path: String,
    #[serde(rename = "in")]
    in_degree: [u32; 2],
    out: [u32; 2],
    dependents: [u32; 2],
}

fn setup(path: &Path) -> (Database, RepositoryEntry, AtlasConfig) {
    let config = AtlasConfig::default();
    let mut database = Database::open_in_memory().unwrap();
    let outcome = analyze(&mut database, path, &AnalyzeOptions::default(), &config).unwrap();
    (database, outcome.repository, config)
}

fn oid_of(database: &Database, repository: &RepositoryEntry, subject: &str) -> String {
    let run = database.latest_run(repository.id).unwrap().unwrap();
    database
        .commit_page(run.id, None, 1000)
        .unwrap()
        .commits
        .into_iter()
        .find(|commit| commit.subject == subject)
        .unwrap_or_else(|| panic!("no commit named {subject:?}"))
        .oid
        .to_string()
}

fn request(base: impl Into<String>, target: impl Into<String>) -> CompareRequest {
    CompareRequest {
        base: base.into(),
        target: target.into(),
        limit: None,
    }
}

fn pairs(changes: &[ImportChange]) -> Vec<(String, String)> {
    changes
        .iter()
        .map(|c| (c.edge.source.to_string(), c.edge.target.to_string()))
        .collect()
}

fn members(cycles: &[atlas_model::Cycle]) -> Vec<Vec<String>> {
    cycles
        .iter()
        .map(|c| c.members.iter().map(ToString::to_string).collect())
        .collect()
}

fn check(response: &CompareResponse, case: &Case) {
    let name = &case.name;
    assert_eq!(response.relation, case.relation, "{name}");
    assert_eq!(
        [response.base.file_count, response.target.file_count],
        case.files,
        "{name}"
    );
    assert_eq!(
        [response.base.import_count, response.target.import_count],
        case.imports,
        "{name}"
    );
    assert_eq!(
        [response.base.cycle_count, response.target.cycle_count],
        case.cycles,
        "{name}"
    );

    let paths = |nodes: &[atlas_model::GraphNode]| -> Vec<String> {
        nodes.iter().map(|n| n.path.to_string()).collect()
    };
    assert_eq!(paths(&response.added_files), case.added_files, "{name}");
    assert_eq!(paths(&response.removed_files), case.removed_files, "{name}");
    let renames: Vec<(String, String)> = response
        .renamed_files
        .iter()
        .map(|r| (r.from.to_string(), r.to.to_string()))
        .collect();
    assert_eq!(renames, case.renamed_files, "{name}");
    assert_eq!(pairs(&response.added_imports), case.added_imports, "{name}");
    assert_eq!(
        pairs(&response.removed_imports),
        case.removed_imports,
        "{name}"
    );
    assert_eq!(members(&response.new_cycles), case.new_cycles, "{name}");
    assert_eq!(
        members(&response.resolved_cycles),
        case.resolved_cycles,
        "{name}"
    );

    let actual: Vec<_> = response
        .metric_changes
        .iter()
        .map(|m| {
            (
                m.path.to_string(),
                [m.in_degree_before, m.in_degree_after],
                [m.out_degree_before, m.out_degree_after],
                [m.dependents_before, m.dependents_after],
            )
        })
        .collect();
    let expected: Vec<_> = case
        .metric_changes
        .iter()
        .map(|m| (m.path.clone(), m.in_degree, m.out, m.dependents))
        .collect();
    assert_eq!(actual, expected, "{name}");
}

#[test]
fn fixture_comparisons_match_hand_derived_facts() {
    let expected: ExpectedFile =
        serde_json::from_str(include_str!("../../../fixtures/expected/compare.json")).unwrap();
    let repo = tempfile::tempdir().unwrap();
    generate_main_fixture(repo.path()).unwrap();
    let (database, repository, config) = setup(repo.path());

    for case in &expected.cases {
        let wanted = request(
            oid_of(&database, &repository, &case.base),
            oid_of(&database, &repository, &case.target),
        );
        check(
            &compare(&database, &repository, &wanted, &config).unwrap(),
            case,
        );
    }
}

#[test]
fn a_comparison_is_reproducible_and_runs_backwards() {
    let repo = tempfile::tempdir().unwrap();
    generate_main_fixture(repo.path()).unwrap();
    let (database, repository, config) = setup(repo.path());
    let early = oid_of(&database, &repository, "Add billing");
    let late = oid_of(&database, &repository, "Introduce session and token cycle");

    let forward = compare(&database, &repository, &request(&early, &late), &config).unwrap();
    let again = compare(&database, &repository, &request(&early, &late), &config).unwrap();
    assert_eq!(forward, again);

    let backward = compare(&database, &repository, &request(&late, &early), &config).unwrap();
    assert_eq!(backward.relation, CommitRelation::Backward);
    assert!(backward.notes.iter().any(|note| note.contains("older")));
    assert_eq!(
        pairs(&backward.removed_imports),
        pairs(&forward.added_imports)
    );
    assert_eq!(backward.resolved_cycles, forward.new_cycles);
    assert_eq!(
        backward.renamed_files[0].to.as_str(),
        "src/users/profile.ts"
    );
}

#[test]
fn a_commit_compared_with_itself_is_empty() {
    let repo = tempfile::tempdir().unwrap();
    generate_main_fixture(repo.path()).unwrap();
    let (database, repository, config) = setup(repo.path());

    let response = compare(&database, &repository, &request("HEAD", "HEAD"), &config).unwrap();
    assert_eq!(response.relation, CommitRelation::Same);
    assert_eq!(response.counts, Default::default());
    assert_eq!(response.notes, ["Both sides are the same commit."]);
}

#[test]
fn a_renamed_and_edited_file_keeps_its_imports_and_cycle() {
    let repo = tempfile::tempdir().unwrap();
    let mut fixture = FixtureRepo::init(repo.path()).unwrap();
    let a = format!("import {{ b0 }} from './b';\n{}", exports("a", 12));
    let b = format!("import {{ a0 }} from './a';\n{}", exports("b", 12));
    let base = fixture
        .change()
        .unwrap()
        .write("src/a.ts", a)
        .write("src/b.ts", b)
        .commit("Create")
        .unwrap();
    let target = fixture
        .change()
        .unwrap()
        .remove("src/a.ts")
        .write(
            "src/core/a.ts",
            format!("import {{ b0 }} from '../b';\n{}", exports("a", 12)),
        )
        .replace("src/b.ts", "'./a'", "'./core/a'")
        .commit("Move a")
        .unwrap();
    let (database, repository, config) = setup(repo.path());

    let wanted = request(base.to_string(), target.to_string());
    let response = compare(&database, &repository, &wanted, &config).unwrap();
    assert_eq!(response.counts.renamed_files, 1);
    assert_eq!(response.counts.added_imports, 0);
    assert_eq!(response.counts.removed_imports, 0);
    assert_eq!(response.counts.imports_following_renames, 2);
    assert_eq!(response.counts.unchanged_cycles, 1);
    assert_eq!(
        response.counts.new_cycles + response.counts.resolved_cycles,
        0
    );
}

#[test]
fn a_tsconfig_change_is_called_out() {
    let repo = tempfile::tempdir().unwrap();
    let mut fixture = FixtureRepo::init(repo.path()).unwrap();
    let base = fixture
        .change()
        .unwrap()
        .write("src/a.ts", "import { b } from '@/b';\n")
        .write("src/b.ts", "export const b = 1;\n")
        .commit("Unresolved alias")
        .unwrap();
    let target = fixture
        .change()
        .unwrap()
        .write(
            "tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"]}}}"#,
        )
        .commit("Define the alias")
        .unwrap();
    let (database, repository, config) = setup(repo.path());

    let wanted = request(base.to_string(), target.to_string());
    let response = compare(&database, &repository, &wanted, &config).unwrap();
    assert_eq!(
        pairs(&response.added_imports),
        [("src/a.ts".to_owned(), "src/b.ts".to_owned())],
        "no source file changed"
    );
    let notes = response.notes.join("\n");
    assert!(notes.contains("root tsconfig.json differs"), "{notes}");
    assert!(notes.contains("alias resolution"), "{notes}");
    assert!(
        notes.contains("Unresolved imports went from 1 to 0"),
        "{notes}"
    );
}

#[test]
fn imports_lost_to_a_syntax_error_are_uncertain() {
    let repo = tempfile::tempdir().unwrap();
    let mut fixture = FixtureRepo::init(repo.path()).unwrap();
    let base = fixture
        .change()
        .unwrap()
        .write(
            "src/a.ts",
            "import { b } from './b';\nexport const a = b;\n",
        )
        .write("src/b.ts", "export const b = 1;\n")
        .commit("Create")
        .unwrap();
    let target = fixture
        .change()
        .unwrap()
        .write("src/a.ts", "export const a = (((;\n")
        .commit("Break the syntax")
        .unwrap();
    let (database, repository, config) = setup(repo.path());

    let wanted = request(base.to_string(), target.to_string());
    let response = compare(&database, &repository, &wanted, &config).unwrap();
    assert_eq!(response.removed_imports.len(), 1);
    assert!(response.removed_imports[0].uncertain);
    assert_eq!(response.counts.uncertain_imports, 1);
    assert!(response.notes.join("\n").contains("marked uncertain"));
}

#[test]
fn commits_on_separate_lines_are_diverged() {
    let repo = tempfile::tempdir().unwrap();
    let mut fixture = FixtureRepo::init(repo.path()).unwrap();
    let root = fixture
        .change()
        .unwrap()
        .write("a.ts", "export const a = 1;\n")
        .commit("Root")
        .unwrap();
    let left = fixture
        .change()
        .unwrap()
        .write("left.ts", "export const l = 1;\n")
        .commit("Left")
        .unwrap();
    fixture.branch_from(root);
    let right = fixture
        .change()
        .unwrap()
        .detached()
        .write("right.ts", "export const r = 1;\n")
        .commit("Right")
        .unwrap();
    fixture.detach_head(left).unwrap();
    let (database, repository, config) = setup(repo.path());

    let wanted = request(left.to_string(), right.to_string());
    let response = compare(&database, &repository, &wanted, &config).unwrap();
    assert_eq!(response.relation, CommitRelation::Diverged);
    assert_eq!(response.added_files[0].path.as_str(), "right.ts");
    assert_eq!(response.removed_files[0].path.as_str(), "left.ts");
    assert!(response.notes[0].contains("two lines of work"));
}

#[test]
fn a_limit_cuts_lists_but_never_counts() {
    let repo = tempfile::tempdir().unwrap();
    let mut fixture = FixtureRepo::init(repo.path()).unwrap();
    let base = fixture
        .change()
        .unwrap()
        .write("a.ts", "export const a = 1;\n")
        .commit("Root")
        .unwrap();
    let target = fixture
        .change()
        .unwrap()
        .write("b.ts", "import './a';\n")
        .write("c.ts", "import './a';\n")
        .commit("Add")
        .unwrap();
    let (database, repository, config) = setup(repo.path());

    let mut wanted = request(base.to_string(), target.to_string());
    wanted.limit = Some(1);
    let response = compare(&database, &repository, &wanted, &config).unwrap();
    assert_eq!(response.added_files.len(), 1);
    assert_eq!(response.counts.added_files, 2);
    assert_eq!(response.added_imports.len(), 1);
    assert_eq!(response.counts.added_imports, 2);

    wanted.limit = Some(0);
    let error = compare(&database, &repository, &wanted, &config).unwrap_err();
    assert!(matches!(error, AtlasError::InvalidInput(_)), "{error}");
}

#[test]
fn an_unknown_commit_is_an_error() {
    let repo = tempfile::tempdir().unwrap();
    generate_main_fixture(repo.path()).unwrap();
    let (database, repository, config) = setup(repo.path());
    let error = compare(
        &database,
        &repository,
        &request("HEAD", "no-such-ref"),
        &config,
    );
    assert!(error.is_err());
}

/// Rewrites half of a file in each commit, so that the two ends share nothing.
fn drifted(fixture: &mut FixtureRepo) -> (String, String) {
    let base = fixture
        .change()
        .unwrap()
        .write("old.ts", exports("first", 12))
        .write("user.ts", "import { first0 } from './old';\n")
        .commit("Create")
        .unwrap();
    fixture
        .change()
        .unwrap()
        .remove("old.ts")
        .write(
            "mid.ts",
            format!("{}{}", exports("first", 6), exports("second", 6)),
        )
        .write("user.ts", "import { first0 } from './mid';\n")
        .commit("Move and rewrite half")
        .unwrap();
    let target = fixture
        .change()
        .unwrap()
        .remove("mid.ts")
        .write(
            "new.ts",
            format!("{}{}", exports("second", 6), exports("third", 6)),
        )
        .write("user.ts", "import { second0 } from './new';\n")
        .commit("Move and rewrite the rest")
        .unwrap();
    (base.to_string(), target.to_string())
}

#[test]
fn a_file_rewritten_step_by_step_is_followed_through_history() {
    let repo = tempfile::tempdir().unwrap();
    let mut fixture = FixtureRepo::init(repo.path()).unwrap();
    let (base, target) = drifted(&mut fixture);
    let (database, repository, config) = setup(repo.path());

    let wanted = request(&base, &target);
    let response = compare(&database, &repository, &wanted, &config).unwrap();
    assert_eq!(
        response.counts.added_files + response.counts.removed_files,
        0
    );
    assert_eq!(response.renamed_files[0].from.as_str(), "old.ts");
    assert_eq!(response.renamed_files[0].to.as_str(), "new.ts");
    assert_eq!(response.renamed_files[0].evidence, RenameEvidence::History);
    assert_eq!(
        response.counts.added_imports + response.counts.removed_imports,
        0
    );

    let wanted = request(&target, &base);
    let response = compare(&database, &repository, &wanted, &config).unwrap();
    assert_eq!(response.renamed_files[0].from.as_str(), "new.ts");
    assert_eq!(response.renamed_files[0].to.as_str(), "old.ts");
}

#[test]
fn a_new_file_is_not_the_deleted_file_that_had_its_path() {
    let repo = tempfile::tempdir().unwrap();
    let mut fixture = FixtureRepo::init(repo.path()).unwrap();
    let base = fixture
        .change()
        .unwrap()
        .write("a.ts", exports("old", 12))
        .commit("Create")
        .unwrap();
    fixture
        .change()
        .unwrap()
        .remove("a.ts")
        .commit("Delete")
        .unwrap();
    fixture
        .change()
        .unwrap()
        .write("a.ts", exports("unrelated", 12))
        .commit("Recreate")
        .unwrap();
    let target = fixture
        .change()
        .unwrap()
        .rename("a.ts", "b.ts")
        .commit("Rename the new file")
        .unwrap();
    let (database, repository, config) = setup(repo.path());

    let wanted = request(base.to_string(), target.to_string());
    let response = compare(&database, &repository, &wanted, &config).unwrap();
    assert!(
        response.renamed_files.is_empty(),
        "{:?}",
        response.renamed_files
    );
    assert_eq!(response.counts.added_files, 1);
    assert_eq!(response.counts.removed_files, 1);
}

#[test]
fn a_deleted_file_does_not_become_a_new_file_at_its_last_path() {
    let repo = tempfile::tempdir().unwrap();
    let mut fixture = FixtureRepo::init(repo.path()).unwrap();
    let base = fixture
        .change()
        .unwrap()
        .write("a.ts", exports("old", 12))
        .commit("Create")
        .unwrap();
    fixture
        .change()
        .unwrap()
        .rename("a.ts", "b.ts")
        .commit("Rename")
        .unwrap();
    fixture
        .change()
        .unwrap()
        .remove("b.ts")
        .commit("Delete")
        .unwrap();
    let target = fixture
        .change()
        .unwrap()
        .write("b.ts", exports("unrelated", 3))
        .commit("Recreate")
        .unwrap();
    let (database, repository, config) = setup(repo.path());

    let wanted = request(base.to_string(), target.to_string());
    let response = compare(&database, &repository, &wanted, &config).unwrap();
    assert!(
        response.renamed_files.is_empty(),
        "{:?}",
        response.renamed_files
    );
}

#[test]
fn the_timeline_reports_each_commit_and_finds_the_cycle() {
    let repo = tempfile::tempdir().unwrap();
    generate_main_fixture(repo.path()).unwrap();
    let (database, repository, config) = setup(repo.path());

    let response = timeline(&database, &repository, 40, &config).unwrap();
    assert_eq!(response.commit_count, 13);
    assert_eq!(response.points.len(), 13);

    // From the commit list in fixtures/generator/src/lib.rs.
    let at = |subject: &str| {
        let point = response
            .points
            .iter()
            .find(|point| point.commit.subject == subject)
            .unwrap();
        (point.file_count, point.import_count, point.cycle_count)
    };
    assert_eq!(at("Create auth, API and users modules"), (4, 2, 0));
    assert_eq!(at("Add billing"), (6, 4, 0));
    assert_eq!(at("Introduce session and token cycle"), (6, 6, 1));
    assert_eq!(at("Break session and token cycle"), (6, 4, 0));

    let sampled = timeline(&database, &repository, 3, &config).unwrap();
    let positions: Vec<u32> = sampled.points.iter().map(|point| point.position).collect();
    assert_eq!(positions, [0, 6, 12]);
    assert!(timeline(&database, &repository, 0, &config).is_err());
}
