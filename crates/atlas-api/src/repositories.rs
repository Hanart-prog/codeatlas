//! Repository routes. All analysis happens in `atlas-analysis`.

use std::path::{Path as FsPath, PathBuf};

use atlas_analysis::{
    AnalyzeOptions, DEFAULT_TIMELINE_POINTS, GraphQuery, compare as diff_commits, file_detail,
    graph as load_graph, impact as rank_impact, open_repository, overview as load_overview,
    timeline as load_timeline,
};
use atlas_model::{
    AnalyzeRequest, AtlasError, CommitPage, CompareRequest, CompareResponse, DEFAULT_HISTORY_LIMIT,
    FileDetail, GraphResponse, ImpactRequest, ImpactResponse, JobResponse, OpenRepositoryRequest,
    OverviewResponse, RepoPath, RepositoryEntry, RepositoryId, TimelineResponse,
};
use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;

use crate::{ApiError, AppState};

const DEFAULT_PAGE_SIZE: u32 = 50;
const MAX_PAGE_SIZE: u32 = 200;
const DEFAULT_GRAPH_NODES: u32 = 500;

fn parse_id(raw: &str) -> Result<RepositoryId, ApiError> {
    raw.parse()
        .map(RepositoryId)
        .map_err(|_| ApiError::invalid("repository id must be an integer"))
}

fn json_body<T>(body: Result<Json<T>, JsonRejection>) -> Result<T, ApiError> {
    body.map(|Json(value)| value)
        .map_err(|rejection| ApiError::invalid(rejection.body_text()))
}

fn found(entry: Option<RepositoryEntry>) -> Result<RepositoryEntry, AtlasError> {
    entry.ok_or_else(|| AtlasError::NotFound("no such repository".to_owned()))
}

/// `GET /repositories`
pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<RepositoryEntry>>, ApiError> {
    let entries = state.read(|db| Ok(db.repositories()?)).await?;
    Ok(Json(entries))
}

/// `POST /repositories/open`: validates and registers an explicit local path.
pub async fn open(
    State(state): State<AppState>,
    body: Result<Json<OpenRepositoryRequest>, JsonRejection>,
) -> Result<Json<RepositoryEntry>, ApiError> {
    let request = json_body(body)?;
    let path = PathBuf::from(request.path);
    if !path.is_absolute() {
        return Err(ApiError::invalid("repository path must be absolute"));
    }
    let entry = state.read(move |db| open_repository(db, &path)).await?;
    Ok(Json(entry))
}

/// `POST /repositories/:id/analyze`: starts a background job.
pub async fn analyze(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Result<Json<AnalyzeRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<JobResponse>), ApiError> {
    let id = parse_id(&id)?;
    let request = json_body(body)?;
    let repository = state.read(move |db| found(db.repository(id)?)).await?;

    let options = AnalyzeOptions {
        git_ref: request.git_ref.unwrap_or_else(|| "HEAD".to_owned()),
        history_limit: request.history.unwrap_or(DEFAULT_HISTORY_LIMIT),
    };
    let job = crate::jobs::start(
        &state,
        id,
        FsPath::new(&repository.path).to_path_buf(),
        options,
    );
    Ok((StatusCode::ACCEPTED, Json(job)))
}

/// `GET /repositories/:id/overview`
pub async fn overview(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<OverviewResponse>, ApiError> {
    let id = parse_id(&id)?;
    let overview = state.read(move |db| load_overview(db, id)).await?;
    Ok(Json(overview))
}

#[derive(Debug, Deserialize)]
pub struct PageQuery {
    limit: Option<u32>,
    cursor: Option<u32>,
}

/// `GET /repositories/:id/commits?limit=&cursor=`: newest first.
pub async fn commits(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(page): Query<PageQuery>,
) -> Result<Json<CommitPage>, ApiError> {
    let id = parse_id(&id)?;
    let limit = page
        .limit
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);
    let result = state
        .read(move |db| {
            let run = db
                .latest_run(id)?
                .ok_or_else(|| AtlasError::NotFound("repository not analyzed yet".to_owned()))?;
            Ok(db.commit_page(run.id, page.cursor, limit)?)
        })
        .await?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
pub struct GraphParams {
    #[serde(rename = "ref")]
    git_ref: Option<String>,
    directory: Option<String>,
    limit: Option<u32>,
    include: Option<String>,
}

/// `GET /repositories/:id/graph?ref=&directory=&limit=`
pub async fn graph(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<GraphParams>,
) -> Result<Json<GraphResponse>, ApiError> {
    let id = parse_id(&id)?;
    let result = state
        .write(move |db, config| {
            let repository = found(db.repository(id)?)?;
            let query = GraphQuery {
                git_ref: params.git_ref,
                directory: params.directory,
                limit: params
                    .limit
                    .unwrap_or(DEFAULT_GRAPH_NODES)
                    .clamp(1, config.limits.max_graph_nodes),
                include_cochange: params.include.as_deref() == Some("cochange"),
            };
            load_graph(db, &repository, &query, config)
        })
        .await?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
pub struct FileParams {
    path: String,
    #[serde(rename = "ref")]
    git_ref: Option<String>,
}

/// `GET /repositories/:id/files?path=&ref=`. The path is a query parameter, so no
/// route has to decode a repository path out of its URL segments.
pub async fn file(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<FileParams>,
) -> Result<Json<FileDetail>, ApiError> {
    let id = parse_id(&id)?;
    let path: RepoPath = params.path.parse().map_err(ApiError::from)?;
    let result = state
        .write(move |db, config| {
            let repository = found(db.repository(id)?)?;
            file_detail(db, &repository, params.git_ref, &path, config)
        })
        .await?;
    Ok(Json(result))
}

/// `POST /repositories/:id/impact`
pub async fn impact(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Result<Json<ImpactRequest>, JsonRejection>,
) -> Result<Json<ImpactResponse>, ApiError> {
    let id = parse_id(&id)?;
    let request = json_body(body)?;
    let result = state
        .write(move |db, config| {
            let repository = found(db.repository(id)?)?;
            rank_impact(db, &repository, &request, config)
        })
        .await?;
    Ok(Json(result))
}

/// `POST /repositories/:id/compare`
pub async fn compare(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Result<Json<CompareRequest>, JsonRejection>,
) -> Result<Json<CompareResponse>, ApiError> {
    let id = parse_id(&id)?;
    let request = json_body(body)?;
    let result = state
        .write(move |db, config| {
            let repository = found(db.repository(id)?)?;
            diff_commits(db, &repository, &request, config)
        })
        .await?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
pub struct TimelineParams {
    points: Option<u32>,
}

/// `GET /repositories/:id/timeline?points=`
pub async fn timeline(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<TimelineParams>,
) -> Result<Json<TimelineResponse>, ApiError> {
    let id = parse_id(&id)?;
    let points = params.points.unwrap_or(DEFAULT_TIMELINE_POINTS);
    let result = state
        .write(move |db, config| {
            let repository = found(db.repository(id)?)?;
            load_timeline(db, &repository, points, config)
        })
        .await?;
    Ok(Json(result))
}
