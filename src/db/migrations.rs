//! Transactional schema migration runner (A-1).
//!
//! All schema changes go through explicit, versioned migrations recorded in the
//! `schema_migrations` table. Startup must never rely on best-effort
//! `ALTER TABLE` statements that silently ignore failure.
//!
//! Each pending migration runs in its own transaction: on any failure the
//! transaction is rolled back and the error is returned as
//! [`DbError::Migration`], which is fatal. The database is therefore always
//! left at the schema of the last fully-applied migration — never a partial
//! one — and callers must not fall back to ad-hoc schema creation.

use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension};

use super::error::DbError;

/// A single, ordered schema migration.
struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
}

/// The ordered list of migrations. Append new migrations with the next version;
/// never edit or reorder an existing entry once it has shipped.
const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "initial_schema",
        sql: INITIAL_SCHEMA,
    },
    Migration {
        version: 2,
        name: "path_qualified_tags",
        sql: PATH_QUALIFIED_TAGS,
    },
];

/// Applies all pending migrations in order, each in its own transaction.
///
/// Returns [`DbError::Migration`] on the first failure without applying any
/// later migration; the failed migration's transaction is rolled back.
pub fn run(conn: &mut Connection) -> Result<(), DbError> {
    let applied = applied_versions(conn)
        .map_err(|e| DbError::Migration(format!("could not read schema_migrations: {e}")))?;

    for migration in MIGRATIONS {
        if applied.contains(&migration.version) {
            continue;
        }
        apply(conn, migration).map_err(|e| {
            DbError::Migration(format!(
                "migration {} ('{}') failed: {e}",
                migration.version, migration.name
            ))
        })?;
    }

    Ok(())
}

/// Applies one migration inside a transaction, recording it in
/// `schema_migrations`. A returned error leaves the transaction rolled back.
fn apply(conn: &mut Connection, migration: &Migration) -> Result<(), rusqlite::Error> {
    let tx = conn.transaction()?;
    tx.execute_batch(migration.sql)?;
    let applied_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    tx.execute(
        "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![migration.version, migration.name, applied_at],
    )?;
    tx.commit()
}

/// Returns the set of already-applied migration versions. Empty when the
/// `schema_migrations` table does not exist yet (fresh database).
fn applied_versions(conn: &Connection) -> Result<HashSet<i64>, rusqlite::Error> {
    let exists = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_migrations'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !exists {
        return Ok(HashSet::new());
    }

    let mut stmt = conn.prepare("SELECT version FROM schema_migrations")?;
    stmt.query_map([], |row| row.get::<_, i64>(0))?
        .collect::<Result<HashSet<_>, _>>()
}

/// Migration 1 — the initial schema. `IF NOT EXISTS` keeps it a safe no-op on
/// databases created before the migration system existed; those installs are
/// simply recorded at version 1.
const INITIAL_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS schema_migrations (
    version    INTEGER PRIMARY KEY,
    name       TEXT    NOT NULL,
    applied_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS source_roots (
    id            INTEGER PRIMARY KEY,
    path          TEXT    NOT NULL UNIQUE,
    display_path  TEXT    NOT NULL,
    added_at      INTEGER NOT NULL,
    is_available  INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS media (
    id              INTEGER PRIMARY KEY,
    path            TEXT    NOT NULL UNIQUE,
    filename        TEXT    NOT NULL,
    source_root_id  INTEGER NOT NULL,
    media_type      TEXT    NOT NULL CHECK(media_type IN ('image', 'video')),
    size_bytes      INTEGER NOT NULL,
    created_at      INTEGER,
    modified_at     INTEGER NOT NULL,
    thumbnail_path  TEXT,
    duration_secs   INTEGER,
    indexed_at      INTEGER NOT NULL,
    scan_generation INTEGER NOT NULL DEFAULT 0,
    -- Cascading deletes ensure media is instantly dropped without manual cleanup queries when a source root is removed.
    FOREIGN KEY (source_root_id) REFERENCES source_roots(id) ON DELETE CASCADE
);

-- Separated from media table into a many-to-many schema to allow global tag renaming and fast cross-root filtering.
CREATE TABLE IF NOT EXISTS tags (
    id   INTEGER PRIMARY KEY,
    name TEXT    NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS media_tags (
    media_id  INTEGER NOT NULL,
    tag_id    INTEGER NOT NULL,
    PRIMARY KEY (media_id, tag_id),
    FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id)   REFERENCES tags(id)   ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_media_source_root ON media(source_root_id);
-- Fast sorting for the default 'Date modified' UI view, allowing quick limit/offset pagination.
CREATE INDEX IF NOT EXISTS idx_media_modified_at ON media(modified_at);
CREATE INDEX IF NOT EXISTS idx_media_filename    ON media(filename);
CREATE INDEX IF NOT EXISTS idx_media_tags_tag    ON media_tags(tag_id);
";

/// Migration 2 — path-qualified tag identity (A-2).
///
/// Replaces the global `tags(name UNIQUE)` table with identity keyed by
/// `(source_root_id, relative_folder_path)`, so same-named folders in different
/// roots or subtrees become distinct tags. Tags are derived from folder
/// structure at scan time, so the old (unqualified) tag rows and their
/// associations are dropped and rebuilt on the next library scan.
const PATH_QUALIFIED_TAGS: &str = "
DROP TABLE IF EXISTS media_tags;
DROP TABLE IF EXISTS tags;

CREATE TABLE tags (
    id                   INTEGER PRIMARY KEY,
    source_root_id       INTEGER NOT NULL,
    relative_folder_path TEXT    NOT NULL,
    display_name         TEXT    NOT NULL,
    display_path         TEXT    NOT NULL,
    UNIQUE (source_root_id, relative_folder_path),
    FOREIGN KEY (source_root_id) REFERENCES source_roots(id) ON DELETE CASCADE
);

CREATE TABLE media_tags (
    media_id  INTEGER NOT NULL,
    tag_id    INTEGER NOT NULL,
    PRIMARY KEY (media_id, tag_id),
    FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id)   REFERENCES tags(id)   ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_media_tags_tag     ON media_tags(tag_id);
CREATE INDEX IF NOT EXISTS idx_tags_source_root   ON tags(source_root_id);
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_is_idempotent_and_records_versions() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();

        // schema_migrations records exactly the known migrations.
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count as usize, MIGRATIONS.len());

        // Re-running applies nothing new.
        run(&mut conn).unwrap();
        let count2: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count2, count);
    }

    #[test]
    fn failed_migration_rolls_back() {
        let mut conn = Connection::open_in_memory().unwrap();
        // Bootstrap the migrations table, then attempt a bad migration directly.
        conn.execute_batch(
            "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, name TEXT NOT NULL, applied_at INTEGER NOT NULL);",
        )
        .unwrap();

        let bad = Migration {
            version: 99,
            name: "bad",
            sql: "CREATE TABLE ok (id INTEGER); CREATE TABLE ok (id INTEGER);",
        };
        assert!(apply(&mut conn, &bad).is_err());

        // The first statement's table must have been rolled back with the failure.
        let table_exists = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='ok'",
                [],
                |_| Ok(()),
            )
            .optional()
            .unwrap()
            .is_some();
        assert!(!table_exists, "partial migration must roll back");
    }
}
