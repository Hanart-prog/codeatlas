//! Temporal backtesting: for each later commit, rank files from what was knowable
//! at its parent and compare them with what the commit actually changed.
//!
//! The history handed to a ranking is a slice ending at the parent, and the graph
//! is the parent's snapshot, so a case cannot see its own commit or anything
//! after it. See `docs/evaluation.md`.

use std::collections::{BTreeSet, HashSet};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use atlas_git::{ExclusionRules, GitRepository, is_typescript};
use atlas_graph::DependencyGraph;
use atlas_model::history::{ChangeStatus, CommitChanges};
use atlas_model::{
    ALGORITHM_VERSION, AtlasConfig, AtlasError, BACKTEST_VERSION, BacktestCase, BacktestCounts,
    BacktestEntry, BacktestRequest, BacktestResult, CaseRanking, CommitSummary, Estimate,
    EvidenceKind, MetricsAtK, PARSER_VERSION, Ranker, RankerResult, RepoPath, RepositoryEntry,
    TrendBucket, TrendRate,
};
use atlas_storage::Database;

use crate::cochange::{ScopedHistory, is_eligible, scope_history};
use crate::impact::{DEFAULT_HISTORY_WINDOW, DEFAULT_MAX_DEPTH, Weights, cutoff_graph, rank};
use crate::timeline::sample;

const DEFAULT_WARMUP: u32 = 100;
const DEFAULT_MAX_CASES: u32 = 200;
const DEFAULT_TOP_K: [u32; 2] = [5, 10];
const DEFAULT_SEED: u32 = 1;

const MAX_CASES: u32 = 1000;
const MAX_TOP_K: u32 = 100;
const MAX_CUTOFFS: usize = 8;
const MAX_WINDOW: u32 = 5000;

const BOOTSTRAP_RESAMPLES: usize = 1000;
const TREND_BUCKETS: usize = 10;

/// Below this many cases, averages move too much to compare rankers.
const LOW_SAMPLE: u32 = 30;

const RANKERS: [Ranker; 5] = [
    Ranker::Combined,
    Ranker::CoChange,
    Ranker::ReverseDependency,
    Ranker::MostChanged,
    Ranker::Random,
];

struct Params {
    warmup: u32,
    max_cases: u32,
    top_k: Vec<u32>,
    window: u32,
    seed: u32,
}

fn validate(request: &BacktestRequest) -> Result<Params, AtlasError> {
    let invalid = |message: String| Err(AtlasError::InvalidInput(message));
    let mut top_k = request
        .top_k
        .clone()
        .unwrap_or_else(|| DEFAULT_TOP_K.to_vec());
    top_k.sort_unstable();
    top_k.dedup();
    let params = Params {
        warmup: request.warmup.unwrap_or(DEFAULT_WARMUP),
        max_cases: request.max_cases.unwrap_or(DEFAULT_MAX_CASES),
        top_k,
        window: request.history_window.unwrap_or(DEFAULT_HISTORY_WINDOW),
        seed: request.seed.unwrap_or(DEFAULT_SEED),
    };
    if params.warmup == 0 {
        return invalid("warmup must be at least 1".to_owned());
    }
    if !(1..=MAX_CASES).contains(&params.max_cases) {
        return invalid(format!("max_cases must be between 1 and {MAX_CASES}"));
    }
    if params.top_k.is_empty() || params.top_k.len() > MAX_CUTOFFS {
        return invalid(format!("give between 1 and {MAX_CUTOFFS} top_k values"));
    }
    if params.top_k.iter().any(|k| !(1..=MAX_TOP_K).contains(k)) {
        return invalid(format!("top_k values must be between 1 and {MAX_TOP_K}"));
    }
    if !(1..=MAX_WINDOW).contains(&params.window) {
        return invalid(format!("history_window must be between 1 and {MAX_WINDOW}"));
    }
    Ok(params)
}

/// SplitMix64: small, stable across platforms, and good enough for shuffles and
/// resampling. Results must not change with a dependency's release.
struct Random(u64);

impl Random {
    fn new(seed: u32, stream: u64) -> Self {
        Self(u64::from(seed) << 32 ^ stream)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A value below `bound`.
    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }

    fn shuffle<T>(&mut self, items: &mut [T]) {
        for last in (1..items.len()).rev() {
            items.swap(last, self.below(last + 1));
        }
    }
}

/// A commit that can be evaluated.
#[derive(Debug, PartialEq, Eq)]
struct PlannedCase {
    position: usize,
    /// Its eligible files that existed at the parent, at parent paths, sorted.
    files: Vec<RepoPath>,
    new_files: u32,
    renamed: Vec<RepoPath>,
    deleted: Vec<RepoPath>,
}

/// Sorts every commit into warm-up, skipped or evaluable.
fn plan_cases(
    commits: &[CommitChanges],
    warmup: u32,
    bulk_threshold: u32,
) -> (Vec<PlannedCase>, BacktestCounts) {
    let mut counts = BacktestCounts {
        commit_count: commits.len() as u32,
        ..BacktestCounts::default()
    };
    let mut cases = Vec::new();

    for (position, commit) in commits.iter().enumerate() {
        let eligible: Vec<_> = commit.changes.iter().filter(|c| is_eligible(c)).collect();
        if eligible.is_empty() {
            counts.no_eligible_file_commits += 1;
            continue;
        }
        if eligible.len() > bulk_threshold as usize {
            counts.bulk_commits += 1;
            continue;
        }
        if counts.warmup_commits < warmup {
            counts.warmup_commits += 1;
            continue;
        }
        counts.candidate_cases += 1;
        if eligible.len() < 2 {
            counts.too_few_files += 1;
            continue;
        }

        let mut files = BTreeSet::new();
        let (mut renamed, mut deleted) = (Vec::new(), Vec::new());
        for change in &eligible {
            // New files have no old path, and a file moved in from another
            // language was not in the parent's graph.
            let Some(path) = change.old_path.as_ref().filter(|path| is_typescript(path)) else {
                continue;
            };
            files.insert(path.clone());
            match change.status {
                ChangeStatus::Renamed => renamed.push(path.clone()),
                ChangeStatus::Deleted => deleted.push(path.clone()),
                _ => {}
            }
        }
        match files.len() {
            0 => counts.no_input_at_parent += 1,
            1 => counts.no_target_at_parent += 1,
            _ => cases.push(PlannedCase {
                position,
                new_files: (eligible.len() - files.len()) as u32,
                files: files.into_iter().collect(),
                renamed,
                deleted,
            }),
        }
    }
    counts.evaluable_cases = cases.len() as u32;
    (cases, counts)
}

/// Everything a ranker can use for one case: nothing later than the parent.
struct Knowledge<'a> {
    graph: &'a DependencyGraph,
    history: &'a ScopedHistory,
    rules: &'a ExclusionRules,
    seed: u32,
    position: u64,
    config: &'a AtlasConfig,
}

/// One ranking: its top paths and how many files it could rank.
struct Ranked {
    paths: Vec<RepoPath>,
    candidate_count: u32,
}

impl Knowledge<'_> {
    /// The Change Impact ranking, keeping only files with `required` evidence.
    fn weighted(
        &self,
        input: &RepoPath,
        weights: Weights,
        required: Option<EvidenceKind>,
    ) -> Vec<RepoPath> {
        let selected = BTreeSet::from([input.clone()]);
        let ranked = rank(
            self.graph,
            self.history,
            &selected,
            DEFAULT_MAX_DEPTH,
            weights,
            self.config,
        );
        ranked
            .into_iter()
            .filter(|item| item.score > 0.0)
            .filter(|item| required.is_none_or(|kind| item.evidence.contains(&kind)))
            .map(|item| item.path)
            .collect()
    }

    /// Files alive at the parent that changed in the window, with their counts.
    fn changed_files(&self, input: &RepoPath) -> Vec<(u32, RepoPath)> {
        let mut files: Vec<(u32, RepoPath)> = self
            .history
            .alive
            .iter()
            .filter(|(path, _)| *path != input)
            .filter(|(path, _)| is_typescript(path) && !self.rules.is_excluded(path))
            .map(|(path, &id)| {
                let count = self.history.index.commits_of(id).len() as u32;
                (count, path.clone())
            })
            .filter(|(count, _)| *count > 0)
            .collect();
        files.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        files
    }

    fn run(&self, ranker: Ranker, input: &RepoPath) -> Vec<RepoPath> {
        let only_history = Weights {
            static_weight: 0.0,
            historical_weight: 1.0,
        };
        let only_imports = Weights {
            static_weight: 1.0,
            historical_weight: 0.0,
        };
        match ranker {
            Ranker::Combined => self.weighted(input, Weights::DEFAULT, None),
            Ranker::CoChange => {
                self.weighted(input, only_history, Some(EvidenceKind::HistoricalCoChange))
            }
            Ranker::ReverseDependency => {
                self.weighted(input, only_imports, Some(EvidenceKind::StaticDependency))
            }
            Ranker::MostChanged => self
                .changed_files(input)
                .into_iter()
                .map(|(_, path)| path)
                .collect(),
            Ranker::Random => {
                let mut paths: Vec<RepoPath> = self
                    .changed_files(input)
                    .into_iter()
                    .map(|(_, path)| path)
                    .collect();
                paths.sort();
                Random::new(self.seed, self.position).shuffle(&mut paths);
                paths
            }
        }
    }

    fn ranked(&self, ranker: Ranker, input: &RepoPath, largest_k: usize) -> Ranked {
        let mut paths = self.run(ranker, input);
        let candidate_count = paths.len() as u32;
        paths.truncate(largest_k);
        Ranked {
            paths,
            candidate_count,
        }
    }
}

fn hits(paths: &[RepoPath], targets: &[RepoPath], k: u32) -> u32 {
    let targets: HashSet<&RepoPath> = targets.iter().collect();
    paths
        .iter()
        .take(k as usize)
        .filter(|path| targets.contains(path))
        .count() as u32
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

/// A percentile bootstrap of the mean over cases. Every metric resamples the same
/// cases, because each uses a fresh generator from the same seed.
fn estimate(values: &[f64], seed: u32) -> Estimate {
    if values.is_empty() {
        return Estimate {
            mean: 0.0,
            low: 0.0,
            high: 0.0,
        };
    }
    let mut random = Random::new(seed, u64::MAX);
    let mut means: Vec<f64> = (0..BOOTSTRAP_RESAMPLES)
        .map(|_| {
            let sum: f64 = (0..values.len())
                .map(|_| values[random.below(values.len())])
                .sum();
            sum / values.len() as f64
        })
        .collect();
    means.sort_by(f64::total_cmp);
    Estimate {
        mean: mean(values),
        low: means[BOOTSTRAP_RESAMPLES * 25 / 1000],
        high: means[BOOTSTRAP_RESAMPLES * 975 / 1000 - 1],
    }
}

/// `(ranking, targets)` per case, for one way of ranking.
fn metrics(outcomes: &[(&[RepoPath], &[RepoPath])], top_k: &[u32], seed: u32) -> Vec<MetricsAtK> {
    top_k
        .iter()
        .map(|&k| {
            let found: Vec<u32> = outcomes
                .iter()
                .map(|(paths, targets)| hits(paths, targets, k))
                .collect();
            let per_case = |value: &dyn Fn(u32, usize) -> f64| -> Vec<f64> {
                found
                    .iter()
                    .zip(outcomes)
                    .map(|(&hit, (_, targets))| value(hit, targets.len()))
                    .collect()
            };
            MetricsAtK {
                k,
                precision: estimate(&per_case(&|hit, _| f64::from(hit) / f64::from(k)), seed),
                recall: estimate(&per_case(&|hit, total| f64::from(hit) / total as f64), seed),
                hit_rate: estimate(&per_case(&|hit, _| f64::from(hit.min(1))), seed),
            }
        })
        .collect()
}

fn ranker_result(cases: &[BacktestCase], ranker: Ranker, top_k: &[u32], seed: u32) -> RankerResult {
    let rankings: Vec<&CaseRanking> = cases
        .iter()
        .filter_map(|case| case.rankings.iter().find(|r| r.ranker == ranker))
        .collect();
    let outcomes: Vec<(&[RepoPath], &[RepoPath])> = rankings
        .iter()
        .zip(cases)
        .map(|(ranking, case)| (ranking.paths.as_slice(), case.targets.as_slice()))
        .collect();
    let candidates: Vec<f64> = rankings
        .iter()
        .map(|ranking| f64::from(ranking.candidate_count))
        .collect();
    RankerResult {
        ranker,
        metrics: metrics(&outcomes, top_k, seed),
        mean_candidate_count: mean(&candidates),
        empty_case_count: rankings.iter().filter(|r| r.candidate_count == 0).count() as u32,
    }
}

fn trend(cases: &[BacktestCase], top_k: &[u32]) -> Vec<TrendBucket> {
    let buckets = TREND_BUCKETS.min(cases.len());
    (0..buckets)
        .map(|bucket| {
            let slice =
                &cases[bucket * cases.len() / buckets..(bucket + 1) * cases.len() / buckets];
            let mut rates = Vec::new();
            for ranker in RANKERS {
                for &k in top_k {
                    let hit = slice
                        .iter()
                        .filter(|case| {
                            case.rankings
                                .iter()
                                .find(|r| r.ranker == ranker)
                                .is_some_and(|r| hits(&r.paths, &case.targets, k) > 0)
                        })
                        .count();
                    rates.push(TrendRate {
                        ranker,
                        k,
                        hit_rate: hit as f64 / slice.len() as f64,
                    });
                }
            }
            TrendBucket {
                from_position: slice.first().map_or(0, |case| case.position),
                to_position: slice.last().map_or(0, |case| case.position),
                case_count: slice.len() as u32,
                rates,
            }
        })
        .collect()
}

fn notes(counts: &BacktestCounts, is_shallow: bool, is_truncated: bool) -> Vec<String> {
    let mut notes = Vec::new();
    if counts.warmup_commits == 0 && counts.evaluated_cases == 0 {
        notes.push(
            "No commit changes a TS or TSX file, so there is nothing to evaluate.".to_owned(),
        );
    } else if counts.evaluated_cases == 0 {
        notes.push(format!(
            "No commit could be evaluated: {} eligible commits went to warm-up and {} remained. \
             Lower the warm-up or analyze more history.",
            counts.warmup_commits, counts.candidate_cases
        ));
    } else if counts.evaluated_cases < LOW_SAMPLE {
        notes.push(format!(
            "Only {} cases were evaluated, so these averages can move a lot with a few commits.",
            counts.evaluated_cases
        ));
    }
    if counts.evaluated_cases < counts.evaluable_cases {
        notes.push(format!(
            "{} of {} evaluable cases were used, evenly spaced through history.",
            counts.evaluated_cases, counts.evaluable_cases
        ));
    }
    if counts.merge_cases > 0 {
        notes.push(format!(
            "{} cases are merge commits, which can bundle unrelated work.",
            counts.merge_cases
        ));
    }
    if is_shallow {
        notes.push("This is a shallow clone, so older history is missing.".to_owned());
    }
    if is_truncated {
        notes.push(
            "The analysis stopped before the first commit, so the earliest cases know less."
                .to_owned(),
        );
    }
    notes
}

/// Runs a backtest, or returns the stored result of an identical one. `progress`
/// gets `(done, total)` after each case.
pub fn backtest(
    database: &Database,
    repository: &RepositoryEntry,
    request: &BacktestRequest,
    config: &AtlasConfig,
    mut progress: impl FnMut(u32, u32),
) -> Result<BacktestEntry, AtlasError> {
    let params = validate(request)?;
    let run = database.latest_run(repository.id)?.ok_or_else(|| {
        AtlasError::NotFound(format!("{} has not been analyzed yet", repository.path))
    })?;

    let config_hash = config.analysis_hash();
    let key = serde_json::json!({
        "warmup": params.warmup,
        "max_cases": params.max_cases,
        "top_k": params.top_k,
        "history_window": params.window,
        "seed": params.seed,
        "backtest_version": BACKTEST_VERSION,
        "parser_version": PARSER_VERSION,
        "algorithm_version": ALGORITHM_VERSION,
        "config_hash": config_hash,
    })
    .to_string();
    if let Some(stored) = database.backtest(run.id, &key)? {
        return Ok(stored);
    }

    let git = GitRepository::open(Path::new(&repository.path))?;
    let rules = ExclusionRules::from_config(&config.analysis, &config.limits)?;
    let walk = git.first_parent_history(&run.target_oid, run.history_limit)?;
    let commits = walk
        .oids
        .iter()
        .map(|oid| git.commit_changes(oid, &rules))
        .collect::<Result<Vec<_>, _>>()?;

    let threshold = config.analysis.bulk_commit_file_threshold;
    let (planned, mut counts) = plan_cases(&commits, params.warmup, threshold);
    let chosen: Vec<&PlannedCase> = sample(counts.evaluable_cases, params.max_cases)
        .into_iter()
        .map(|index| &planned[index as usize])
        .collect();
    counts.evaluated_cases = chosen.len() as u32;

    let total = chosen.len() as u32;
    let largest_k = params.top_k.iter().copied().max().unwrap_or(0) as usize;
    let mut cases = Vec::with_capacity(chosen.len());
    let mut seeded = Vec::with_capacity(chosen.len());

    for (done, case) in chosen.iter().enumerate() {
        let position = case.position;
        let commit = &commits[position].commit;
        let parent = commits[position - 1].commit.oid.clone();

        // The only history a case gets: commits up to and including its parent.
        let start = position.saturating_sub(params.window as usize);
        let history = scope_history(
            commits[start..position].to_vec(),
            git.is_shallow(),
            start > 0 || walk.is_truncated,
            config,
        );
        let graph = cutoff_graph(database, &git, repository, &parent, config)?;
        let knowledge = Knowledge {
            graph: &graph,
            history: &history,
            rules: &rules,
            seed: params.seed,
            position: position as u64,
            config,
        };

        let input = &case.files[0];
        let rankings = RANKERS
            .iter()
            .map(|&ranker| {
                let ranked = knowledge.ranked(ranker, input, largest_k);
                CaseRanking {
                    ranker,
                    paths: ranked.paths,
                    candidate_count: ranked.candidate_count,
                }
            })
            .collect();

        // The same case with an arbitrary input, for the sensitivity check.
        let mut stream = Random::new(params.seed, position as u64 ^ (1 << 63));
        let alternative = stream.below(case.files.len());
        let other_input = &case.files[alternative];
        let other_targets: Vec<RepoPath> = case
            .files
            .iter()
            .filter(|path| *path != other_input)
            .cloned()
            .collect();
        seeded.push((
            knowledge
                .ranked(Ranker::Combined, other_input, largest_k)
                .paths,
            other_targets,
        ));

        counts.merge_cases += u32::from(commit.parents.len() > 1);
        counts.target_files += case.files.len() as u32 - 1;
        counts.new_files_excluded += case.new_files;
        counts.renamed_targets += case.renamed.iter().filter(|p| *p != input).count() as u32;
        counts.deleted_targets += case.deleted.iter().filter(|p| *p != input).count() as u32;

        cases.push(BacktestCase {
            position: position as u32,
            commit: CommitSummary {
                oid: commit.oid.clone(),
                committed_at: commit.committed_at,
                subject: commit.subject.clone(),
            },
            parent,
            input: input.clone(),
            targets: case.files[1..].to_vec(),
            rankings,
        });
        progress(done as u32 + 1, total);
    }

    let seeded_outcomes: Vec<(&[RepoPath], &[RepoPath])> = seeded
        .iter()
        .map(|(paths, targets)| (paths.as_slice(), targets.as_slice()))
        .collect();
    let result = BacktestResult {
        target_oid: run.target_oid.clone(),
        warmup: params.warmup,
        max_cases: params.max_cases,
        history_window: params.window,
        seed: params.seed,
        parser_version: PARSER_VERSION,
        algorithm_version: ALGORITHM_VERSION,
        backtest_version: BACKTEST_VERSION,
        config_hash,
        coverage: match counts.candidate_cases {
            0 => 0.0,
            candidates => f64::from(counts.evaluable_cases) / f64::from(candidates),
        },
        rankers: RANKERS
            .iter()
            .map(|&ranker| ranker_result(&cases, ranker, &params.top_k, params.seed))
            .collect(),
        seeded_input: metrics(&seeded_outcomes, &params.top_k, params.seed),
        trend: trend(&cases, &params.top_k),
        notes: notes(&counts, git.is_shallow(), walk.is_truncated),
        top_k: params.top_k,
        counts,
        cases,
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64);
    Ok(database.store_backtest(run.id, &key, &result, now)?)
}

#[cfg(test)]
mod tests {
    use atlas_model::RepoPath;
    use proptest::prelude::*;

    use super::{Random, estimate, hits, metrics, trend};

    fn p(name: &str) -> RepoPath {
        name.parse().unwrap()
    }

    #[test]
    fn hits_count_only_within_k() {
        let (paths, targets) = ([p("a"), p("b"), p("c")], [p("c"), p("z")]);
        assert_eq!(hits(&paths, &targets, 2), 0);
        assert_eq!(hits(&paths, &targets, 3), 1);
        assert_eq!(hits(&paths, &targets, 10), 1);
    }

    #[test]
    fn precision_divides_by_k_even_with_fewer_results() {
        let (paths, targets) = ([p("a")], [p("a"), p("b")]);
        let outcomes: [(&[RepoPath], &[RepoPath]); 1] = [(&paths, &targets)];
        let result = &metrics(&outcomes, &[4], 1)[0];
        assert_eq!(result.precision.mean, 0.25);
        assert_eq!(result.recall.mean, 0.5);
        assert_eq!(result.hit_rate.mean, 1.0);
    }

    #[test]
    fn identical_values_have_no_spread() {
        let result = estimate(&[0.5; 20], 7);
        assert_eq!((result.low, result.mean, result.high), (0.5, 0.5, 0.5));
    }

    #[test]
    fn an_interval_is_reproducible_and_depends_on_the_seed() {
        let values: Vec<f64> = (0..50).map(|i| f64::from(i * i % 97) / 97.0).collect();
        assert_eq!(estimate(&values, 1), estimate(&values, 1));
        assert_ne!(estimate(&values, 1), estimate(&values, 2));
    }

    #[test]
    fn an_interval_is_about_as_wide_as_theory_says() {
        // A proportion of 0.5 over 100 cases has a standard error of 0.05.
        let values: Vec<f64> = (0..100).map(|i| f64::from(i % 2)).collect();
        let result = estimate(&values, 1);
        assert!((0.38..=0.42).contains(&result.low), "{result:?}");
        assert!((0.58..=0.62).contains(&result.high), "{result:?}");
    }

    #[test]
    fn the_generator_is_stable_across_releases() {
        let mut random = Random::new(1, 0);
        assert_eq!(random.next(), Random::new(1, 0).next());
        let mut items: Vec<u32> = (0..6).collect();
        Random::new(1, 2).shuffle(&mut items);
        let mut again: Vec<u32> = (0..6).collect();
        Random::new(1, 2).shuffle(&mut again);
        assert_eq!(items, again);
    }

    #[test]
    fn no_cases_means_no_trend() {
        assert!(trend(&[], &[5]).is_empty());
    }

    proptest! {
        #[test]
        fn a_shuffle_keeps_every_item(seed: u32, stream: u64, size in 0usize..50) {
            let mut items: Vec<usize> = (0..size).collect();
            Random::new(seed, stream).shuffle(&mut items);
            items.sort_unstable();
            prop_assert_eq!(items, (0..size).collect::<Vec<_>>());
        }

        #[test]
        fn an_interval_contains_its_bounds_in_order(
            values in prop::collection::vec(0.0f64..=1.0, 1..40),
            seed: u32,
        ) {
            let result = estimate(&values, seed);
            prop_assert!(result.low <= result.high);
            prop_assert!(result.low >= 0.0 && result.high <= 1.0);
        }
    }
}
