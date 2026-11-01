//! Checks backtests against hand-calculated results and for look-ahead.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use atlas_analysis::{AnalyzeOptions, analyze, backtest, impact};
use atlas_fixtures::{FixtureRepo, exports, generate_main_fixture};
use atlas_model::{
    AtlasConfig, BacktestCase, BacktestCounts, BacktestRequest, BacktestResult, ImpactRequest,
    Ranker, RepositoryEntry,
};
use atlas_storage::Database;
use serde::Deserialize;

#[derive(Deserialize)]
struct Expected {
    coverage: f64,
    counts: BacktestCounts,
    cases: Vec<ExpectedCase>,
    metrics: BTreeMap<String, BTreeMap<u32, [f64; 3]>>,
    mean_candidates: BTreeMap<String, f64>,
    empty_cases: BTreeMap<String, u32>,
}

#[derive(Deserialize)]
struct ExpectedCase {
    subject: String,
    input: String,
    targets: Vec<String>,
    combined: Vec<String>,
    co_change: Vec<String>,
    reverse_dependency: Vec<String>,
    most_changed: Vec<String>,
}

const A: &str = "src/a.ts";
const B: &str = "src/b.ts";
const C: &str = "src/c.ts";
const D: &str = "src/d.ts";

/// `b` imports `a`, and `a` and `c` keep changing together. The numbers after each
/// commit are its position; `fixtures/expected/backtest.json` is derived from them.
fn generate(directory: &Path) -> FixtureRepo {
    let mut repo = FixtureRepo::init(directory).unwrap();
    // 0: every file.
    repo.change()
        .unwrap()
        .write(A, exports("a", 12))
        .write(
            B,
            format!("import {{ a0 }} from './a';\n{}", exports("b", 12)),
        )
        .write(C, exports("c", 12))
        .write(D, exports("d", 12))
        .commit("Create")
        .unwrap();
    // 1-3: a and c together.
    for round in 1..=3 {
        repo.change()
            .unwrap()
            .append(A, &exports(&format!("a{round}x"), 1))
            .append(C, &exports(&format!("c{round}x"), 1))
            .commit(&format!("Change a and c, round {round}"))
            .unwrap();
    }
    // 4: last of the warm-up.
    repo.change()
        .unwrap()
        .append(D, &exports("d4x", 1))
        .commit("Change d")
        .unwrap();
    // 5: case. Input a, targets b and c.
    repo.change()
        .unwrap()
        .append(A, &exports("a5x", 1))
        .append(B, &exports("b5x", 1))
        .append(C, &exports("c5x", 1))
        .commit("Change a, b and c")
        .unwrap();
    // 6: skipped. e is new, so d has nothing to predict.
    repo.change()
        .unwrap()
        .append(D, &exports("d6x", 1))
        .write("src/e.ts", exports("e", 3))
        .commit("Change d and add e")
        .unwrap();
    // 7: skipped. One file.
    repo.change()
        .unwrap()
        .append(B, &exports("b7x", 1))
        .commit("Change b")
        .unwrap();
    // 8: case. Input c, target d.
    repo.change()
        .unwrap()
        .append(C, &exports("c8x", 1))
        .append(D, &exports("d8x", 1))
        .commit("Change c and d")
        .unwrap();
    // 9: case. Input a, target d under its old path.
    repo.change()
        .unwrap()
        .append(A, &exports("a9x", 1))
        .rename(D, "src/f.ts")
        .commit("Change a and rename d")
        .unwrap();
    repo
}

fn setup(path: &Path) -> (Database, RepositoryEntry, AtlasConfig) {
    let config = AtlasConfig::default();
    let mut database = Database::open_in_memory().unwrap();
    let outcome = analyze(&mut database, path, &AnalyzeOptions::default(), &config).unwrap();
    (database, outcome.repository, config)
}

fn run(
    database: &Database,
    repository: &RepositoryEntry,
    config: &AtlasConfig,
    warmup: u32,
    top_k: &[u32],
) -> BacktestResult {
    let request = BacktestRequest {
        warmup: Some(warmup),
        top_k: Some(top_k.to_vec()),
        ..BacktestRequest::default()
    };
    backtest(database, repository, &request, config, |_, _| {})
        .unwrap()
        .result
}

fn ranking(case: &BacktestCase, ranker: Ranker) -> Vec<String> {
    let found = case.rankings.iter().find(|r| r.ranker == ranker).unwrap();
    found.paths.iter().map(ToString::to_string).collect()
}

fn close(actual: f64, expected: f64, what: &str) {
    assert!(
        (actual - expected).abs() < 1e-5,
        "{what}: {actual} != {expected}"
    );
}

const NAMED: [(&str, Ranker); 4] = [
    ("combined", Ranker::Combined),
    ("co_change", Ranker::CoChange),
    ("reverse_dependency", Ranker::ReverseDependency),
    ("most_changed", Ranker::MostChanged),
];

#[test]
fn metrics_match_hand_calculation() {
    let expected: Expected =
        serde_json::from_str(include_str!("../../../fixtures/expected/backtest.json")).unwrap();
    let repo = tempfile::tempdir().unwrap();
    generate(repo.path());
    let (database, repository, config) = setup(repo.path());
    let result = run(&database, &repository, &config, 5, &[1, 2]);

    assert_eq!(result.counts, expected.counts);
    close(result.coverage, expected.coverage, "coverage");
    assert_eq!(result.cases.len(), expected.cases.len());

    for (case, wanted) in result.cases.iter().zip(&expected.cases) {
        let name = &wanted.subject;
        assert_eq!(&case.commit.subject, name);
        assert_eq!(case.input.as_str(), wanted.input, "{name}");
        let targets: Vec<String> = case.targets.iter().map(ToString::to_string).collect();
        assert_eq!(targets, wanted.targets, "{name}");
        assert_eq!(ranking(case, Ranker::Combined), wanted.combined, "{name}");
        assert_eq!(ranking(case, Ranker::CoChange), wanted.co_change, "{name}");
        assert_eq!(
            ranking(case, Ranker::ReverseDependency),
            wanted.reverse_dependency,
            "{name}"
        );
        assert_eq!(
            ranking(case, Ranker::MostChanged),
            wanted.most_changed,
            "{name}"
        );
    }

    for (name, ranker) in NAMED {
        let actual = result.rankers.iter().find(|r| r.ranker == ranker).unwrap();
        close(
            actual.mean_candidate_count,
            expected.mean_candidates[name],
            name,
        );
        if let Some(&empty) = expected.empty_cases.get(name) {
            assert_eq!(actual.empty_case_count, empty, "{name}");
        }
        for metrics in &actual.metrics {
            let [precision, recall, hit_rate] = expected.metrics[name][&metrics.k];
            let what = format!("{name}@{}", metrics.k);
            close(metrics.precision.mean, precision, &what);
            close(metrics.recall.mean, recall, &what);
            close(metrics.hit_rate.mean, hit_rate, &what);
        }
    }
}

#[test]
fn every_case_ranks_exactly_like_change_impact_at_its_parent() {
    for fixture in [true, false] {
        let repo = tempfile::tempdir().unwrap();
        if fixture {
            generate_main_fixture(repo.path()).unwrap();
        } else {
            generate(repo.path());
        }
        let (database, repository, config) = setup(repo.path());
        for window in [None, Some(3), Some(4)] {
            let request = BacktestRequest {
                warmup: Some(3),
                top_k: Some(vec![10]),
                history_window: window,
                ..BacktestRequest::default()
            };
            let result = backtest(&database, &repository, &request, &config, |_, _| {})
                .unwrap()
                .result;
            assert!(!result.cases.is_empty());

            for case in &result.cases {
                let request = ImpactRequest {
                    selected_paths: vec![case.input.to_string()],
                    cutoff: Some(case.parent.to_string()),
                    history_window: window,
                    ..ImpactRequest::default()
                };
                let response = impact(&database, &repository, &request, &config).unwrap();
                let served: Vec<String> = response
                    .recommendations
                    .iter()
                    .map(|item| item.path.to_string())
                    .collect();
                assert_eq!(
                    ranking(case, Ranker::Combined),
                    served,
                    "{} with window {window:?}",
                    case.commit.subject
                );
            }
        }
    }
}

#[test]
fn a_new_file_and_an_excluded_file_are_not_targets() {
    let repo = tempfile::tempdir().unwrap();
    let mut fixture = FixtureRepo::init(repo.path()).unwrap();
    fixture
        .change()
        .unwrap()
        .write(A, exports("a", 6))
        .write(B, exports("b", 6))
        .write("vendor/v.ts", exports("v", 6))
        .commit("Create")
        .unwrap();
    fixture
        .change()
        .unwrap()
        .append(A, &exports("a1x", 1))
        .append(B, &exports("b1x", 1))
        .append("vendor/v.ts", &exports("v1x", 1))
        .write("src/n.ts", exports("n", 3))
        .commit("Change everything")
        .unwrap();
    let (database, repository, config) = setup(repo.path());

    let result = run(&database, &repository, &config, 1, &[5]);
    assert_eq!(result.cases[0].input.as_str(), A);
    let targets: Vec<&str> = result.cases[0].targets.iter().map(|p| p.as_str()).collect();
    assert_eq!(targets, [B]);
    assert_eq!(result.counts.new_files_excluded, 1);
}

#[test]
fn each_case_gets_its_own_random_order() {
    let repo = tempfile::tempdir().unwrap();
    let mut fixture = FixtureRepo::init(repo.path()).unwrap();
    let names: Vec<String> = (0..12).map(|i| format!("src/f{i:02}.ts")).collect();
    let mut create = fixture.change().unwrap();
    for name in &names {
        create = create.write(name, exports("x", 6));
    }
    create.commit("Create").unwrap();
    for round in 0..2 {
        fixture
            .change()
            .unwrap()
            .append(&names[0], &exports(&format!("a{round}x"), 1))
            .append(&names[1], &exports(&format!("b{round}x"), 1))
            .commit(&format!("Round {round}"))
            .unwrap();
    }
    let (database, repository, config) = setup(repo.path());

    let result = run(&database, &repository, &config, 1, &[10]);
    let first = ranking(&result.cases[0], Ranker::Random);
    let second = ranking(&result.cases[1], Ranker::Random);
    assert_eq!(first.len(), second.len());
    assert_ne!(first, second);
}

fn by_commit(result: &BacktestResult) -> HashMap<String, &BacktestCase> {
    result
        .cases
        .iter()
        .map(|case| (case.commit.oid.to_string(), case))
        .collect()
}

#[test]
fn later_commits_cannot_change_an_earlier_case() {
    let repo = tempfile::tempdir().unwrap();
    let mut fixture = generate(repo.path());
    let (database, repository, config) = setup(repo.path());
    let before = run(&database, &repository, &config, 5, &[1, 2, 10]);

    // Written to change the answers if anything leaked: c and d now always change
    // with a and b, and d imports a.
    for round in 0..8 {
        fixture
            .change()
            .unwrap()
            .append(A, &exports(&format!("leak{round}a"), 1))
            .append(B, &exports(&format!("leak{round}b"), 1))
            .append("src/f.ts", &exports(&format!("leak{round}f"), 1))
            .append(
                "src/e.ts",
                &format!(
                    "import {{ a0 }} from './a';\n{}",
                    exports(&format!("leak{round}e"), 1)
                ),
            )
            .commit("Future work")
            .unwrap();
    }
    let (database, repository, config) = setup(repo.path());
    let after = run(&database, &repository, &config, 5, &[1, 2, 10]);

    assert!(
        after.cases.len() > before.cases.len(),
        "the new commits must be cases"
    );
    let later = by_commit(&after);
    for case in &before.cases {
        assert_eq!(Some(case), later.get(case.commit.oid.as_str()).copied());
    }
}

#[test]
fn a_commit_never_informs_its_own_case() {
    let repo = tempfile::tempdir().unwrap();
    let mut fixture = FixtureRepo::init(repo.path()).unwrap();
    fixture
        .change()
        .unwrap()
        .write(A, exports("a", 6))
        .commit("Create")
        .unwrap();
    fixture
        .change()
        .unwrap()
        .write(B, exports("b", 6))
        .commit("Add b")
        .unwrap();
    for round in 0..3 {
        fixture
            .change()
            .unwrap()
            .append(A, &exports(&format!("a{round}y"), 1))
            .append(B, &exports(&format!("b{round}y"), 1))
            .commit(&format!("Together {round}"))
            .unwrap();
    }
    let (database, repository, config) = setup(repo.path());
    let result = run(&database, &repository, &config, 2, &[1]);

    // The first case's own commit is the first time a and b change together, so
    // it must see no shared history.
    let found: Vec<usize> = result
        .cases
        .iter()
        .map(|case| ranking(case, Ranker::CoChange).len())
        .collect();
    assert_eq!(found, [0, 1, 1]);
}

#[test]
fn a_repeat_returns_the_stored_result_and_new_settings_do_not() {
    let repo = tempfile::tempdir().unwrap();
    generate(repo.path());
    let (database, repository, config) = setup(repo.path());

    let request = BacktestRequest {
        warmup: Some(5),
        ..BacktestRequest::default()
    };
    let mut steps = Vec::new();
    let first = backtest(&database, &repository, &request, &config, |done, total| {
        steps.push((done, total));
    })
    .unwrap();
    assert_eq!(steps, [(1, 3), (2, 3), (3, 3)]);

    let mut repeats = 0;
    let again = backtest(&database, &repository, &request, &config, |_, _| {
        repeats += 1
    })
    .unwrap();
    assert_eq!(first, again);
    assert_eq!(repeats, 0);

    let seeded = BacktestRequest {
        seed: Some(2),
        ..request
    };
    let other = backtest(&database, &repository, &seeded, &config, |_, _| {}).unwrap();
    assert_eq!(other.result.seed, 2);
    assert_eq!(
        database.latest_backtest(repository.id).unwrap(),
        Some(other)
    );
}

#[test]
fn random_is_seeded_and_seeded_inputs_are_reported() {
    let repo = tempfile::tempdir().unwrap();
    generate(repo.path());
    let (database, repository, config) = setup(repo.path());

    let result = run(&database, &repository, &config, 5, &[1, 2]);
    assert_eq!(result.seeded_input.len(), 2);
    let random = result
        .rankers
        .iter()
        .find(|r| r.ranker == Ranker::Random)
        .unwrap();
    let most = result
        .rankers
        .iter()
        .find(|r| r.ranker == Ranker::MostChanged)
        .unwrap();
    assert_eq!(random.mean_candidate_count, most.mean_candidate_count);
    assert_eq!(result.trend.iter().map(|b| b.case_count).sum::<u32>(), 3);
}

#[test]
fn a_bulk_commit_is_neither_warm_up_nor_a_case() {
    let repo = tempfile::tempdir().unwrap();
    generate(repo.path());
    let mut config = AtlasConfig::default();
    config.analysis.bulk_commit_file_threshold = 2;
    let mut database = Database::open_in_memory().unwrap();
    let repository = analyze(
        &mut database,
        repo.path(),
        &AnalyzeOptions::default(),
        &config,
    )
    .unwrap()
    .repository;

    let result = run(&database, &repository, &config, 3, &[1]);
    // Create (4 files) and "Change a, b and c" (3 files) are over the threshold.
    assert_eq!(result.counts.bulk_commits, 2);
    assert_eq!(result.counts.warmup_commits, 3);
}

#[test]
fn too_little_history_is_explained() {
    let repo = tempfile::tempdir().unwrap();
    generate(repo.path());
    let (database, repository, config) = setup(repo.path());
    let result = run(&database, &repository, &config, 100, &[1]);
    assert_eq!(result.counts.evaluated_cases, 0);
    assert_eq!(result.coverage, 0.0);
    assert!(result.notes[0].contains("No commit could be evaluated"));
}

#[test]
fn bad_parameters_are_rejected() {
    let repo = tempfile::tempdir().unwrap();
    generate(repo.path());
    let (database, repository, config) = setup(repo.path());
    let bad = [
        BacktestRequest {
            warmup: Some(0),
            ..Default::default()
        },
        BacktestRequest {
            max_cases: Some(0),
            ..Default::default()
        },
        BacktestRequest {
            max_cases: Some(5000),
            ..Default::default()
        },
        BacktestRequest {
            top_k: Some(Vec::new()),
            ..Default::default()
        },
        BacktestRequest {
            top_k: Some(vec![0]),
            ..Default::default()
        },
        BacktestRequest {
            top_k: Some(vec![500]),
            ..Default::default()
        },
        BacktestRequest {
            history_window: Some(0),
            ..Default::default()
        },
    ];
    for request in bad {
        assert!(
            backtest(&database, &repository, &request, &config, |_, _| {}).is_err(),
            "{request:?}"
        );
    }
}
