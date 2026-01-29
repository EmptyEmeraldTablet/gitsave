use thiserror::Error;

#[derive(Debug, Error)]
pub enum SaveError {
    #[error("Repository error: {0}")]
    Repository(#[from] git2::Error),

    #[error("Save not found: {0}")]
    SaveNotFound(String),

    #[error("Route not found: {0}")]
    RouteNotFound(String),

    #[error("Uncommitted changes. Save first or use --force")]
    UncommittedChanges,

    #[error("Corrupted data: {0}")]
    CorruptedData(String),

    #[error("Permission denied")]
    PermissionDenied,

    #[error("File too large: {size}KB > {limit}KB")]
    FileTooLarge { size: u64, limit: u64 },

    #[error("Not a gitsave repository")]
    NotRepository,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Config error: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, SaveError>;
