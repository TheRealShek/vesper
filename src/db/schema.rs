//! Database schema initialization.
//!
//! All tables and indexes are created through the versioned, transactional
//! migration runner in [`super::migrations`]; this module only configures
//! connection PRAGMAs and runs the startup orphan cleanup after migrations.

use rusqlite::Connection;

use super::error::DbError;
use super::migrations;

/// Configures connection PRAGMAs, runs pending migrations, then cleans up
/// orphaned rows. A migration failure is fatal and surfaces as
/// [`DbError::Migration`]; the caller must not proceed with a partial schema.
pub fn initialize(conn: &mut Connection) -> Result<(), DbError> {
    conn.execute_batch(PRAGMAS)?;
    migrations::run(conn)?;
    conn.execute(
        "DELETE FROM media WHERE source_root_id NOT IN (SELECT id FROM source_roots)",
        [],
    )?;
    conn.execute(
        "DELETE FROM tags WHERE id NOT IN (SELECT DISTINCT tag_id FROM media_tags)",
        [],
    )?;
    Ok(())
}

const PRAGMAS: &str = "\
    PRAGMA journal_mode = WAL;\n\
    PRAGMA foreign_keys = ON;\n\
    PRAGMA busy_timeout = 5000;\n";
