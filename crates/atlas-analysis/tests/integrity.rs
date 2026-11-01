//! Repositories are untrusted, and results must depend only on their commits.

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use atlas_analysis::{AnalyzeOptions, GraphQuery, analyze, file_detail, graph, overview};
use atlas_fixtures::{FixtureRepo, exports};
use atlas_model::{AtlasConfig, AtlasError};
use atlas_storage::Database;

fn options(git_ref: &str) -> AnalyzeOptions {
    AnalyzeOptions {
        git_ref: git_ref.to_owned(),
        ..AnalyzeOptions::default()
    }
}

fn two_commits(path: &Path) -> (FixtureRepo, String, String) {
    let mut repo = FixtureRepo::init(path).unwrap();
    let first = repo
        .change()
        .unwrap()
        .write("a.ts", exports("a", 3))
        .commit("First")
        .unwrap();
    let second = repo
        .change()
        .unwrap()
        .append("a.ts", &exports("b", 3))
        .commit("Second")
        .unwrap();
    (repo, first.to_string(), second.to_string())
}

#[test]
fn a_repository_cannot_claim_to_be_another_one() {
    let (victim, evil) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    two_commits(victim.path());
    let (repo, ..) = two_commits(evil.path());
    let target = victim.path().canonicalize().unwrap();
    repo.repository()
        .config()
        .unwrap()
        .set_str("core.worktree", target.to_str().unwrap())
        .unwrap();

    let config = AtlasConfig::default();
    let mut database = Database::open_in_memory().unwrap();
    let honest = analyze(
        &mut database,
        victim.path(),
        &AnalyzeOptions::default(),
        &config,
    )
    .unwrap()
    .repository;
    let hostile = analyze(
        &mut database,
        evil.path(),
        &AnalyzeOptions::default(),
        &config,
    )
    .unwrap()
    .repository;
    assert_ne!(honest.id, hostile.id);
}

#[test]
fn uncommitted_files_do_not_change_results() {
    let dir = tempfile::tempdir().unwrap();
    two_commits(dir.path());
    let config = AtlasConfig::default();

    let run = || {
        let mut database = Database::open_in_memory().unwrap();
        let id = analyze(
            &mut database,
            dir.path(),
            &AnalyzeOptions::default(),
            &config,
        )
        .unwrap()
        .repository
        .id;
        let view = overview(&database, id).unwrap();
        (view.top_churn, view.exclusions)
    };
    let clean = run();
    fs::write(dir.path().join(".gitattributes"), "*.ts binary\n").unwrap();
    assert_eq!(clean, run());
}

#[test]
fn a_tree_bomb_is_refused_quickly() {
    let dir = tempfile::tempdir().unwrap();
    let repo = FixtureRepo::init(dir.path()).unwrap();
    let git = repo.repository();

    // Ten levels of ten references to the same subtree: 10^10 paths in 11 trees.
    let blob = git.blob(b"x\n").unwrap();
    let mut tree = {
        let mut builder = git.treebuilder(None).unwrap();
        builder.insert("leaf.md", blob, 0o100644).unwrap();
        builder.write().unwrap()
    };
    for _ in 0..10 {
        let mut builder = git.treebuilder(None).unwrap();
        for name in 0..10 {
            builder.insert(format!("d{name}"), tree, 0o040000).unwrap();
        }
        tree = builder.write().unwrap();
    }
    let signature = git2::Signature::now("a", "a@example.invalid").unwrap();
    let tree = git.find_tree(tree).unwrap();
    git.commit(Some("HEAD"), &signature, &signature, "Bomb", &tree, &[])
        .unwrap();

    let started = Instant::now();
    let mut database = Database::open_in_memory().unwrap();
    let result = analyze(
        &mut database,
        dir.path(),
        &AnalyzeOptions::default(),
        &AtlasConfig::default(),
    );
    assert!(
        matches!(result, Err(AtlasError::TooLarge(_))),
        "{:?}",
        result.err()
    );
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn analyzing_an_older_commit_again_makes_it_current() {
    let dir = tempfile::tempdir().unwrap();
    let (_repo, first, second) = two_commits(dir.path());
    let config = AtlasConfig::default();
    let mut database = Database::open_in_memory().unwrap();

    let id = analyze(&mut database, dir.path(), &options(&first), &config)
        .unwrap()
        .repository
        .id;
    analyze(&mut database, dir.path(), &options(&second), &config).unwrap();
    let again = analyze(&mut database, dir.path(), &options(&first), &config).unwrap();

    assert!(again.cached);
    let view = overview(&database, id).unwrap();
    assert_eq!(view.run.target_oid.as_str(), first);
    assert_eq!(view.history.commit_count, 1);

    let current = graph(
        &database,
        &again.repository,
        &GraphQuery {
            git_ref: None,
            directory: None,
            search: None,
            limit: 10,
            include_cochange: false,
        },
        &config,
    )
    .unwrap();
    assert_eq!(current.commit.as_str(), first);
}

#[test]
fn deepening_a_shallow_clone_analyzes_again() {
    let dir = tempfile::tempdir().unwrap();
    let (_repo, first, _) = two_commits(dir.path());
    let config = AtlasConfig::default();
    let mut database = Database::open_in_memory().unwrap();

    let shallow = dir.path().join(".git/shallow");
    fs::write(&shallow, format!("{first}\n")).unwrap();
    analyze(
        &mut database,
        dir.path(),
        &AnalyzeOptions::default(),
        &config,
    )
    .unwrap();

    fs::remove_file(&shallow).unwrap();
    let outcome = analyze(
        &mut database,
        dir.path(),
        &AnalyzeOptions::default(),
        &config,
    )
    .unwrap();
    assert!(!outcome.cached);
    let view = overview(&database, outcome.repository.id).unwrap();
    assert!(!view.history.is_shallow);
}

#[test]
fn a_repository_over_a_limit_keeps_its_history() {
    let dir = tempfile::tempdir().unwrap();
    two_commits(dir.path());
    let mut config = AtlasConfig::default();
    config.limits.max_graph_nodes = 0;
    let mut database = Database::open_in_memory().unwrap();

    let outcome = analyze(
        &mut database,
        dir.path(),
        &AnalyzeOptions::default(),
        &config,
    )
    .unwrap();
    assert!(outcome.graph_skipped.unwrap().contains("max_graph_nodes"));
    assert_eq!(
        overview(&database, outcome.repository.id)
            .unwrap()
            .history
            .commit_count,
        2
    );
}

#[test]
fn file_activity_is_not_shown_for_other_commits() {
    let dir = tempfile::tempdir().unwrap();
    let (_repo, first, second) = two_commits(dir.path());
    let config = AtlasConfig::default();
    let mut database = Database::open_in_memory().unwrap();
    let repository = analyze(
        &mut database,
        dir.path(),
        &AnalyzeOptions::default(),
        &config,
    )
    .unwrap()
    .repository;

    let path = "a.ts".parse().unwrap();
    let old = file_detail(&database, &repository, Some(first), &path, &config).unwrap();
    assert!(old.churn.is_none());
    let current = file_detail(&database, &repository, Some(second), &path, &config).unwrap();
    assert_eq!(current.churn.unwrap().commit_count, 2);
}
