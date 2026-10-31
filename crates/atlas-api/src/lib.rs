//! The local HTTP API. All analysis lives in the engine crates; this crate only
//! translates between HTTP and those crates.

mod error;
mod health;
mod jobs;
mod repositories;
mod request_guard;

use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use atlas_model::{AtlasConfig, AtlasError, ErrorCode};
use atlas_storage::Database;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::routing::{get, post};
use axum::{Router, middleware};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

pub use error::ApiError;

/// Origins of the Vite dev server, the only cross-origin callers allowed.
pub(crate) const DEV_ORIGINS: [&str; 2] = ["http://localhost:5173", "http://127.0.0.1:5173"];

/// Largest accepted request body. Requests only ever carry paths and settings.
const MAX_BODY_BYTES: usize = 64 * 1024;

/// Analyses that may run at once; more wait in the queue.
const MAX_CONCURRENT_ANALYSES: usize = 2;

/// Dashboard pages show untrusted repository text, so even an escaping bug must
/// not be able to run a script or reach another site.
const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; style-src 'self' 'unsafe-inline'; \
     img-src 'self' data:; object-src 'none'; base-uri 'none'; frame-ancestors 'none'";

/// State shared by request handlers.
#[derive(Debug, Clone)]
pub struct AppState {
    database: Arc<Mutex<Database>>,
    database_path: PathBuf,
    config: Arc<AtlasConfig>,
    jobs: Arc<jobs::JobRegistry>,
    analysis_slots: Arc<Semaphore>,
    web_dir: Option<PathBuf>,
}

impl AppState {
    /// Opens the database at `database_path`. Background jobs open their own
    /// connections to the same file.
    pub fn open(database_path: &Path, config: AtlasConfig) -> Result<Self, AtlasError> {
        let database = Database::open(database_path)?;
        Ok(Self::new(database, database_path.to_path_buf(), config))
    }

    fn new(database: Database, database_path: PathBuf, config: AtlasConfig) -> Self {
        Self {
            database: Arc::new(Mutex::new(database)),
            database_path,
            config: Arc::new(config),
            jobs: Arc::default(),
            analysis_slots: Arc::new(Semaphore::new(MAX_CONCURRENT_ANALYSES)),
            web_dir: None,
        }
    }

    /// Serves the built dashboard from `directory` alongside the API.
    pub fn with_web_dir(mut self, directory: PathBuf) -> Self {
        self.web_dir = Some(directory);
        self
    }

    /// Runs work that may write, on its own connection, so that it does not hold
    /// up other requests.
    async fn write<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut Database, &AtlasConfig) -> Result<T, AtlasError> + Send + 'static,
    ) -> Result<T, ApiError> {
        let path = self.database_path.clone();
        let config = Arc::clone(&self.config);
        let task = tokio::task::spawn_blocking(move || {
            let mut database = Database::open(&path)?;
            work(&mut database, &config)
        });
        match task.await {
            Ok(result) => result.map_err(ApiError::from),
            Err(_) => Err(ApiError::internal("query task failed")),
        }
    }

    /// Runs a query off the async runtime.
    async fn read<T: Send + 'static>(
        &self,
        query: impl FnOnce(&Database) -> Result<T, AtlasError> + Send + 'static,
    ) -> Result<T, ApiError> {
        let database = Arc::clone(&self.database);
        let task = tokio::task::spawn_blocking(move || {
            let database = database
                .lock()
                .map_err(|_| AtlasError::Storage("database lock poisoned".to_owned()))?;
            query(&database)
        });
        match task.await {
            Ok(result) => result.map_err(ApiError::from),
            Err(_) => Err(ApiError::internal("query task failed")),
        }
    }
}

/// Builds the application router.
pub fn router(state: AppState) -> Router {
    let v1 = Router::new()
        .route("/health", get(health::health))
        .route("/repositories", get(repositories::list))
        .route("/repositories/open", post(repositories::open))
        .route("/repositories/{id}/analyze", post(repositories::analyze))
        .route("/repositories/{id}/overview", get(repositories::overview))
        .route("/repositories/{id}/commits", get(repositories::commits))
        .route("/repositories/{id}/graph", get(repositories::graph))
        .route("/repositories/{id}/files", get(repositories::file))
        .route("/repositories/{id}/impact", post(repositories::impact))
        .route("/repositories/{id}/compare", post(repositories::compare))
        .route("/repositories/{id}/timeline", get(repositories::timeline))
        .route("/jobs/{id}", get(jobs::get_job))
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
        .with_state(state.clone());

    let routes = Router::new().nest("/api/v1", v1);
    let routes = match state.web_dir {
        Some(directory) => routes.fallback_service(ServeDir::new(directory)),
        None => routes.fallback(not_found),
    };

    routes
        .layer(SetResponseHeaderLayer::overriding(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(CONTENT_SECURITY_POLICY),
        ))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(middleware::from_fn(request_guard::require_local_request))
        .layer(cors())
        .layer(TraceLayer::new_for_http())
}

/// Serves `state` on `listener` until `shutdown` resolves.
pub async fn serve(
    listener: TcpListener,
    state: AppState,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> io::Result<()> {
    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown)
        .await
}

fn cors() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(DEV_ORIGINS.map(HeaderValue::from_static))
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::CONTENT_TYPE])
}

async fn not_found() -> ApiError {
    ApiError::new(StatusCode::NOT_FOUND, ErrorCode::NotFound, "no such route")
}

async fn method_not_allowed() -> ApiError {
    ApiError::new(
        StatusCode::METHOD_NOT_ALLOWED,
        ErrorCode::MethodNotAllowed,
        "method not allowed for this route",
    )
}

#[cfg(test)]
pub(crate) mod test_support {
    use atlas_model::ApiErrorBody;
    use atlas_storage::Database;
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, Response, header};
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tower::ServiceExt;

    use crate::{AppState, router};

    pub fn app() -> (Router, AppState) {
        let database = Database::open_in_memory().unwrap();
        let state = AppState::new(database, ":memory:".into(), Default::default());
        (router(state.clone()), state)
    }

    pub fn request(uri: &str) -> axum::http::request::Builder {
        Request::builder()
            .uri(uri)
            .header(header::HOST, "127.0.0.1:4317")
    }

    pub async fn send(app: Router, request: Request<Body>) -> Response<Body> {
        app.oneshot(request).await.unwrap()
    }

    pub async fn json(response: Response<Body>) -> Value {
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    pub async fn error_body(response: Response<Body>) -> ApiErrorBody {
        serde_json::from_value(json(response).await).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use atlas_model::ErrorCode;
    use axum::body::Body;
    use axum::http::{Method, StatusCode, header};

    use crate::test_support::{app, error_body, request, send};

    #[tokio::test]
    async fn unknown_routes_use_the_error_envelope() {
        for uri in ["/api/v1/nope", "/nope"] {
            let (app, _) = app();
            let response = send(app, request(uri).body(Body::empty()).unwrap()).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
            assert_eq!(error_body(response).await.error.code, ErrorCode::NotFound);
        }
    }

    #[tokio::test]
    async fn wrong_methods_use_the_error_envelope() {
        let (app, _) = app();
        let req = request("/api/v1/health")
            .method(Method::POST)
            .body(Body::empty())
            .unwrap();
        let response = send(app, req).await;
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(
            error_body(response).await.error.code,
            ErrorCode::MethodNotAllowed
        );
    }

    #[tokio::test]
    async fn cors_allows_the_dev_server_only() {
        for (origin, allowed) in [
            ("http://localhost:5173", true),
            ("http://127.0.0.1:5173", true),
            // Same-origin passes the guard but needs no CORS grant.
            ("http://127.0.0.1:4317", false),
        ] {
            let (app, _) = app();
            let req = request("/api/v1/health")
                .header(header::ORIGIN, origin)
                .body(Body::empty())
                .unwrap();
            let response = send(app, req).await;
            let header = response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN);
            assert_eq!(header.is_some(), allowed, "{origin}");
        }
    }
}
