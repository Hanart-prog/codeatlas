//! Types shared by every CodeAtlas crate.
//!
//! This crate has no knowledge of Git, SQLite or HTTP, which keeps the engine
//! usable from the CLI and the API alike.

pub mod config;
pub mod dto;
pub mod error;
pub mod graph;
pub mod history;
pub mod ids;
pub mod impact;

pub use config::{AnalysisConfig, AtlasConfig, Limits, ServerConfig, StorageConfig};
pub use dto::{
    AnalysisStatus, AnalyzeRequest, ApiErrorBody, ApiErrorDetail, CommitEntry, CommitPage,
    CommitSummary, DatabaseHealth, DependencyEdgeType, ExclusionCount, FileChurn, FileSummary,
    HealthResponse, HealthStatus, HistorySummary, JobResponse, JobStatus, OpenRepositoryRequest,
    OverviewResponse, RepositoryEntry, RunSummary,
};
pub use error::{AtlasError, ErrorCode};
pub use graph::{
    Cycle, FileDetail, GraphEdge, GraphNode, GraphResponse, ImportRecord, Language, ParseStatus,
    ParsedFile, ParserCoverage, ResolutionKind, SnapshotData, SnapshotFile, TsconfigStatus,
    UnresolvedCount, UnresolvedImport, UnresolvedReason,
};
pub use history::{ChangeStatus, ExclusionReason, IndexedHistory};
pub use ids::{AnalysisRunId, Oid, RepoPath, RepositoryId};
pub use impact::{
    CoChangeEdge, CoChangeStats, EvidenceKind, HistoricalEvidence, ImpactRecommendation,
    ImpactRequest, ImpactResponse, SelectedFile, SelectedFileStatus, StaticEvidence,
};

/// Version of the application, reported by the CLI and `/health`.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default number of first-parent commits to index.
pub const DEFAULT_HISTORY_LIMIT: u32 = 500;

/// Version of the HTTP API contract, which is also its path prefix.
pub const API_VERSION: &str = "v1";

/// Version of import extraction. Changing it invalidates cached snapshots.
pub const PARSER_VERSION: u32 = 1;

/// Version of the analysis algorithms. Changing it invalidates cached analyses.
pub const ALGORITHM_VERSION: u32 = 3;
