//! Builds the import snapshot of a commit from Git blobs.
//!
//! Everything comes from the commit's tree, never from the working tree, so
//! historical snapshots cannot pick up today's files.

use std::collections::{HashMap, HashSet};

use atlas_git::{ExclusionRules, GitRepository, TreeFile, TreeListing};
use atlas_model::{
    AtlasConfig, AtlasError, DependencyEdgeType, GraphEdge, Language, Oid, PARSER_VERSION,
    ParseStatus, ParsedFile, ParserCoverage, RepoPath, RepositoryEntry, SnapshotData, SnapshotFile,
    TsconfigStatus, UnresolvedCount, UnresolvedImport, UnresolvedReason,
};
use atlas_parser::{ParsedTsconfig, Resolution, Resolver, parse_source, parse_tsconfig};
use atlas_storage::{Database, SnapshotId, SnapshotKey};

const TSCONFIG_PATH: &str = "tsconfig.json";

/// Returns the snapshot of `commit`, building it if needed.
pub fn ensure_snapshot(
    database: &Database,
    git: &GitRepository,
    commit: &Oid,
    config: &AtlasConfig,
) -> Result<SnapshotId, AtlasError> {
    let repository = database.upsert_repository(&git.canonical_path())?;
    snapshot_for(database, git, &repository, commit, config)
}

pub(crate) fn snapshot_for(
    database: &Database,
    git: &GitRepository,
    repository: &RepositoryEntry,
    commit: &Oid,
    config: &AtlasConfig,
) -> Result<SnapshotId, AtlasError> {
    let config_hash = config.analysis_hash();
    let key = SnapshotKey {
        repository_id: repository.id,
        commit,
        parser_version: PARSER_VERSION,
        config_hash: &config_hash,
    };
    if let Some(id) = database.snapshot_id(&key)? {
        return Ok(id);
    }
    let data = build(database, git, commit, config)?;
    Ok(database.store_snapshot(&key, &data)?)
}

fn build(
    database: &Database,
    git: &GitRepository,
    commit: &Oid,
    config: &AtlasConfig,
) -> Result<SnapshotData, AtlasError> {
    let rules = ExclusionRules::from_config(&config.analysis, &config.limits)?;
    let TreeListing { files, .. } = git.tree_files(commit)?;

    let mut excluded = 0;
    let mut unsupported = 0;
    let mut sources: Vec<(&TreeFile, Language)> = Vec::new();
    for file in &files {
        if rules.is_excluded(&file.path) {
            excluded += 1;
        } else if let Some(language) = Language::of(&file.path) {
            sources.push((file, language));
        } else {
            unsupported += 1;
        }
    }
    if sources.len() > config.limits.max_graph_nodes as usize {
        return Err(AtlasError::InvalidInput(format!(
            "{} source files exceed limits.max_graph_nodes ({})",
            sources.len(),
            config.limits.max_graph_nodes
        )));
    }

    let tsconfig = load_tsconfig(git, &files, config)?;
    let source_paths: HashSet<String> = sources
        .iter()
        .map(|(file, _)| file.path.to_string())
        .collect();
    let all_paths: HashSet<String> = files.iter().map(|file| file.path.to_string()).collect();
    let resolver = Resolver {
        sources: &source_paths,
        all_files: &all_paths,
        tsconfig: tsconfig.as_ref(),
    };

    let mut snapshot_files = Vec::with_capacity(sources.len());
    let mut edges: HashMap<(String, String, u8), GraphEdge> = HashMap::new();
    let mut unresolved = Vec::new();
    let mut external = 0;
    let mut coverage_counts = Counts::default();
    let mut indexed_bytes = 0u64;

    for (file, language) in sources {
        let parsed = load_parsed(database, git, file, language, config, &mut indexed_bytes)?;
        coverage_counts.record(&parsed);

        let mut file_external = 0;
        for import in &parsed.imports {
            match resolver.resolve(file.path.as_str(), &import.specifier) {
                Resolution::Local { path, kind } => {
                    coverage_counts.resolved += 1;
                    let target: RepoPath = path.parse()?;
                    let key = (file.path.to_string(), path, import.kind as u8);
                    edges.entry(key).or_insert_with(|| GraphEdge {
                        source: file.path.clone(),
                        target,
                        edge_type: import.kind,
                        resolution: kind,
                        specifier: import.specifier.clone(),
                        line: import.line,
                    });
                }
                Resolution::External => file_external += 1,
                Resolution::Unresolved(reason) => unresolved.push((
                    file.path.clone(),
                    UnresolvedImport {
                        specifier: import.specifier.clone(),
                        reason,
                        line: import.line,
                    },
                )),
            }
        }
        external += file_external;
        snapshot_files.push(SnapshotFile {
            path: file.path.clone(),
            language,
            blob: file.blob.clone(),
            loc: parsed.loc,
            parse_status: parsed.status,
            external_import_count: file_external,
        });
    }

    if edges.len() > config.limits.max_graph_edges as usize {
        return Err(AtlasError::InvalidInput(format!(
            "{} import edges exceed limits.max_graph_edges ({})",
            edges.len(),
            config.limits.max_graph_edges
        )));
    }
    let mut edges: Vec<GraphEdge> = edges.into_values().collect();
    edges.sort_by(|a, b| {
        (&a.source, &a.target, a.edge_type as u8).cmp(&(&b.source, &b.target, b.edge_type as u8))
    });

    let coverage = ParserCoverage {
        parsed_file_count: coverage_counts.parsed,
        partial_file_count: coverage_counts.partial,
        skipped_file_count: coverage_counts.skipped,
        unsupported_file_count: unsupported,
        excluded_file_count: excluded,
        resolved_import_count: coverage_counts.resolved,
        external_import_count: external,
        unresolved_import_count: unresolved.len() as u32,
        unresolved_by_reason: count_reasons(&unresolved),
        nonliteral_dynamic_import_count: coverage_counts.nonliteral,
        require_call_count: coverage_counts.require,
        tsconfig: tsconfig
            .as_ref()
            .and_then(|config| config.status)
            .unwrap_or(TsconfigStatus::Absent),
    };

    Ok(SnapshotData {
        commit: commit.clone(),
        files: snapshot_files,
        edges,
        unresolved,
        coverage,
    })
}

fn load_tsconfig(
    git: &GitRepository,
    files: &[TreeFile],
    config: &AtlasConfig,
) -> Result<Option<ParsedTsconfig>, AtlasError> {
    let Some(file) = files
        .iter()
        .find(|file| file.path.as_str() == TSCONFIG_PATH)
    else {
        return Ok(None);
    };
    let bytes = if file.size > config.limits.max_file_bytes {
        Vec::new()
    } else {
        git.read_blob(&file.blob)?
    };
    Ok(Some(parse_tsconfig(&bytes)))
}

fn load_parsed(
    database: &Database,
    git: &GitRepository,
    file: &TreeFile,
    language: Language,
    config: &AtlasConfig,
    indexed_bytes: &mut u64,
) -> Result<ParsedFile, AtlasError> {
    let skipped = |status| ParsedFile {
        status,
        loc: 0,
        imports: Vec::new(),
        nonliteral_dynamic_imports: 0,
        require_calls: 0,
    };
    if file.size > config.limits.max_file_bytes {
        return Ok(skipped(ParseStatus::Oversized));
    }
    *indexed_bytes += file.size;
    if *indexed_bytes > config.limits.max_total_indexed_bytes {
        return Ok(skipped(ParseStatus::OverTotalLimit));
    }

    if let Some(parsed) = database.parsed_blob(&file.blob, PARSER_VERSION, language)? {
        return Ok(parsed);
    }
    let parsed = parse_source(&git.read_blob(&file.blob)?, language);
    database.store_parsed_blob(&file.blob, PARSER_VERSION, language, &parsed)?;
    Ok(parsed)
}

#[derive(Default)]
struct Counts {
    parsed: u32,
    partial: u32,
    skipped: u32,
    resolved: u32,
    nonliteral: u32,
    require: u32,
}

impl Counts {
    fn record(&mut self, file: &ParsedFile) {
        match file.status {
            ParseStatus::Parsed => self.parsed += 1,
            ParseStatus::Partial => {
                self.parsed += 1;
                self.partial += 1;
            }
            _ => self.skipped += 1,
        }
        self.nonliteral += file.nonliteral_dynamic_imports;
        self.require += file.require_calls;
    }
}

fn count_reasons(unresolved: &[(RepoPath, UnresolvedImport)]) -> Vec<UnresolvedCount> {
    let mut counts: HashMap<UnresolvedReason, u32> = HashMap::new();
    for (_, import) in unresolved {
        *counts.entry(import.reason).or_default() += 1;
    }
    let mut counts: Vec<UnresolvedCount> = counts
        .into_iter()
        .map(|(reason, count)| UnresolvedCount { reason, count })
        .collect();
    counts.sort_by_key(|entry| (std::cmp::Reverse(entry.count), entry.reason as u8));
    counts
}

/// True for edges that TypeScript erases at compile time.
pub(crate) fn is_type_only(edge_type: DependencyEdgeType) -> bool {
    edge_type == DependencyEdgeType::TypeOnlyImport
}
