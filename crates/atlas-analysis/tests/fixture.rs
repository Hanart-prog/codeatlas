//! Compares engine output on the generated fixture with hand-derived facts.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use atlas_analysis::{AnalyzeOptions, analyze, overview};
use atlas_fixtures::{FixtureRepo, exports, generate_main_fixture};
use atlas_model::{AtlasConfig, FileChurn, OverviewResponse};
use atlas_storage::Database;
use serde::Deserialize;

#[derive(Deserialize)]
struct Expected {
    commit_count: u32,
    merge_commit_count: u32,
    rename_count: u32,
    tip_file_count: u32,
    tip_typescript_file_count: u32,
    changed_file_count: u32,
    unreadable_path_count: u32,
    exclusions: BTreeMap<String, u32>,
    first_subject: String,
    last_subject: String,
    churn: Vec<FileChurn>,
}

fn expected() -> Expected {
    serde_json::from_str(include_str!("../../../fixtures/expected/main.json")).unwrap()
}

fn analyzed(path: &Path) -> (Database, OverviewResponse) {
    let mut database = Database::open_in_memory().unwrap();
    let outcome = analyze(
        &mut database,
        path,
        &AnalyzeOptions::default(),
        &AtlasConfig::default(),
    )
    .unwrap();
    let response = overview(&database, outcome.repository.id).unwrap();
    (database, response)
}

fn count(database: &Database, table: &str) -> i64 {
    database
        .connection()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}

#[test]
fn fixture_facts_match_hand_derived_expectations() {
    let dir = tempfile::tempdir().unwrap();
    let tip = generate_main_fixture(dir.path()).unwrap();
    let (_, overview) = analyzed(dir.path());
    let expected = expected();

    assert_eq!(overview.run.target_oid.as_str(), tip.to_string());
    assert_eq!(overview.history.commit_count, expected.commit_count);
    assert_eq!(
        overview.history.merge_commit_count,
        expected.merge_commit_count
    );
    assert!(!overview.history.is_shallow);
    assert!(!overview.history.is_truncated);
    assert_eq!(
        overview.history.first_commit.unwrap().subject,
        expected.first_subject
    );
    assert_eq!(
        overview.history.last_commit.unwrap().subject,
        expected.last_subject
    );
    assert_eq!(overview.files.tip_file_count, expected.tip_file_count);
    assert_eq!(
        overview.files.tip_typescript_file_count,
        expected.tip_typescript_file_count
    );
    assert_eq!(
        overview.files.changed_file_count,
        expected.changed_file_count
    );
    assert_eq!(overview.files.rename_count, expected.rename_count);
    assert_eq!(
        overview.files.unreadable_path_count,
        expected.unreadable_path_count
    );

    let exclusions: BTreeMap<_, _> = overview
        .exclusions
        .iter()
        .map(|e| (e.reason.as_str().to_owned(), e.count))
        .collect();
    assert_eq!(exclusions, expected.exclusions);
    assert_eq!(overview.top_churn, expected.churn);
}

#[test]
fn rerunning_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    generate_main_fixture(dir.path()).unwrap();
    let config = AtlasConfig::default();
    let options = AnalyzeOptions::default();
    let mut database = Database::open_in_memory().unwrap();

    let first = analyze(&mut database, dir.path(), &options, &config).unwrap();
    assert!(!first.cached);
    let before = overview(&database, first.repository.id).unwrap();
    let tables = [
        "commits",
        "commit_parents",
        "run_commits",
        "file_identities",
        "file_paths",
        "file_changes",
        "analysis_runs",
    ];
    let counts: Vec<_> = tables.iter().map(|t| count(&database, t)).collect();

    let second = analyze(&mut database, dir.path(), &options, &config).unwrap();
    assert!(second.cached);
    assert_eq!(second.run_id, first.run_id);
    assert_eq!(overview(&database, first.repository.id).unwrap(), before);
    assert_eq!(
        tables
            .iter()
            .map(|t| count(&database, t))
            .collect::<Vec<_>>(),
        counts
    );
}

#[test]
fn a_fresh_database_produces_the_same_overview() {
    let dir = tempfile::tempdir().unwrap();
    generate_main_fixture(dir.path()).unwrap();
    let (_, mut one) = analyzed(dir.path());
    let (_, mut two) = analyzed(dir.path());
    one.run.completed_at = None;
    two.run.completed_at = None;
    one.repository.analyzed_at = None;
    two.repository.analyzed_at = None;
    assert_eq!(one, two);
}

#[test]
fn changed_settings_or_limits_create_a_new_run_and_share_commits() {
    let dir = tempfile::tempdir().unwrap();
    generate_main_fixture(dir.path()).unwrap();
    let mut database = Database::open_in_memory().unwrap();
    let options = AnalyzeOptions::default();
    let config = AtlasConfig::default();
    analyze(&mut database, dir.path(), &options, &config).unwrap();

    let mut changed = config.clone();
    changed.analysis.rename_similarity_percent = 90;
    assert!(
        !analyze(&mut database, dir.path(), &options, &changed)
            .unwrap()
            .cached
    );

    let limited = AnalyzeOptions {
        history_limit: 4,
        ..options
    };
    let outcome = analyze(&mut database, dir.path(), &limited, &config).unwrap();
    assert!(!outcome.cached);
    assert_eq!(count(&database, "analysis_runs"), 3);
    assert_eq!(count(&database, "commits"), 13);

    let summary = overview(&database, outcome.repository.id).unwrap();
    assert_eq!(summary.history.commit_count, 4);
    assert!(summary.history.is_truncated);
}

#[test]
fn new_commits_produce_a_new_run_at_the_new_tip() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = FixtureRepo::init(dir.path()).unwrap();
    repo.change()
        .unwrap()
        .write("a.ts", exports("a", 1))
        .commit("one")
        .unwrap();
    let (mut database, _) = analyzed(dir.path());

    repo.change()
        .unwrap()
        .write("b.ts", exports("b", 1))
        .commit("two")
        .unwrap();
    let outcome = analyze(
        &mut database,
        dir.path(),
        &AnalyzeOptions::default(),
        &AtlasConfig::default(),
    )
    .unwrap();
    assert!(!outcome.cached);
    assert_eq!(
        overview(&database, outcome.repository.id)
            .unwrap()
            .history
            .commit_count,
        2
    );
}

#[test]
fn out_of_order_timestamps_do_not_reorder_history() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = FixtureRepo::init(dir.path()).unwrap();
    repo.set_clock(2_000_000_000);
    repo.change()
        .unwrap()
        .write("a.ts", "a\n")
        .commit("newer clock")
        .unwrap();
    repo.set_clock(1_000_000_000);
    repo.change()
        .unwrap()
        .write("b.ts", "b\n")
        .commit("older clock")
        .unwrap();

    let (_, overview) = analyzed(dir.path());
    assert_eq!(
        overview.history.first_commit.unwrap().subject,
        "newer clock"
    );
    assert_eq!(overview.history.last_commit.unwrap().subject, "older clock");
}

#[test]
fn a_shallow_clone_is_flagged() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = FixtureRepo::init(dir.path()).unwrap();
    repo.change()
        .unwrap()
        .write("a.ts", "a\n")
        .commit("one")
        .unwrap();
    let boundary = repo
        .change()
        .unwrap()
        .write("b.ts", "b\n")
        .commit("two")
        .unwrap();
    fs::write(dir.path().join(".git/shallow"), format!("{boundary}\n")).unwrap();

    let (_, overview) = analyzed(dir.path());
    assert!(overview.history.is_shallow);
    assert_eq!(overview.history.commit_count, 1);
}

#[test]
fn an_empty_repository_is_a_clear_error_and_stores_no_run() {
    let dir = tempfile::tempdir().unwrap();
    FixtureRepo::init(dir.path()).unwrap();
    let mut database = Database::open_in_memory().unwrap();
    let error = analyze(
        &mut database,
        dir.path(),
        &AnalyzeOptions::default(),
        &AtlasConfig::default(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("no commits"), "{error}");
    assert_eq!(count(&database, "analysis_runs"), 0);
}

#[test]
fn a_failure_records_a_failed_run_with_no_partial_history() {
    let dir = tempfile::tempdir().unwrap();
    generate_main_fixture(dir.path()).unwrap();
    let mut config = AtlasConfig::default();
    config.analysis.exclude_globs = vec!["a[".to_owned()];
    let mut database = Database::open_in_memory().unwrap();

    assert!(
        analyze(
            &mut database,
            dir.path(),
            &AnalyzeOptions::default(),
            &config
        )
        .is_err()
    );

    let status: String = database
        .connection()
        .query_row("SELECT status FROM analysis_runs", [], |row| row.get(0))
        .unwrap();
    assert_eq!(status, "failed");
    assert_eq!(count(&database, "file_changes"), 0);
    assert_eq!(count(&database, "commits"), 0);
}

#[test]
fn timeline_pages_cover_every_commit_exactly_once() {
    let dir = tempfile::tempdir().unwrap();
    generate_main_fixture(dir.path()).unwrap();
    let mut database = Database::open_in_memory().unwrap();
    let outcome = analyze(
        &mut database,
        dir.path(),
        &AnalyzeOptions::default(),
        &AtlasConfig::default(),
    )
    .unwrap();

    let mut positions = Vec::new();
    let mut cursor = None;
    loop {
        let page = database.commit_page(outcome.run_id, cursor, 5).unwrap();
        positions.extend(page.commits.iter().map(|c| c.position));
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(positions, (0..13).rev().collect::<Vec<u32>>());
}

/// Every file with its bytes and modification time.
fn snapshot(root: &Path) -> BTreeMap<PathBuf, (Vec<u8>, SystemTime)> {
    let mut files = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            let metadata = fs::symlink_metadata(entry.path()).unwrap();
            if metadata.is_dir() {
                pending.push(entry.path());
            } else {
                files.insert(
                    entry.path(),
                    (
                        fs::read(entry.path()).unwrap(),
                        metadata.modified().unwrap(),
                    ),
                );
            }
        }
    }
    files
}

#[test]
fn analysis_leaves_the_repository_byte_for_byte_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    generate_main_fixture(dir.path()).unwrap();
    let before = snapshot(dir.path());
    analyzed(dir.path());
    assert_eq!(snapshot(dir.path()), before);
}

#[cfg(unix)]
#[test]
fn repository_hooks_and_content_are_never_executed() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("executed");
    let script = format!("#!/bin/sh\ntouch '{}'\n", marker.display());

    let mut repo = FixtureRepo::init(dir.path()).unwrap();
    repo.change()
        .unwrap()
        .write("install.sh", script.as_bytes())
        .write(
            "package.json",
            r#"{"scripts":{"postinstall":"./install.sh"}}"#,
        )
        .commit("one")
        .unwrap();

    for hook in [
        "post-checkout",
        "post-merge",
        "pre-commit",
        "post-commit",
        "reference-transaction",
    ] {
        let path = dir.path().join(".git/hooks").join(hook);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, &script).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    analyzed(dir.path());
    assert!(!marker.exists());
}

#[test]
fn a_file_first_seen_by_a_rename_can_be_stored() {
    let repo = tempfile::tempdir().unwrap();
    let mut fixture = FixtureRepo::init(repo.path()).unwrap();
    fixture
        .change()
        .unwrap()
        .write("a.ts", exports("a", 6))
        .commit("Older than the window")
        .unwrap();
    fixture
        .change()
        .unwrap()
        .rename("a.ts", "b.ts")
        .commit("Rename inside the window")
        .unwrap();

    let mut database = Database::open_in_memory().unwrap();
    let options = AnalyzeOptions {
        history_limit: 1,
        ..AnalyzeOptions::default()
    };
    let outcome = analyze(
        &mut database,
        repo.path(),
        &options,
        &AtlasConfig::default(),
    )
    .unwrap();
    let response = overview(&database, outcome.repository.id).unwrap();
    assert_eq!(response.history.commit_count, 1);
}
