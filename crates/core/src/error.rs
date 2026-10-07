use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("lookup failed: {0}")]
    Lookup(String),
    #[error("credential store: {0}")]
    Secret(String),
}

pub type Result<T> = std::result::Result<T, Error>;
