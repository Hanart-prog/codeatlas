use atlas_model::{ApiErrorBody, ApiErrorDetail, AtlasError, ErrorCode};
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// An error rendered as the API's standard JSON envelope.
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: ErrorCode,
    message: String,
}

impl ApiError {
    /// Builds an error from its parts.
    pub fn new(status: StatusCode, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }
}

impl ApiError {
    /// An unexpected internal failure.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::Internal,
            message,
        )
    }

    /// A request that failed validation.
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, ErrorCode::InvalidInput, message)
    }
}

impl From<AtlasError> for ApiError {
    fn from(error: AtlasError) -> Self {
        let code = error.code();
        let status = match code {
            ErrorCode::InvalidInput => StatusCode::BAD_REQUEST,
            ErrorCode::NotFound => StatusCode::NOT_FOUND,
            ErrorCode::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,
            ErrorCode::ForbiddenHost | ErrorCode::ForbiddenOrigin => StatusCode::FORBIDDEN,
            ErrorCode::NotImplemented => StatusCode::NOT_IMPLEMENTED,
            ErrorCode::GitError | ErrorCode::StorageError | ErrorCode::Internal => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        Self::new(status, code, error.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ApiErrorBody {
            error: ApiErrorDetail {
                code: self.code,
                message: self.message,
                details: None,
            },
        };
        (self.status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_errors_map_to_http_statuses() {
        let cases = [
            (
                AtlasError::InvalidInput("bad".to_owned()),
                StatusCode::BAD_REQUEST,
            ),
            (
                AtlasError::NotImplemented {
                    feature: "x",
                    phase: 1,
                },
                StatusCode::NOT_IMPLEMENTED,
            ),
            (
                AtlasError::Storage("down".to_owned()),
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        ];
        for (error, status) in cases {
            assert_eq!(ApiError::from(error).status, status);
        }
    }
}
