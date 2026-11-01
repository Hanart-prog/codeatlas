//! A cached result must be the result a fresh analysis would give, and stopping
//! must leave nothing half-written.

use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use atlas_analysis::{
    AnalyzeOptions, GraphQuery, analyze, backtest, compare, graph, impact, overview, report,
    timeline,
};
use atlas_fixtures::{FixtureRepo, exports, generate_demo_fixture};
use atlas_model::{
    AnalysisStatus, AtlasConfig, AtlasError, BacktestRequest, CompareRequest, ImpactRequest,
    JobProgress, ProgressUnit, Work,
};
use atlas_storage::Database;

fn at(git_ref: &str) -> AnalyzeOptions {
    AnalyzeOptions {
        git_ref: git_ref.to_owned(),
        ..AnalyzeOptions::default()
    }
}

/// `commits` commits on one file each, plus a rename and a delete near the end.
fn history(path: &Path, commits: u32) -> FixtureRepo {
    let mut repo = FixtureRepo::init(path).unwrap();
    repo.change()
        .unwrap()
        .write("src/a.ts", exports("a", 8))
        .write("src/b.ts", "import { a0 } from './a';\n")
        .write("src/gone.ts", exports("gone", 2))
        .commit("Start")
        .unwrap();
    for number in 0..commits {
        repo.change()
            .unwrap()
            .append("src/a.ts", &format!("export const x{number} = 1;\n"))
            .append("src/b.ts", &format!("export const y{number} = 2;\n"))
            .commit(&format!("Change {number}"))
            .unwrap();
    }
    repo.change()
        .unwrap()
        .rename("src/a.ts", "src/core.ts")
        .replace("src/b.ts", "./a", "./core")
        .commit("Rename a")
        .unwrap();
    repo.change()
        .unwrap()
        .remove("src/gone.ts")
        .commit("Delete gone")
        .unwrap();
    repo
}

/// Everything derived from an analysis, as JSON without its run id and time.
fn results(database: &Database, id: atlas_model::RepositoryId, head: &str) -> serde_json::Value {
    let config = AtlasConfig::default();
    let mut overview = overview(database, id).unwrap();
    let repository = overview.repository.clone();
    overview.run.id = atlas_model::AnalysisRunId(0);
    overview.run.completed_at = None;
    overview.repository.analyzed_at = None;
    overview.repository.id = atlas_model::RepositoryId(0);
    let query = GraphQuery {
        git_ref: None,
        directory: None,
        search: None,
        limit: 1000,
        include_cochange: true,
    };
    let ranked = impact(
        database,
        &repository,
        &ImpactRequest {
            selected_paths: vec!["src/core.ts".to_owned()],
            ..ImpactRequest::default()
        },
        &config,
    )
    .unwrap();
    let diff = compare(
        database,
        &repository,
        &CompareRequest {
            base: format!("{head}~4"),
            target: head.to_owned(),
            limit: None,
        },
        &config,
    )
    .unwrap();
    serde_json::json!({
        "overview": overview,
        "graph": graph(database, &repository, &query, &config).unwrap(),
        "impact": ranked,
        "compare": diff,
        "timeline": timeline(database, &repository, 10, &config).unwrap(),
    })
}

#[test]
fn a_new_tip_reads_only_its_new_commits_and_matches_a_fresh_analysis() {
    let directory = tempfile::tempdir().unwrap();
    let repo = history(directory.path(), 20);
    let head = repo.head().unwrap().to_string();
    let config = AtlasConfig::default();

    let mut incremental = Database::open_in_memory().unwrap();
    let older = analyze(&mut incremental, directory.path(), &at("HEAD~3"), &config).unwrap();
    assert_eq!(older.commits_read, 20);
    let newer = analyze(&mut incremental, directory.path(), &at("HEAD"), &config).unwrap();
    assert_eq!(newer.commits_read, 3, "only the three new commits");
    assert!(!newer.cached);

    let mut fresh = Database::open_in_memory().unwrap();
    let full = analyze(&mut fresh, directory.path(), &at("HEAD"), &config).unwrap();
    assert_eq!(full.commits_read, 23);

    assert_eq!(
        results(&incremental, newer.repository.id, &head),
        results(&fresh, full.repository.id, &head)
    );
}

#[test]
fn a_moved_history_window_still_matches_a_fresh_analysis() {
    let directory = tempfile::tempdir().unwrap();
    let repo = history(directory.path(), 20);
    let head = repo.head().unwrap().to_string();
    let config = AtlasConfig::default();
    let window = |git_ref: &str| AnalyzeOptions {
        git_ref: git_ref.to_owned(),
        history_limit: 10,
    };

    // The oldest commit changes, so identities must be assigned from scratch.
    let mut incremental = Database::open_in_memory().unwrap();
    analyze(
        &mut incremental,
        directory.path(),
        &window("HEAD~5"),
        &config,
    )
    .unwrap();
    let newer = analyze(&mut incremental, directory.path(), &window("HEAD"), &config).unwrap();
    assert_eq!(newer.commits_read, 5);

    let mut fresh = Database::open_in_memory().unwrap();
    let full = analyze(&mut fresh, directory.path(), &window("HEAD"), &config).unwrap();
    assert_eq!(
        results(&incremental, newer.repository.id, &head),
        results(&fresh, full.repository.id, &head)
    );
}

#[test]
fn backtests_match_with_and_without_cached_diffs() {
    let directory = tempfile::tempdir().unwrap();
    generate_demo_fixture(directory.path()).unwrap();
    let config = AtlasConfig::default();
    let request = BacktestRequest {
        warmup: Some(60),
        max_cases: Some(30),
        ..BacktestRequest::default()
    };

    let mut incremental = Database::open_in_memory().unwrap();
    analyze(&mut incremental, directory.path(), &at("HEAD~1"), &config).unwrap();
    let outcome = analyze(&mut incremental, directory.path(), &at("HEAD"), &config).unwrap();
    let cached = backtest(
        &incremental,
        &outcome.repository,
        &request,
        &config,
        |_, _| {},
    )
    .unwrap();

    let mut fresh = Database::open_in_memory().unwrap();
    let outcome = analyze(&mut fresh, directory.path(), &at("HEAD"), &config).unwrap();
    let full = backtest(&fresh, &outcome.repository, &request, &config, |_, _| {}).unwrap();

    assert!(full.result.counts.evaluated_cases >= 10);
    assert_eq!(cached.result, full.result);
}

#[test]
fn a_changed_setting_does_not_reuse_diffs() {
    let directory = tempfile::tempdir().unwrap();
    history(directory.path(), 5);
    let config = AtlasConfig::default();
    let mut database = Database::open_in_memory().unwrap();
    analyze(&mut database, directory.path(), &at("HEAD"), &config).unwrap();

    let mut stricter = config.clone();
    stricter
        .analysis
        .exclude_globs
        .push("src/gone.ts".to_owned());
    let outcome = analyze(&mut database, directory.path(), &at("HEAD"), &stricter).unwrap();
    assert_eq!(outcome.commits_read, 8);
    let exclusions = overview(&database, outcome.repository.id)
        .unwrap()
        .exclusions;
    assert!(!exclusions.is_empty(), "the new rule took effect");
}

#[test]
fn cancelling_keeps_no_history_and_the_next_run_resumes() {
    let directory = tempfile::tempdir().unwrap();
    history(directory.path(), 250);
    let config = AtlasConfig::default();
    let mut database = Database::open_in_memory().unwrap();

    let work = Work::default();
    let stopper = work.clone();
    let seen = Arc::new(AtomicU32::new(0));
    let counter = Arc::clone(&seen);
    database.set_work(Work::clone(&work).reporting(move |progress| {
        counter.store(progress.done, Ordering::Relaxed);
        if progress.done == 150 {
            stopper.cancel();
        }
    }));
    let error = analyze(&mut database, directory.path(), &at("HEAD"), &config).unwrap_err();
    assert!(matches!(error, AtlasError::Cancelled), "{error}");
    assert_eq!(
        seen.load(Ordering::Relaxed),
        150,
        "stopped at the next step"
    );

    let status: String = database
        .connection()
        .query_row("SELECT status FROM analysis_runs", [], |row| row.get(0))
        .unwrap();
    assert_eq!(status, AnalysisStatus::Cancelled.as_str());
    let rows: u32 = database
        .connection()
        .query_row("SELECT count(*) FROM run_commits", [], |row| row.get(0))
        .unwrap();
    assert_eq!(rows, 0);

    database.set_work(Work::default());
    let outcome = analyze(&mut database, directory.path(), &at("HEAD"), &config).unwrap();
    assert_eq!(
        outcome.commits_read,
        253 - 150,
        "the 150 finished commits are kept"
    );
    let summary = overview(&database, outcome.repository.id).unwrap();
    assert_eq!(summary.history.commit_count, 253);
}

#[test]
fn cancelling_while_parsing_keeps_the_history_and_builds_the_graph_later() {
    let directory = tempfile::tempdir().unwrap();
    history(directory.path(), 3);
    let config = AtlasConfig::default();
    let mut database = Database::open_in_memory().unwrap();

    let work = Work::default();
    let stopper = work.clone();
    database.set_work(work.reporting(move |progress| {
        if progress.unit == ProgressUnit::Files {
            stopper.cancel();
        }
    }));
    let error = analyze(&mut database, directory.path(), &at("HEAD"), &config).unwrap_err();
    assert!(matches!(error, AtlasError::Cancelled), "{error}");

    database.set_work(Work::default());
    let outcome = analyze(&mut database, directory.path(), &at("HEAD"), &config).unwrap();
    assert!(outcome.cached);
    let query = GraphQuery {
        git_ref: None,
        directory: None,
        search: None,
        limit: 10,
        include_cochange: false,
    };
    let response = graph(&database, &outcome.repository, &query, &config).unwrap();
    assert_eq!(response.nodes.len(), 2);
}

#[test]
fn a_cancelled_backtest_stores_nothing() {
    let directory = tempfile::tempdir().unwrap();
    generate_demo_fixture(directory.path()).unwrap();
    let config = AtlasConfig::default();
    let mut database = Database::open_in_memory().unwrap();
    let outcome = analyze(&mut database, directory.path(), &at("HEAD"), &config).unwrap();

    let work = Work::default();
    let stopper = work.clone();
    let units = Arc::new(Mutex::new(Vec::<JobProgress>::new()));
    let sink = Arc::clone(&units);
    database.set_work(work.reporting(move |progress| {
        sink.lock().unwrap().push(progress);
        if progress.unit == ProgressUnit::Cases {
            stopper.cancel();
        }
    }));
    let request = BacktestRequest {
        warmup: Some(60),
        ..BacktestRequest::default()
    };
    let error = backtest(&database, &outcome.repository, &request, &config, |_, _| {}).unwrap_err();
    assert!(matches!(error, AtlasError::Cancelled), "{error}");
    assert!(
        database
            .latest_backtest(outcome.repository.id)
            .unwrap()
            .is_none()
    );
    assert_eq!(units.lock().unwrap().last().unwrap().done, 1);
}

#[test]
fn a_passed_deadline_stops_queries_and_keeps_their_work() {
    let directory = tempfile::tempdir().unwrap();
    let config = AtlasConfig::default();
    history(directory.path(), 30);
    let mut database = Database::open_in_memory().unwrap();
    let outcome = analyze(&mut database, directory.path(), &at("HEAD~1"), &config).unwrap();

    database.set_work(Work::within(Duration::ZERO));
    let error = timeline(&database, &outcome.repository, 10, &config).unwrap_err();
    assert!(matches!(error, AtlasError::TimedOut), "{error}");

    database.set_work(Work::default());
    let points = timeline(&database, &outcome.repository, 10, &config).unwrap();
    assert_eq!(points.points.len(), 10);
}

#[test]
fn reports_escape_repository_text_and_run_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let mut repo = FixtureRepo::init(directory.path()).unwrap();
    let hostile = "<script>alert(1)</script>";
    repo.change()
        .unwrap()
        .write("src/<img src=x onerror=alert(2)>.ts", exports("a", 2))
        .write(
            "src/a.ts",
            "import { a0 } from './<img src=x onerror=alert(2)>';\n",
        )
        .commit(hostile)
        .unwrap();
    repo.change()
        .unwrap()
        .append("src/a.ts", "export const b = 1;\n")
        .commit("</td><script>alert(3)</script>")
        .unwrap();
    let config = AtlasConfig::default();
    let mut database = Database::open_in_memory().unwrap();
    let outcome = analyze(&mut database, directory.path(), &at("HEAD"), &config).unwrap();

    let document = report(&database, &outcome.repository, &config).unwrap();
    let html = atlas_analysis::render_report(&document);
    assert!(!html.contains("<script"), "{html}");
    assert!(!html.contains("<img"), "{html}");
    assert!(
        html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"),
        "{html}"
    );
    assert!(html.contains("default-src 'none'"));
}

#[test]
fn reports_describe_one_commit_and_hide_the_local_path() {
    let directory = tempfile::tempdir().unwrap();
    generate_demo_fixture(directory.path()).unwrap();
    let config = AtlasConfig::default();
    let mut database = Database::open_in_memory().unwrap();
    let outcome = analyze(&mut database, directory.path(), &at("HEAD~1"), &config).unwrap();
    let request = BacktestRequest {
        warmup: Some(60),
        max_cases: Some(10),
        ..BacktestRequest::default()
    };
    backtest(&database, &outcome.repository, &request, &config, |_, _| {}).unwrap();

    let document = report(&database, &outcome.repository, &config).unwrap();
    assert!(document.backtest.is_some());
    assert_eq!(
        document.architecture.as_ref().unwrap().file_count,
        document.timeline.last().unwrap().file_count
    );
    let html = atlas_analysis::render_report(&document);
    let json = serde_json::to_string(&document).unwrap();
    let parent = directory.path().parent().unwrap().to_str().unwrap();
    assert!(!html.contains(parent));
    assert!(!json.contains(parent));
    assert!(html.contains("does not measure bugs") || html.contains("not measure bugs"));

    // The backtest above belongs to the older commit, so a newer report omits it.
    let newer = analyze(&mut database, directory.path(), &at("HEAD"), &config).unwrap();
    let document = report(&database, &newer.repository, &config).unwrap();
    assert!(document.backtest.is_none());
}
