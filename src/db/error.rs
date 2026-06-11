//! Error types for the database module.

use thiserror::Error;

/// Errors that can occur during database operations.
#[derive(Debug, Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("invalid media type stored in database: '{0}'")]
    InvalidMediaType(String),
}
