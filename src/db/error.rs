//! Error types for the database module.

use thiserror::Error;

/// Errors that can occur during database operations.
// Wraps rusqlite::Error to insulate callers from rusqlite internals and allow swapping the DB backend later.
#[derive(Debug, Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    // Separated from generic Sqlite errors because it signals data corruption rather than I/O failure, requiring different handling.
    #[error("invalid media type stored in database: '{0}'")]
    InvalidMediaType(String),
}
