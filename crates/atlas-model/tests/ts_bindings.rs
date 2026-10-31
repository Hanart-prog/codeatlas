//! Keeps the frontend's API types in step with the Rust DTOs. Run with
//! `UPDATE_BINDINGS=1` (`make types`) to regenerate after changing a DTO.

use std::fs;
use std::path::PathBuf;

use atlas_model::{
    AnalysisRunId, AnalysisStatus, AnalyzeRequest, ApiErrorBody, ApiErrorDetail, CommitEntry,
    CommitPage, CommitSummary, DatabaseHealth, DependencyEdgeType, ErrorCode, ExclusionCount,
    ExclusionReason, FileChurn, FileSummary, HealthResponse, HealthStatus, HistorySummary,
    JobResponse, JobStatus, Oid, OpenRepositoryRequest, OverviewResponse, RepoPath,
    RepositoryEntry, RepositoryId, RunSummary,
};
use atlas_model::{
    Cycle, FileDetail, GraphEdge, GraphNode, GraphResponse, Language, ParseStatus, ParserCoverage,
    ResolutionKind, TsconfigStatus, UnresolvedCount, UnresolvedImport, UnresolvedReason,
};
use ts_rs::TS;

fn binding<T: TS + 'static>() -> (String, String) {
    (
        format!("{}.ts", T::name(&Default::default())),
        T::export_to_string(&Default::default()).unwrap(),
    )
}

#[test]
fn generated_typescript_is_up_to_date() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/src/types/generated")
        .canonicalize()
        .expect("apps/web/src/types/generated must exist");
    let update = std::env::var_os("UPDATE_BINDINGS").is_some();

    let bindings = [
        binding::<AnalysisRunId>(),
        binding::<AnalysisStatus>(),
        binding::<AnalyzeRequest>(),
        binding::<ApiErrorBody>(),
        binding::<ApiErrorDetail>(),
        binding::<DatabaseHealth>(),
        binding::<DependencyEdgeType>(),
        binding::<ErrorCode>(),
        binding::<HealthResponse>(),
        binding::<HealthStatus>(),
        binding::<CommitEntry>(),
        binding::<CommitPage>(),
        binding::<CommitSummary>(),
        binding::<ExclusionCount>(),
        binding::<ExclusionReason>(),
        binding::<FileChurn>(),
        binding::<FileSummary>(),
        binding::<HistorySummary>(),
        binding::<JobResponse>(),
        binding::<JobStatus>(),
        binding::<OpenRepositoryRequest>(),
        binding::<OverviewResponse>(),
        binding::<Cycle>(),
        binding::<FileDetail>(),
        binding::<GraphEdge>(),
        binding::<GraphNode>(),
        binding::<GraphResponse>(),
        binding::<Language>(),
        binding::<ParseStatus>(),
        binding::<ParserCoverage>(),
        binding::<ResolutionKind>(),
        binding::<TsconfigStatus>(),
        binding::<UnresolvedCount>(),
        binding::<UnresolvedImport>(),
        binding::<UnresolvedReason>(),
        binding::<Oid>(),
        binding::<RepoPath>(),
        binding::<RepositoryEntry>(),
        binding::<RepositoryId>(),
        binding::<RunSummary>(),
    ];

    for (file_name, contents) in bindings {
        let path = directory.join(&file_name);
        if update {
            fs::write(&path, contents).unwrap();
        } else {
            let committed = fs::read_to_string(&path).unwrap_or_default();
            assert_eq!(
                committed, contents,
                "{file_name} is stale; run `make types`"
            );
        }
    }
}
