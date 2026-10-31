//! Reads repositories built by the fixture crate.

use std::fs;
use std::path::Path;

use atlas_fixtures::{FixtureRepo, exports, generate_main_fixture};
use atlas_git::{ExclusionRules, GitError, GitRepository};
use atlas_model::history::{ChangeStatus, ExclusionReason, FileChange};
use atlas_model::{AnalysisConfig, Limits, Oid};

fn rules() -> ExclusionRules {
    ExclusionRules::from_config(&AnalysisConfig::default(), &Limits::default()).unwrap()
}

fn open(path: &Path) -> GitRepository {
    GitRepository::open(path).unwrap()
}

fn head(repo: &GitRepository) -> Oid {
    repo.resolve("HEAD").unwrap()
}

fn fixture(path: &Path) -> FixtureRepo {
    FixtureRepo::init(path).unwrap()
}

fn commit(fixture: &mut FixtureRepo, path: &str, content: &str) -> atlas_fixtures::Oid {
    fixture
        .change()
        .unwrap()
        .write(path, content)
        .commit(path)
        .unwrap()
}

fn changes_at_head(path: &Path) -> Vec<FileChange> {
    let repo = open(path);
    repo.commit_changes(&head(&repo), &rules()).unwrap().changes
}

fn path(change: &FileChange) -> &str {
    change
        .new_path
        .as_ref()
        .or(change.old_path.as_ref())
        .unwrap()
        .as_str()
}

#[test]
fn a_directory_that_is_not_a_repository_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        GitRepository::open(dir.path()),
        Err(GitError::NotARepository { .. })
    ));
}

#[test]
fn opening_does_not_search_parent_directories() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    let nested = dir.path().join("nested");
    fs::create_dir(&nested).unwrap();
    assert!(GitRepository::open(&nested).is_err());
}

#[test]
fn an_empty_repository_reports_that_it_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    assert!(matches!(
        open(dir.path()).resolve("HEAD"),
        Err(GitError::Empty)
    ));
}

#[test]
fn unknown_references_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    generate_main_fixture(dir.path()).unwrap();
    assert!(matches!(
        open(dir.path()).resolve("no-such-branch"),
        Err(GitError::UnknownRef { .. })
    ));
}

#[test]
fn detached_head_resolves() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = fixture(dir.path());
    let first = commit(&mut repo, "a.ts", "a\n");
    commit(&mut repo, "b.ts", "b\n");
    repo.detach_head(first).unwrap();
    assert_eq!(head(&open(dir.path())).as_str(), first.to_string());
}

#[test]
fn first_parent_walk_skips_merged_side_commits_and_runs_oldest_first() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = fixture(dir.path());
    let base = commit(&mut repo, "a.ts", "a\n");
    let side = repo
        .change()
        .unwrap()
        .detached()
        .write("s.ts", "s\n")
        .commit("side")
        .unwrap();
    repo.branch_from(base);
    let merge = repo
        .change()
        .unwrap()
        .write("s.ts", "s\n")
        .merge_of(side)
        .commit("merge")
        .unwrap();

    let git = open(dir.path());
    let history = git.first_parent_history(&head(&git), 100).unwrap();
    let oids: Vec<_> = history.oids.iter().map(ToString::to_string).collect();
    assert_eq!(oids, [base.to_string(), merge.to_string()]);
    assert!(!history.is_truncated);
}

#[test]
fn a_merge_is_one_change_set_against_its_first_parent() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = fixture(dir.path());
    let base = commit(&mut repo, "a.ts", "a\n");
    let side = repo
        .change()
        .unwrap()
        .detached()
        .write("s.ts", "s\n")
        .commit("side")
        .unwrap();
    repo.branch_from(base);
    repo.change()
        .unwrap()
        .write("s.ts", "s\n")
        .merge_of(side)
        .commit("merge")
        .unwrap();

    let git = open(dir.path());
    let changes = git.commit_changes(&head(&git), &rules()).unwrap();
    assert_eq!(changes.commit.parents.len(), 2);
    assert_eq!(changes.changes.len(), 1);
    assert_eq!(path(&changes.changes[0]), "s.ts");
}

#[test]
fn the_limit_truncates_to_the_newest_commits() {
    let dir = tempfile::tempdir().unwrap();
    generate_main_fixture(dir.path()).unwrap();
    let git = open(dir.path());
    let tip = head(&git);

    let limited = git.first_parent_history(&tip, 3).unwrap();
    assert_eq!(limited.oids.len(), 3);
    assert_eq!(limited.oids.last(), Some(&tip));
    assert!(limited.is_truncated);

    let exact = git.first_parent_history(&tip, 13).unwrap();
    assert_eq!(exact.oids.len(), 13);
    assert!(!exact.is_truncated);
}

#[test]
fn shallow_clones_are_detected_and_stop_at_the_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = fixture(dir.path());
    commit(&mut repo, "a.ts", "a\n");
    let boundary = commit(&mut repo, "b.ts", "b\n");
    let tip = commit(&mut repo, "c.ts", "c\n");
    fs::write(dir.path().join(".git/shallow"), format!("{boundary}\n")).unwrap();

    let git = open(dir.path());
    assert!(git.is_shallow());
    let history = git.first_parent_history(&head(&git), 100).unwrap();
    let oids: Vec<_> = history.oids.iter().map(ToString::to_string).collect();
    assert_eq!(oids, [boundary.to_string(), tip.to_string()]);
}

#[test]
fn renames_are_detected_with_their_line_edits() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = fixture(dir.path());
    commit(&mut repo, "old.ts", &exports("a", 20));
    repo.change()
        .unwrap()
        .remove("old.ts")
        .write("new.ts", exports("a", 20) + &exports("b", 2))
        .commit("two")
        .unwrap();

    let changes = changes_at_head(dir.path());
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].status, ChangeStatus::Renamed);
    assert_eq!(changes[0].old_path.as_ref().unwrap().as_str(), "old.ts");
    assert_eq!(changes[0].new_path.as_ref().unwrap().as_str(), "new.ts");
    assert_eq!((changes[0].added_lines, changes[0].deleted_lines), (2, 0));
}

#[test]
fn repository_config_cannot_switch_off_rename_detection() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = fixture(dir.path());
    commit(&mut repo, "old.ts", &exports("a", 20));
    repo.change()
        .unwrap()
        .rename("old.ts", "new.ts")
        .commit("two")
        .unwrap();

    let config = dir.path().join(".git/config");
    let mut text = fs::read_to_string(&config).unwrap();
    text.push_str("[diff]\n\trenames = false\n");
    fs::write(&config, text).unwrap();

    assert_eq!(changes_at_head(dir.path())[0].status, ChangeStatus::Renamed);
}

#[test]
fn unrelated_replacement_is_a_delete_and_an_add() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = fixture(dir.path());
    commit(&mut repo, "old.ts", &exports("a", 10));
    repo.change()
        .unwrap()
        .remove("old.ts")
        .write("new.ts", "completely different\ncontent here\n")
        .commit("two")
        .unwrap();

    let statuses: Vec<_> = changes_at_head(dir.path())
        .iter()
        .map(|c| c.status)
        .collect();
    assert!(statuses.contains(&ChangeStatus::Added));
    assert!(statuses.contains(&ChangeStatus::Deleted));
}

#[test]
fn exclusions_are_classified_rather_than_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = fixture(dir.path());
    let pointer = commit(&mut repo, "a.ts", "a\n");
    repo.change()
        .unwrap()
        .write("image.bin", b"\0\x01\x02binary")
        .symlink("link", "a.ts")
        .submodule("sub", pointer)
        .write("dist/out.js", "x\n")
        .commit("two")
        .unwrap();

    let changes = changes_at_head(dir.path());
    let reason = |name: &str| changes.iter().find(|c| path(c) == name).unwrap().exclusion;
    assert_eq!(reason("image.bin"), Some(ExclusionReason::Binary));
    assert_eq!(reason("link"), Some(ExclusionReason::Symlink));
    assert_eq!(reason("sub"), Some(ExclusionReason::Submodule));
    assert_eq!(reason("dist/out.js"), Some(ExclusionReason::Rule));
}

#[test]
fn oversized_blobs_are_excluded_without_counting_lines() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = fixture(dir.path());
    commit(&mut repo, "big.ts", &exports("a", 100));

    let limits = Limits {
        max_file_bytes: 100,
        ..Limits::default()
    };
    let tight = ExclusionRules::from_config(&AnalysisConfig::default(), &limits).unwrap();
    let git = open(dir.path());
    let changes = git.commit_changes(&head(&git), &tight).unwrap().changes;
    assert_eq!(changes[0].exclusion, Some(ExclusionReason::Oversized));
    assert_eq!(changes[0].added_lines, 0);
}

#[test]
fn non_utf8_paths_are_counted_and_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = fixture(dir.path());
    repo.change()
        .unwrap()
        .write("ok.ts", "a\n")
        .write(b"caf\xe9.ts".as_slice(), "b\n")
        .commit("one")
        .unwrap();

    let git = open(dir.path());
    let changes = git.commit_changes(&head(&git), &rules()).unwrap();
    assert_eq!(changes.changes.len(), 1);
    assert_eq!(changes.unreadable_paths, 1);
}

#[test]
fn spaces_and_unicode_in_paths_survive() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = fixture(dir.path());
    commit(&mut repo, "a b/ñandú.ts", "x\n");
    assert_eq!(path(&changes_at_head(dir.path())[0]), "a b/ñandú.ts");
}

#[test]
fn ignored_paths_in_a_commit_are_still_analyzed() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = fixture(dir.path());
    repo.change()
        .unwrap()
        .write(".gitignore", "*.ts\n")
        .write("tracked.ts", "x\n")
        .commit("one")
        .unwrap();
    let paths: Vec<_> = changes_at_head(dir.path())
        .iter()
        .map(|c| path(c).to_owned())
        .collect();
    assert_eq!(paths, [".gitignore", "tracked.ts"]);
}

#[test]
fn tree_summary_counts_regular_and_typescript_files() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = fixture(dir.path());
    repo.change()
        .unwrap()
        .write("src/a.ts", "x\n")
        .write("src/b.tsx", "x\n")
        .write("src/c.d.ts", "x\n")
        .write("README.md", "x\n")
        .symlink("link.ts", "src/a.ts")
        .commit("one")
        .unwrap();

    let git = open(dir.path());
    let summary = git.tree_summary(&head(&git), &rules()).unwrap();
    assert_eq!(summary.file_count, 4);
    assert_eq!(summary.typescript_file_count, 2);
}

#[test]
fn the_fixture_is_deterministic() {
    let (one, two) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    assert_eq!(
        generate_main_fixture(one.path()).unwrap(),
        generate_main_fixture(two.path()).unwrap()
    );
}
