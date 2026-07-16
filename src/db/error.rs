//! Error types for the database module.

use thiserror::Error;

/// Errors that can occur during database operations.
// Wraps rusqlite::Error to insulate callers from rusqlite internals and allow swapping the DB backend later.
#[derive(Debug, Error)]
pub enum DbError {
    #[error("database {0} mutex was poisoned")]
    MutexPoisoned(&'static str),

    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    // A recognized schema-migration failure. Distinct from generic Sqlite errors
    // so startup can route it to the recoverable-migration flow (04 §12) rather
    // than the generic closing dialog.
    #[error("migration failed: {0}")]
    Migration(String),

    // Separated from generic Sqlite errors because it signals data corruption rather than I/O failure, requiring different handling.
    #[allow(dead_code)]
    #[error("invalid media type stored in database: '{0}'")]
    InvalidMediaType(String),
}
