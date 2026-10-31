use atlas_model::{
    ALGORITHM_VERSION, API_VERSION, APP_VERSION, DatabaseHealth, HealthResponse, HealthStatus,
    PARSER_VERSION,
};
use atlas_storage::latest_version;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

use crate::AppState;

/// `GET /api/v1/health`: readiness and version. Returns 503 when the database
/// cannot be queried or is not at the schema this build expects.
pub async fn health(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    let schema_version = state
        .read(|database| Ok(database.schema_version().ok()))
        .await
        .ok()
        .flatten();

    let expected_schema_version = latest_version();
    let (status, code) = if schema_version == Some(expected_schema_version) {
        (HealthStatus::Ok, StatusCode::OK)
    } else {
        (HealthStatus::Degraded, StatusCode::SERVICE_UNAVAILABLE)
    };

    let body = HealthResponse {
        status,
        version: APP_VERSION.to_owned(),
        api_version: API_VERSION.to_owned(),
        parser_version: PARSER_VERSION,
        algorithm_version: ALGORITHM_VERSION,
        database: DatabaseHealth {
            reachable: schema_version.is_some(),
            schema_version,
            expected_schema_version,
        },
    };
    (code, Json(body))
}

#[cfg(test)]
mod tests {
    use atlas_model::{APP_VERSION, HealthResponse, HealthStatus};
    use atlas_storage::latest_version;
    use axum::body::Body;
    use axum::http::{StatusCode, header};

    use crate::test_support::{app, json, request, send};

    #[tokio::test]
    async fn reports_readiness_and_versions() {
        let (app, _) = app();
        let response = send(app, request("/api/v1/health").body(Body::empty()).unwrap()).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
        let body: HealthResponse = serde_json::from_value(json(response).await).unwrap();
        assert_eq!(body.status, HealthStatus::Ok);
        assert_eq!(body.version, APP_VERSION);
        assert_eq!(body.api_version, "v1");
        assert!(body.database.reachable);
        assert_eq!(body.database.schema_version, Some(latest_version()));
    }

    #[tokio::test]
    async fn reports_degraded_when_the_database_is_unusable() {
        let (app, state) = app();
        state
            .database
            .lock()
            .unwrap()
            .connection()
            .execute("DROP TABLE schema_migrations", [])
            .unwrap();

        let response = send(app, request("/api/v1/health").body(Body::empty()).unwrap()).await;

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body: HealthResponse = serde_json::from_value(json(response).await).unwrap();
        assert_eq!(body.status, HealthStatus::Degraded);
        assert!(!body.database.reachable);
        assert_eq!(body.database.schema_version, None);
    }
}
