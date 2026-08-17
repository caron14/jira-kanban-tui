use thiserror::Error;

#[derive(Debug, Error)]
pub enum JiraError {
    #[error("authentication failed: {0}")]
    Authentication(String),

    #[error("permission denied or not found: {0}")]
    PermissionOrNotFound(String),

    #[error("rate limited, retry after {retry_after:?}: {message}")]
    RateLimited { retry_after: Option<u64>, message: String },

    #[error("timeout or offline: {0}")]
    TimeoutOrOffline(String),

    #[error("validation/conflict: {0}")]
    Validation(String),

    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("unsupported capability: {0}")]
    UnsupportedCapability(String),

    #[error("other: {0}")]
    Other(String),
}
