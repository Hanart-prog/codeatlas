//! The change impact simulator.
//!
//! Ranks files to inspect from two kinds of evidence that stay separate in the
//! output: reverse import dependencies and historical co-change. Everything is
//! read as of the cutoff. See `docs/algorithms.md` for the definitions.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

use atlas_git::{ExclusionRules, GitRepository, is_typescript};
use atlas_graph::DependencyGraph;
use atlas_model::{
    AtlasConfig, AtlasError, CommitSummary, DependencyEdgeType, EvidenceKind, HistoricalEvidence,
    ImpactRecommendation, ImpactRequest, ImpactResponse, Oid, RepoPath, RepositoryEntry,
    SelectedFile, SelectedFileStatus, StaticEvidence,
};
use atlas_storage::Database;

use crate::cochange::{ScopedHistory, load_scoped_history};
use crate::snapshot::snapshot_for;

pub(crate) const DEFAULT_MAX_DEPTH: u32 = 3;
const DEFAULT_TOP_K: u32 = 10;
pub(crate) const DEFAULT_HISTORY_WINDOW: u32 = 200;
const DEFAULT_STATIC_WEIGHT: f64 = 0.05;
const DEFAULT_HISTORICAL_WEIGHT: f64 = 1.0;

const MAX_SELECTED: usize = 50;
const MAX_DEPTH: u32 = 10;
const MAX_TOP_K: u32 = 100;
const MAX_WINDOW: u32 = 5000;

/// Supporting commits listed per recommendation.
const EVIDENCE_COMMITS: usize = 5;

/// Scores within this precision tie, so that floating-point noise cannot decide
/// an order, and the tie falls to the path.
const SCORE_PRECISION: f64 = 1_000_000.0;

/// Parameters after defaults and validation.
struct Params {
    max_depth: u32,
    top_k: u32,
    window: u32,
    weights: Weights,
}

/// How static and historical evidence combine.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Weights {
    pub static_weight: f64,
    pub historical_weight: f64,
}

impl Weights {
    /// The Change Impact defaults.
    pub const DEFAULT: Self = Self {
        static_weight: DEFAULT_STATIC_WEIGHT,
        historical_weight: DEFAULT_HISTORICAL_WEIGHT,
    };
}

fn validate(request: &ImpactRequest) -> Result<Params, AtlasError> {
    let invalid = |message: String| Err(AtlasError::InvalidInput(message));
    if request.selected_paths.is_empty() {
        return invalid("select at least one file".to_owned());
    }
    if request.selected_paths.len() > MAX_SELECTED {
        return invalid(format!("select at most {MAX_SELECTED} files"));
    }
    let params = Params {
        max_depth: request.max_depth.unwrap_or(DEFAULT_MAX_DEPTH),
        top_k: request.top_k.unwrap_or(DEFAULT_TOP_K),
        window: request.history_window.unwrap_or(DEFAULT_HISTORY_WINDOW),
        weights: Weights {
            static_weight: request.static_weight.unwrap_or(DEFAULT_STATIC_WEIGHT),
            historical_weight: request
                .historical_weight
                .unwrap_or(DEFAULT_HISTORICAL_WEIGHT),
        },
    };
    if !(1..=MAX_DEPTH).contains(&params.max_depth) {
        return invalid(format!("max_depth must be between 1 and {MAX_DEPTH}"));
    }
    if !(1..=MAX_TOP_K).contains(&params.top_k) {
        return invalid(format!("top_k must be between 1 and {MAX_TOP_K}"));
    }
    if !(1..=MAX_WINDOW).contains(&params.window) {
        return invalid(format!("history_window must be between 1 and {MAX_WINDOW}"));
    }
    let weights = [
        params.weights.static_weight,
        params.weights.historical_weight,
    ];
    if weights.iter().any(|w| !w.is_finite() || *w < 0.0) || weights.iter().sum::<f64>() <= 0.0 {
        return invalid("weights must be non-negative and not both zero".to_owned());
    }
    Ok(params)
}

/// Ranks files to inspect when changing the selected files.
pub fn impact(
    database: &Database,
    repository: &RepositoryEntry,
    request: &ImpactRequest,
    config: &AtlasConfig,
) -> Result<ImpactResponse, AtlasError> {
    let params = validate(request)?;
    let git = GitRepository::open(Path::new(&repository.path))?;
    let cutoff = resolve_cutoff(database, repository, &git, request.cutoff.as_deref())?;

    let selected_paths: BTreeSet<RepoPath> = request
        .selected_paths
        .iter()
        .map(|path| path.parse())
        .collect::<Result<_, _>>()?;

    let graph = cutoff_graph(database, &git, repository, &cutoff, config)?;
    let history = load_scoped_history(&git, &cutoff, params.window, config)?;

    let rules = ExclusionRules::from_config(&config.analysis, &config.limits)?;
    let tree: HashSet<RepoPath> = git
        .tree_files(&cutoff)?
        .files
        .into_iter()
        .map(|file| file.path)
        .collect();

    let mut selected = Vec::with_capacity(selected_paths.len());
    let mut notes = Vec::new();
    for path in &selected_paths {
        if !tree.contains(path) {
            return Err(AtlasError::NotFound(format!(
                "{path} does not exist at commit {cutoff}. Paths are relative to the \
                 repository root."
            )));
        }
        let status = if rules.is_excluded(path) {
            SelectedFileStatus::Excluded
        } else if is_typescript(path) {
            SelectedFileStatus::Analyzed
        } else {
            SelectedFileStatus::UnsupportedLanguage
        };
        let commit_count = history
            .alive
            .get(path)
            .map_or(0, |&id| history.index.commits_of(id).len() as u32);
        let direct_dependent_count = graph.in_degree(path);
        notes.extend(selected_note(
            path,
            status,
            commit_count,
            direct_dependent_count,
        ));
        selected.push(SelectedFile {
            path: path.clone(),
            status,
            commit_count,
            direct_dependent_count,
        });
    }

    let mut ranked = rank(
        &graph,
        &history,
        &selected_paths,
        params.max_depth,
        params.weights,
        config,
    );
    let candidate_count = ranked.len() as u32;
    ranked.truncate(params.top_k as usize);

    let linked = ranked
        .iter()
        .any(|item| !item.evidence.contains(&EvidenceKind::ActivityOnly));
    if !linked {
        notes.push(
            "No file has import or co-change evidence here. That is an absence of evidence, \
             not a sign that the change is safe. Anything listed only changes often."
                .to_owned(),
        );
    }
    Ok(ImpactResponse {
        cutoff,
        selected,
        recommendations: ranked,
        candidate_count,
        history: history.stats,
        max_depth: params.max_depth,
        static_weight: params.weights.static_weight,
        historical_weight: params.weights.historical_weight,
        notes,
    })
}

/// Every candidate with evidence, best first. Change Impact and the backtest both
/// rank through this, so a backtest measures the ranking users see.
pub(crate) fn rank(
    graph: &DependencyGraph,
    history: &ScopedHistory,
    selected: &BTreeSet<RepoPath>,
    max_depth: u32,
    weights: Weights,
    config: &AtlasConfig,
) -> Vec<ImpactRecommendation> {
    let candidates = collect_candidates(graph, selected, max_depth);
    let historical = historical_evidence(history, selected);

    let mut paths: BTreeSet<&RepoPath> = candidates.keys().collect();
    paths.extend(historical.scores.keys());

    let mut ranked: Vec<ImpactRecommendation> = paths
        .into_iter()
        .map(|path| {
            recommendation(
                path,
                candidates.get(path).cloned(),
                historical.evidence.get(path).cloned(),
                historical.scores.get(path).copied().unwrap_or(0.0),
                weights,
                config,
            )
        })
        .collect();
    ranked.sort_by_key(|item| (Reverse(score_key(item.score)), item.path.clone()));
    for (index, item) in ranked.iter_mut().enumerate() {
        item.rank = index as u32 + 1;
    }
    ranked
}

fn resolve_cutoff(
    database: &Database,
    repository: &RepositoryEntry,
    git: &GitRepository,
    cutoff: Option<&str>,
) -> Result<Oid, AtlasError> {
    match cutoff {
        Some(reference) => Ok(git.resolve(reference)?),
        None => database
            .latest_run(repository.id)?
            .map(|run| run.target_oid)
            .ok_or_else(|| {
                AtlasError::NotFound(format!("{} has not been analyzed yet", repository.path))
            }),
    }
}

/// The import graph of the cutoff commit, so that today's imports never leak
/// into a historical ranking.
pub(crate) fn cutoff_graph(
    database: &Database,
    git: &GitRepository,
    repository: &RepositoryEntry,
    cutoff: &Oid,
    config: &AtlasConfig,
) -> Result<DependencyGraph, AtlasError> {
    let snapshot = snapshot_for(database, git, repository, cutoff, config)?;
    let nodes = database.snapshot_nodes(snapshot)?;
    let mut graph = DependencyGraph::new(nodes.into_iter().map(|node| node.path));
    for edge in database.snapshot_edges(snapshot)? {
        let type_only = edge.edge_type == DependencyEdgeType::TypeOnlyImport;
        graph.add_edge(&edge.source, &edge.target, type_only);
    }
    Ok(graph)
}

fn selected_note(
    path: &RepoPath,
    status: SelectedFileStatus,
    commit_count: u32,
    dependents: u32,
) -> Option<String> {
    match status {
        SelectedFileStatus::Excluded => Some(format!(
            "{path} matches an exclusion rule, so it has no import or co-change evidence."
        )),
        SelectedFileStatus::UnsupportedLanguage => Some(format!(
            "{path} is not TS or TSX, so it has no import or co-change evidence."
        )),
        SelectedFileStatus::Analyzed if commit_count == 0 && dependents == 0 => Some(format!(
            "{path} has no eligible commits in this window and no file imports it."
        )),
        SelectedFileStatus::Analyzed if commit_count == 0 => {
            Some(format!("{path} has no eligible commits in this window."))
        }
        SelectedFileStatus::Analyzed if dependents == 0 => {
            Some(format!("No file imports {path} directly at this commit."))
        }
        SelectedFileStatus::Analyzed => None,
    }
}

/// Reverse dependencies within range, keeping the nearest selected file. Selected
/// files are iterated in path order, so a distance tie resolves deterministically.
fn collect_candidates(
    graph: &DependencyGraph,
    selected: &BTreeSet<RepoPath>,
    max_depth: u32,
) -> BTreeMap<RepoPath, StaticEvidence> {
    let mut candidates: BTreeMap<RepoPath, StaticEvidence> = BTreeMap::new();
    for path in selected {
        for (candidate, chain) in graph.reverse_paths(path, max_depth) {
            if selected.contains(&candidate) {
                continue;
            }
            let distance = chain.len() as u32 - 1;
            let nearer = candidates
                .get(&candidate)
                .is_none_or(|existing| distance < existing.distance);
            if nearer {
                candidates.insert(
                    candidate,
                    StaticEvidence {
                        distance,
                        path: chain,
                    },
                );
            }
        }
    }
    candidates
}

struct Historical {
    /// Smoothed co-change score per candidate.
    scores: BTreeMap<RepoPath, f64>,
    /// Best evidence per candidate that shares a commit with a selected file.
    evidence: BTreeMap<RepoPath, HistoricalEvidence>,
}

/// `(shared + rate) / (selected + 1)`, the chance a file changes with the selected
/// one, shrunk toward how often it changes at all. One shared commit counts a
/// little, and a file with none falls back to its own activity. Selected files
/// are iterated in path order, so ties resolve deterministically.
fn historical_evidence(history: &ScopedHistory, selected: &BTreeSet<RepoPath>) -> Historical {
    let mut result = Historical {
        scores: BTreeMap::new(),
        evidence: BTreeMap::new(),
    };
    let eligible = history.index.eligible_commit_count();

    // Deleted files cannot be inspected, and selected files need no recommendation.
    for (candidate, &other) in &history.alive {
        let candidate_count = history.index.commits_of(other).len() as u32;
        if candidate_count == 0 || selected.contains(candidate) {
            continue;
        }
        let rate = f64::from(candidate_count) / f64::from(eligible);
        let (mut best_score, mut best_evidence) = (0.0, 0.0);
        for path in selected {
            let identity = history.alive.get(path).copied();
            let selected_count = identity.map_or(0, |id| history.index.commits_of(id).len() as u32);
            let shared = identity.map_or(&[][..], |id| history.index.shared(id, other));
            let score = (shared.len() as f64 + rate) / f64::from(selected_count + 1);
            if score > best_score {
                best_score = score;
            }
            if !shared.is_empty() && score > best_evidence {
                best_evidence = score;
                let evidence =
                    pair_evidence(history, path, other, shared, selected_count, eligible);
                result.evidence.insert(candidate.clone(), evidence);
            }
        }
        result.scores.insert(candidate.clone(), best_score);
    }
    result
}

fn pair_evidence(
    history: &ScopedHistory,
    selected: &RepoPath,
    candidate: usize,
    shared: &[u32],
    selected_count: u32,
    eligible: u32,
) -> HistoricalEvidence {
    let shared_count = shared.len() as u32;
    let candidate_count = history.index.commits_of(candidate).len() as u32;
    let conditional_probability = f64::from(shared_count) / f64::from(selected_count);
    let base_rate = f64::from(candidate_count) / f64::from(eligible);
    HistoricalEvidence {
        selected: selected.clone(),
        shared_commit_count: shared_count,
        selected_commit_count: selected_count,
        candidate_commit_count: candidate_count,
        conditional_probability,
        jaccard: f64::from(shared_count)
            / f64::from(selected_count + candidate_count - shared_count),
        base_rate,
        lift: conditional_probability / base_rate,
        commits: shared
            .iter()
            .rev()
            .take(EVIDENCE_COMMITS)
            .map(|&position| {
                let commit = &history.commits[position as usize].commit;
                CommitSummary {
                    oid: commit.oid.clone(),
                    committed_at: commit.committed_at,
                    subject: commit.subject.clone(),
                }
            })
            .collect(),
    }
}

fn recommendation(
    path: &RepoPath,
    static_evidence: Option<StaticEvidence>,
    historical_evidence: Option<HistoricalEvidence>,
    historical_score: f64,
    weights: Weights,
    config: &AtlasConfig,
) -> ImpactRecommendation {
    let static_score = static_evidence
        .as_ref()
        .map_or(0.0, |evidence| 1.0 / f64::from(evidence.distance));

    let mut evidence = Vec::new();
    let mut caveats = Vec::new();
    if static_evidence.is_some() {
        evidence.push(EvidenceKind::StaticDependency);
    }
    match &historical_evidence {
        Some(history) => {
            evidence.push(EvidenceKind::HistoricalCoChange);
            if static_evidence.is_none() {
                caveats.push(
                    "No import path connects these files, so this is correlation in history only."
                        .to_owned(),
                );
            }
            if history.shared_commit_count < config.analysis.min_shared_commits {
                caveats.push(format!(
                    "Changed together only {} time(s), so this is weak evidence.",
                    history.shared_commit_count
                ));
            }
            if history.lift < 1.2 {
                caveats.push(format!(
                    "Changes in {:.0}% of commits anyway, so this pairing says little.",
                    history.base_rate * 100.0
                ));
            }
        }
        None if static_evidence.is_none() => {
            evidence.push(EvidenceKind::ActivityOnly);
            caveats.push(
                "Nothing links this file to the change. It is listed because it changes often."
                    .to_owned(),
            );
        }
        None => {}
    }

    ImpactRecommendation {
        rank: 0,
        path: path.clone(),
        score: weights.static_weight * static_score + weights.historical_weight * historical_score,
        static_score,
        historical_score,
        evidence,
        static_evidence,
        historical_evidence,
        caveats,
    }
}

fn score_key(score: f64) -> i64 {
    (score * SCORE_PRECISION).round() as i64
}

#[cfg(test)]
mod tests {
    use super::score_key;

    #[test]
    fn floating_point_noise_does_not_break_ties() {
        assert_eq!(score_key(0.1 + 0.2), score_key(0.3));
        assert!(score_key(0.300_01) > score_key(0.3));
    }
}
