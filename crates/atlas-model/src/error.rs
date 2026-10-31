//! The application error model. `ErrorCode` is the stable, machine-readable part
//! that crosses the API boundary; messages are for people.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

/// Stable error codes returned in API error bodies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The request or configuration failed validation.
    InvalidInput,
    /// The requested route or resource does not exist.
    NotFound,
    /// The route exists but does not accept this method.
    MethodNotAllowed,
    /// The request's `Host` header does not name this machine.
    ForbiddenHost,
    /// The request came from a web page this server does not trust.
    ForbiddenOrigin,
    /// The feature is planned but not built yet.
    NotImplemented,
    /// Git could not read the repository.
    GitError,
    /// SQLite could not serve the request.
    StorageError,
    /// An unexpected internal failure.
    Internal,
}

/// Errors shared across the workspace.
#[derive(Debug, Error)]
pub enum AtlasError {
    /// Input failed validation.
    #[error("{0}")]
    InvalidInput(String),
    /// The configuration file or a setting is invalid.
    #[error("invalid configuration: {0}")]
    Config(String),
    /// The feature is planned for a later phase.
    #[error("{feature} is not implemented yet (planned for Phase {phase})")]
    NotImplemented {
        /// What the user tried to use.
        feature: &'static str,
        /// The specification phase that delivers it.
        phase: u8,
    },
    /// The requested repository, commit or run does not exist.
    #[error("{0}")]
    NotFound(String),
    /// Git could not read the repository.
    #[error("git error: {0}")]
    Git(String),
    /// A storage failure.
    #[error("storage error: {0}")]
    Storage(String),
    /// A filesystem or network failure.
    #[error("{context}: {source}")]
    Io {
        /// What was being attempted.
        context: String,
        /// The underlying failure.
        #[source]
        source: std::io::Error,
    },
}

impl AtlasError {
    /// The stable code for this error.
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::InvalidInput(_) | Self::Config(_) => ErrorCode::InvalidInput,
            Self::NotImplemented { .. } => ErrorCode::NotImplemented,
            Self::NotFound(_) => ErrorCode::NotFound,
            Self::Git(_) => ErrorCode::GitError,
            Self::Storage(_) => ErrorCode::StorageError,
            Self::Io { .. } => ErrorCode::Internal,
        }
    }
}
