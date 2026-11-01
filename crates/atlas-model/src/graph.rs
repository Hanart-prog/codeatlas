//! Import graphs. Static imports are evidence of a dependency in source; they
//! say nothing about what runs, and are kept apart from co-change correlation.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::dto::{DependencyEdgeType, FileChurn};
use crate::ids::{Oid, RepoPath};

/// The grammar a source file is parsed with. `.ts` and `.tsx` differ, because
/// `<T>x` is a cast in one and JSX in the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    /// `.ts`, `.mts`, `.cts`.
    Typescript,
    /// `.tsx`.
    Tsx,
}

impl Language {
    /// The grammar for a path, if it is a supported source file.
    pub fn of(path: &RepoPath) -> Option<Self> {
        let path = path.as_str();
        if path.ends_with(".tsx") {
            Some(Self::Tsx)
        } else if [".ts", ".mts", ".cts"].iter().any(|e| path.ends_with(e)) {
            Some(Self::Typescript)
        } else {
            None
        }
    }
}

/// What happened when a source file was parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ParseStatus {
    /// Parsed without syntax errors.
    Parsed,
    /// The parser recovered from syntax errors, so imports may be incomplete.
    Partial,
    /// No syntax tree was produced.
    Failed,
    /// Over `limits.max_file_bytes`, so not parsed.
    Oversized,
    /// Over `limits.max_total_indexed_bytes`, so not parsed.
    OverTotalLimit,
    /// A Git LFS pointer, whose real content is not in the repository.
    LfsPointer,
}

/// One import as written in source, before resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportRecord {
    /// The module specifier. Untrusted text.
    pub specifier: String,
    /// How it was imported.
    pub kind: DependencyEdgeType,
    /// One-based source line.
    pub line: u32,
}

/// The result of parsing one blob. Cached by blob id and parser version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedFile {
    /// Outcome.
    pub status: ParseStatus,
    /// Non-blank lines.
    pub loc: u32,
    /// Imports, in source order.
    pub imports: Vec<ImportRecord>,
    /// `import(...)` calls whose argument is not a literal.
    pub nonliteral_dynamic_imports: u32,
    /// `require(...)` calls, which are not analyzed.
    pub require_calls: u32,
}

/// How an import was matched to a local file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionKind {
    /// A relative path.
    Relative,
    /// A `tsconfig` `paths` entry.
    Alias,
    /// A path under `tsconfig` `baseUrl`.
    BaseUrl,
}

/// Why an import that is not a package has no local edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum UnresolvedReason {
    /// No candidate file exists.
    NotFound,
    /// The target exists but is not a supported source file, or is excluded.
    NonSourceTarget,
    /// The path leaves the repository root.
    EscapesRepository,
    /// A `tsconfig` alias matched, but no candidate file exists.
    AliasTargetMissing,
    /// It looks like an alias, such as `@/x`, but nothing defines it.
    UnknownAlias,
    /// A form that is not supported, such as `#internal` or an empty path.
    UnsupportedSpecifier,
}

/// How the root `tsconfig.json` was used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum TsconfigStatus {
    /// There is no root `tsconfig.json`.
    Absent,
    /// It was read and applied.
    Applied,
    /// It was applied, but it uses `extends`, which is not followed.
    AppliedIgnoringExtends,
    /// It could not be parsed, so no aliases apply.
    Invalid,
}

/// Number of imports left unresolved for one reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UnresolvedCount {
    /// Why.
    pub reason: UnresolvedReason,
    /// How many imports.
    pub count: u32,
}

/// What a snapshot covers. See `docs/algorithms.md`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ParserCoverage {
    /// Files parsed, including partly.
    pub parsed_file_count: u32,
    /// Files parsed with syntax errors.
    pub partial_file_count: u32,
    /// Files with no syntax tree, or skipped by a limit or as LFS pointers.
    pub skipped_file_count: u32,
    /// Files in other languages, which are not analyzed.
    pub unsupported_file_count: u32,
    /// Files left out by exclusion rules.
    pub excluded_file_count: u32,
    /// Imports resolved to a local file.
    pub resolved_import_count: u32,
    /// Imports classified as external packages.
    pub external_import_count: u32,
    /// Imports with no local edge and not known to be external.
    pub unresolved_import_count: u32,
    /// Unresolved imports by reason, most common first.
    pub unresolved_by_reason: Vec<UnresolvedCount>,
    /// `import(...)` calls with a non-literal argument, not in the graph.
    pub nonliteral_dynamic_import_count: u32,
    /// `require(...)` calls, not in the graph.
    pub require_call_count: u32,
    /// How the root `tsconfig.json` was used.
    pub tsconfig: TsconfigStatus,
}

/// A file in the import graph. Degrees count distinct files in the whole
/// snapshot, even when the response is filtered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GraphNode {
    /// Path at this commit.
    pub path: RepoPath,
    /// Grammar used.
    pub language: Language,
    /// Non-blank lines.
    pub loc: u32,
    /// Parse outcome.
    pub parse_status: ParseStatus,
    /// Files importing this one.
    pub in_degree: u32,
    /// Files this one imports.
    pub out_degree: u32,
    /// Whether this file is part of an import cycle.
    pub in_cycle: bool,
}

/// `source -> target` means `source` imports `target`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GraphEdge {
    /// The importing file.
    pub source: RepoPath,
    /// The imported file.
    pub target: RepoPath,
    /// Kind of import.
    pub edge_type: DependencyEdgeType,
    /// How the target was found.
    pub resolution: ResolutionKind,
    /// The specifier as written. Untrusted text.
    pub specifier: String,
    /// One-based line of the first such import.
    pub line: u32,
}

/// A strongly connected component of the import graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Cycle {
    /// Member files, sorted.
    pub members: Vec<RepoPath>,
    /// The cycle disappears without `import type` edges, so it cannot occur at
    /// run time.
    pub type_only: bool,
}

/// Body of `GET /api/v1/repositories/:id/graph`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GraphResponse {
    /// The commit this graph describes.
    pub commit: Oid,
    /// Files, most connected first, ties by path.
    pub nodes: Vec<GraphNode>,
    /// Edges between the returned files.
    pub edges: Vec<GraphEdge>,
    /// All cycles in the snapshot, largest first.
    pub cycles: Vec<Cycle>,
    /// What the snapshot covers.
    pub coverage: ParserCoverage,
    /// Co-change pairs among the returned files, when requested. These are
    /// correlation, not dependencies, so they are not in `edges`.
    pub cochange: Vec<crate::impact::CoChangeEdge>,
    /// Files matching the request before `limit` applied.
    pub matching_node_count: u32,
    /// Files in the whole snapshot.
    pub total_node_count: u32,
    /// Edges in the whole snapshot.
    pub total_edge_count: u32,
}

/// What a box on the directory map stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum DirectoryGroupKind {
    /// A subdirectory of the map's directory, with everything under it.
    Directory,
    /// The files directly in the map's directory.
    LooseFiles,
}

/// A group of files on the directory map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DirectoryGroup {
    /// Directory path; the map's own directory for loose files, empty at the root.
    pub path: String,
    /// What the group stands for.
    pub kind: DirectoryGroupKind,
    /// Source files in the group.
    pub file_count: u32,
    /// Files in the group that are in an import cycle.
    pub cyclic_file_count: u32,
    /// Imports between files of the group.
    pub internal_import_count: u32,
}

/// Imports from one group to another. Every import is counted, with no regard to
/// kind; they say nothing about what runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DirectoryLink {
    /// Index in `groups` of the importing group.
    pub source: u32,
    /// Index in `groups` of the imported group.
    pub target: u32,
    /// Imports between them.
    pub import_count: u32,
}

/// Body of `GET /api/v1/repositories/:id/graph/directories`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DirectoryMap {
    /// The commit described.
    pub commit: Oid,
    /// The directory shown: the one asked for, or deeper when it held only one
    /// subdirectory. Empty at the repository root.
    pub directory: String,
    /// Groups, most files first, ties by path.
    pub groups: Vec<DirectoryGroup>,
    /// Imports between the returned groups, by source then target.
    pub links: Vec<DirectoryLink>,
    /// Groups left out by `limit`.
    pub hidden_group_count: u32,
    /// Files in those groups.
    pub hidden_file_count: u32,
    /// Files under `directory`.
    pub matching_node_count: u32,
    /// Files in the whole snapshot.
    pub total_node_count: u32,
    /// Edges in the whole snapshot.
    pub total_edge_count: u32,
}

/// An import with no local edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UnresolvedImport {
    /// The specifier as written. Untrusted text.
    pub specifier: String,
    /// Why it has no edge.
    pub reason: UnresolvedReason,
    /// One-based source line.
    pub line: u32,
}

/// Body of `GET /api/v1/repositories/:id/files?path=`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FileDetail {
    /// The commit described.
    pub commit: Oid,
    /// The file.
    pub node: GraphNode,
    /// Local files it imports.
    pub imports: Vec<GraphEdge>,
    /// Local files importing it.
    pub imported_by: Vec<GraphEdge>,
    /// Imports with no local edge.
    pub unresolved: Vec<UnresolvedImport>,
    /// Number of external package imports.
    pub external_import_count: u32,
    /// Activity in the latest analysis; absent if the file has no eligible change.
    pub churn: Option<FileChurn>,
}

/// A source file in a snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotFile {
    /// Path at this commit.
    pub path: RepoPath,
    /// Grammar used.
    pub language: Language,
    /// Blob holding the content.
    pub blob: Oid,
    /// Non-blank lines.
    pub loc: u32,
    /// Parse outcome.
    pub parse_status: ParseStatus,
    /// Imports classified as external packages.
    pub external_import_count: u32,
}

/// Everything derived from one commit's tree, ready to persist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotData {
    /// The commit.
    pub commit: Oid,
    /// Source files, sorted by path.
    pub files: Vec<SnapshotFile>,
    /// Resolved edges, one per source, target and type.
    pub edges: Vec<GraphEdge>,
    /// Imports with no edge, with the file containing them.
    pub unresolved: Vec<(RepoPath, UnresolvedImport)>,
    /// What the snapshot covers.
    pub coverage: ParserCoverage,
}
