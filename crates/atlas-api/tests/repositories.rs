//! Drives the repository flow through the router against real Git and SQLite.

use std::path::Path;
use std::time::Duration;

use atlas_api::{AppState, router};
use atlas_fixtures::generate_main_fixture;
use atlas_model::{
    ApiErrorBody, CommitPage, ErrorCode, FileDetail, GraphResponse, JobResponse, JobStatus,
    OverviewResponse, RepositoryEntry,
};
use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use serde::de::DeserializeOwned;
use tempfile::TempDir;
use tower::ServiceExt;

struct Harness {
    app: Router,
    _data: TempDir,
}

impl Harness {
    fn new() -> Self {
        let data = tempfile::tempdir().unwrap();
        let state = AppState::open(&data.path().join("atlas.sqlite"), Default::default()).unwrap();
        Self {
            app: router(state),
            _data: data,
        }
    }

    async fn send(&self, method: Method, uri: &str, body: Option<&str>) -> (StatusCode, Vec<u8>) {
        let mut request = Request::builder()
            .method(method)
            .uri(format!("/api/v1{uri}"))
            .header(header::HOST, "127.0.0.1:4317");
        if body.is_some() {
            request = request.header(header::CONTENT_TYPE, "application/json");
        }
        let request = request
            .body(body.map_or_else(Body::empty, |b| Body::from(b.to_owned())))
            .unwrap();
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, bytes.to_vec())
    }

    async fn json<T: DeserializeOwned>(
        &self,
        method: Method,
        uri: &str,
        body: Option<&str>,
        expected: StatusCode,
    ) -> T {
        let (status, bytes) = self.send(method, uri, body).await;
        assert_eq!(status, expected, "{}", String::from_utf8_lossy(&bytes));
        serde_json::from_slice(&bytes).unwrap()
    }

    async fn open(&self, path: &Path) -> RepositoryEntry {
        let body = serde_json::json!({ "path": path }).to_string();
        self.json(
            Method::POST,
            "/repositories/open",
            Some(&body),
            StatusCode::OK,
        )
        .await
    }

    async fn analyze(&self, id: i64, body: &str) -> JobResponse {
        let started: JobResponse = self
            .json(
                Method::POST,
                &format!("/repositories/{id}/analyze"),
                Some(body),
                StatusCode::ACCEPTED,
            )
            .await;
        for _ in 0..500 {
            let job: JobResponse = self
                .json(
                    Method::GET,
                    &format!("/jobs/{}", started.id),
                    None,
                    StatusCode::OK,
                )
                .await;
            if matches!(job.status, JobStatus::Completed | JobStatus::Failed) {
                return job;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("job did not finish");
    }

    async fn error(
        &self,
        method: Method,
        uri: &str,
        body: Option<&str>,
        status: StatusCode,
    ) -> ErrorCode {
        let body: ApiErrorBody = self.json(method, uri, body, status).await;
        body.error.code
    }
}

#[tokio::test]
async fn open_analyze_and_read_real_results() {
    let repo = tempfile::tempdir().unwrap();
    generate_main_fixture(repo.path()).unwrap();
    let harness = Harness::new();

    let entry = harness.open(repo.path()).await;
    assert_eq!(entry.analyzed_at, None);
    let id = entry.id.0;

    let job = harness.analyze(id, "{}").await;
    assert_eq!(job.status, JobStatus::Completed, "{:?}", job.error);

    let overview: OverviewResponse = harness
        .json(
            Method::GET,
            &format!("/repositories/{id}/overview"),
            None,
            StatusCode::OK,
        )
        .await;
    assert_eq!(overview.history.commit_count, 13);
    assert_eq!(
        overview.top_churn[0].path.as_str(),
        "src/billing/invoice.ts"
    );

    let page: CommitPage = harness
        .json(
            Method::GET,
            &format!("/repositories/{id}/commits?limit=5"),
            None,
            StatusCode::OK,
        )
        .await;
    assert_eq!(page.commits.len(), 5);
    assert_eq!(page.commits[0].position, 12);
    assert_eq!(page.next_cursor, Some(7));

    let next: CommitPage = harness
        .json(
            Method::GET,
            &format!("/repositories/{id}/commits?limit=5&cursor=7"),
            None,
            StatusCode::OK,
        )
        .await;
    assert_eq!(next.commits[0].position, 7);

    let listed: Vec<RepositoryEntry> = harness
        .json(Method::GET, "/repositories", None, StatusCode::OK)
        .await;
    assert_eq!(listed.len(), 1);
    assert!(listed[0].analyzed_at.is_some());
}

#[tokio::test]
async fn opening_twice_returns_the_same_repository() {
    let repo = tempfile::tempdir().unwrap();
    generate_main_fixture(repo.path()).unwrap();
    let harness = Harness::new();
    assert_eq!(
        harness.open(repo.path()).await.id,
        harness.open(repo.path()).await.id
    );
}

#[tokio::test]
async fn invalid_paths_are_rejected() {
    let harness = Harness::new();
    let plain_directory = tempfile::tempdir().unwrap();
    let not_a_repository = serde_json::json!({ "path": plain_directory.path() }).to_string();
    for body in [
        r#"{"path":"relative/path"}"#,
        r#"{"path":"/no/such/directory"}"#,
        not_a_repository.as_str(),
        r#"{"path":1}"#,
        r#"{"path":"/x","extra":true}"#,
        "not json",
    ] {
        let (status, bytes) = harness
            .send(Method::POST, "/repositories/open", Some(body))
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        let error: ApiErrorBody = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(error.error.code, ErrorCode::InvalidInput, "{body}");
    }
}

#[tokio::test]
async fn opening_requires_a_json_content_type() {
    let harness = Harness::new();
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/repositories/open")
        .header(header::HOST, "127.0.0.1:4317")
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from(r#"{"path":"/tmp"}"#))
        .unwrap();
    let response = harness.app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn unknown_ids_are_not_found() {
    let harness = Harness::new();
    for uri in [
        "/repositories/99/overview",
        "/repositories/99/commits",
        "/jobs/99",
    ] {
        let code = harness
            .error(Method::GET, uri, None, StatusCode::NOT_FOUND)
            .await;
        assert_eq!(code, ErrorCode::NotFound, "{uri}");
    }
    let code = harness
        .error(
            Method::POST,
            "/repositories/99/analyze",
            Some("{}"),
            StatusCode::NOT_FOUND,
        )
        .await;
    assert_eq!(code, ErrorCode::NotFound);
    let code = harness
        .error(
            Method::GET,
            "/repositories/abc/overview",
            None,
            StatusCode::BAD_REQUEST,
        )
        .await;
    assert_eq!(code, ErrorCode::InvalidInput);
}

#[tokio::test]
async fn overview_before_analysis_is_not_found() {
    let repo = tempfile::tempdir().unwrap();
    generate_main_fixture(repo.path()).unwrap();
    let harness = Harness::new();
    let id = harness.open(repo.path()).await.id.0;
    let code = harness
        .error(
            Method::GET,
            &format!("/repositories/{id}/overview"),
            None,
            StatusCode::NOT_FOUND,
        )
        .await;
    assert_eq!(code, ErrorCode::NotFound);
}

#[tokio::test]
async fn a_bad_reference_fails_the_job_with_a_reason() {
    let repo = tempfile::tempdir().unwrap();
    generate_main_fixture(repo.path()).unwrap();
    let harness = Harness::new();
    let id = harness.open(repo.path()).await.id.0;

    let job = harness.analyze(id, r#"{"git_ref":"no-such-branch"}"#).await;
    assert_eq!(job.status, JobStatus::Failed);
    assert!(job.error.unwrap().contains("no-such-branch"));
}

#[tokio::test]
async fn concurrent_jobs_for_one_repository_leave_one_consistent_result() {
    let repo = tempfile::tempdir().unwrap();
    generate_main_fixture(repo.path()).unwrap();
    let harness = Harness::new();
    let id = harness.open(repo.path()).await.id.0;

    let jobs = tokio::join!(
        harness.analyze(id, "{}"),
        harness.analyze(id, "{}"),
        harness.analyze(id, "{}"),
        harness.analyze(id, "{}"),
    );
    let finished = [jobs.0, jobs.1, jobs.2, jobs.3];
    assert!(
        finished
            .iter()
            .any(|job| job.status == JobStatus::Completed)
    );

    let overview: OverviewResponse = harness
        .json(
            Method::GET,
            &format!("/repositories/{id}/overview"),
            None,
            StatusCode::OK,
        )
        .await;
    assert_eq!(overview.history.commit_count, 13);
}

#[tokio::test]
async fn oversized_bodies_are_rejected() {
    let harness = Harness::new();
    let body = format!(r#"{{"path":"/{}"}}"#, "a".repeat(100_000));
    let (status, _) = harness
        .send(Method::POST, "/repositories/open", Some(&body))
        .await;
    assert!(status.is_client_error(), "{status}");
    assert_ne!(status, StatusCode::OK);
}

#[tokio::test]
async fn graph_and_file_routes_serve_real_snapshots() {
    let repo = tempfile::tempdir().unwrap();
    generate_main_fixture(repo.path()).unwrap();
    let harness = Harness::new();
    let id = harness.open(repo.path()).await.id.0;
    harness.analyze(id, "{}").await;

    let graph: GraphResponse = harness
        .json(
            Method::GET,
            &format!("/repositories/{id}/graph?limit=3&directory=src"),
            None,
            StatusCode::OK,
        )
        .await;
    assert_eq!(graph.nodes.len(), 3);
    assert_eq!(graph.total_edge_count, 4);

    let uri = format!("/repositories/{id}/files?path=src%2Fm%C3%B3dulo%20uno%2F%C3%ADndice.ts");
    let detail: FileDetail = harness.json(Method::GET, &uri, None, StatusCode::OK).await;
    assert_eq!(detail.node.path.as_str(), "src/módulo uno/índice.ts");
}

#[tokio::test]
async fn traversal_in_a_file_path_is_rejected() {
    let repo = tempfile::tempdir().unwrap();
    generate_main_fixture(repo.path()).unwrap();
    let harness = Harness::new();
    let id = harness.open(repo.path()).await.id.0;
    harness.analyze(id, "{}").await;

    let uri = format!("/repositories/{id}/files?path=..%2F..%2Fetc%2Fpasswd");
    let code = harness
        .error(Method::GET, &uri, None, StatusCode::BAD_REQUEST)
        .await;
    assert_eq!(code, ErrorCode::InvalidInput);
}

#[tokio::test]
async fn the_built_dashboard_is_served_with_a_content_security_policy() {
    let web = tempfile::tempdir().unwrap();
    std::fs::write(web.path().join("index.html"), "<title>dash</title>").unwrap();
    let data = tempfile::tempdir().unwrap();
    let state = AppState::open(&data.path().join("atlas.sqlite"), Default::default())
        .unwrap()
        .with_web_dir(web.path().to_path_buf());

    let request = Request::builder()
        .uri("/")
        .header(header::HOST, "127.0.0.1:4317")
        .body(Body::empty())
        .unwrap();
    let response = router(state.clone()).oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response
            .headers()
            .contains_key(header::CONTENT_SECURITY_POLICY)
    );

    let request = Request::builder()
        .uri("/../../etc/passwd")
        .header(header::HOST, "127.0.0.1:4317")
        .body(Body::empty())
        .unwrap();
    let response = router(state.clone()).oneshot(request).await.unwrap();
    assert_ne!(response.status(), StatusCode::OK);

    let request = Request::builder()
        .uri("/api/v1/nope")
        .header(header::HOST, "127.0.0.1:4317")
        .body(Body::empty())
        .unwrap();
    let response = router(state).oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let error: ApiErrorBody = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(error.error.code, ErrorCode::NotFound);
}
