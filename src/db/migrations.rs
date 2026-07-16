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
    Migration {
        version: 3,
        name: "media_schema_fixes",
        sql: MEDIA_SCHEMA_FIXES,
    },
    Migration {
        version: 4,
        name: "scan_errors_settings_session",
        sql: SCAN_ERRORS_SETTINGS_SESSION,
    },
    Migration {
        version: 5,
        name: "normalized_search_keys",
        sql: NORMALIZED_SEARCH_KEYS,
    },
];

/// Applies all pending migrations in order, each in its own transaction.
///
/// Returns [`DbError::Migration`] on the first failure without applying any
/// later migration; the failed migration's transaction is rolled back.
pub fn run(conn: &mut Connection) -> Result<usize, DbError> {
    let applied = applied_versions(conn)
        .map_err(|e| DbError::Migration(format!("could not read schema_migrations: {e}")))?;
    let mut applied_count = 0;

    for migration in MIGRATIONS {
        if applied.contains(&migration.version) {
            continue;
        }
        apply(conn, migration).map_err(|e| {
            tracing::error!(
                version = migration.version,
                name = migration.name,
                error = %e,
                "schema migration failed"
            );
            DbError::Migration(format!(
                "migration {} ('{}') failed: {e}",
                migration.version, migration.name
            ))
        })?;
        applied_count += 1;
        crate::logging::migration_applied(migration.version, migration.name);
    }

    Ok(applied_count)
}

/// Applies one migration inside a transaction, recording it in
/// `schema_migrations`. A returned error leaves the transaction rolled back.
fn apply(conn: &mut Connection, migration: &Migration) -> Result<(), rusqlite::Error> {
    let tx = conn.transaction()?;
    tx.execute_batch(migration.sql)?;
    if migration.version == 5 {
        backfill_normalized_search_keys(&tx)?;
    }
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

fn backfill_normalized_search_keys(tx: &rusqlite::Transaction<'_>) -> Result<(), rusqlite::Error> {
    let media = {
        let mut statement = tx.prepare("SELECT id, filename, path FROM media")?;
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    for (id, filename, path) in media {
        tx.execute(
            "UPDATE media
                SET filename_search = ?1, basename_search = ?2, path_search = ?3
              WHERE id = ?4",
            rusqlite::params![
                crate::db::search_normalization::normalize_search_text(&filename),
                crate::db::search_normalization::normalized_basename(&filename),
                crate::db::search_normalization::normalize_search_text(&path),
                id,
            ],
        )?;
    }

    let tags = {
        let mut statement = tx.prepare("SELECT id, display_name, display_path FROM tags")?;
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    for (id, display_name, display_path) in tags {
        tx.execute(
            "UPDATE tags
                SET display_name_search = ?1, display_path_search = ?2
              WHERE id = ?3",
            rusqlite::params![
                crate::db::search_normalization::normalize_search_text(&display_name),
                crate::db::search_normalization::normalize_search_text(&display_path),
                id,
            ],
        )?;
    }

    Ok(())
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

/// Migration 3 — required `media` columns, uniques, and indexes (A-3).
///
/// 02 §4 requires the `media` table to carry a per-root relative path, a
/// canonical path identity, thumbnail cache/state columns, and a
/// last-accessed timestamp, plus the listed unique constraints and indexes.
/// `indexed_at` is renamed to `date_added` to match the spec's "Date added"
/// semantics (assigned when a path identity is first committed, preserved
/// across rescans and metadata-only updates).
///
/// New identity columns (`relative_path`, `canonical_identity`) are added
/// nullable and are **not** backfilled here: `relative_path` requires the
/// owning root's path to compute and `canonical_identity` requires
/// filesystem resolution, so both are populated on the next scan/upsert of
/// each row. Pre-migration rows therefore keep `NULL` for these columns
/// until re-scanned; SQLite treats those NULLs as distinct, so the new
/// unique indexes do not collide on legacy rows.
const MEDIA_SCHEMA_FIXES: &str = "
ALTER TABLE media RENAME COLUMN indexed_at TO date_added;

ALTER TABLE media ADD COLUMN relative_path       TEXT;
ALTER TABLE media ADD COLUMN canonical_identity  TEXT;
ALTER TABLE media ADD COLUMN thumbnail_cache_key TEXT;
ALTER TABLE media ADD COLUMN thumbnail_stale     INTEGER NOT NULL DEFAULT 0;
ALTER TABLE media ADD COLUMN thumbnail_failure   TEXT;
ALTER TABLE media ADD COLUMN last_accessed_at    INTEGER;

CREATE UNIQUE INDEX IF NOT EXISTS idx_media_root_relpath      ON media(source_root_id, relative_path);
CREATE UNIQUE INDEX IF NOT EXISTS idx_media_canonical_identity ON media(canonical_identity);
CREATE INDEX IF NOT EXISTS idx_media_date_added       ON media(date_added);
CREATE INDEX IF NOT EXISTS idx_media_size_bytes       ON media(size_bytes);
CREATE INDEX IF NOT EXISTS idx_media_media_type       ON media(media_type);
CREATE INDEX IF NOT EXISTS idx_media_last_accessed_at ON media(last_accessed_at);
CREATE INDEX IF NOT EXISTS idx_media_root_generation  ON media(source_root_id, scan_generation);
";

/// Migration 4 — required `scan_errors`, `settings`, and `session_state`
/// tables (A-4, 02 §4).
///
/// `scan_errors` is keyed by `(source_root_id, scan_generation, path)` and
/// carries an error category, message, and last-seen timestamp (04 §12). Rows
/// are recorded for paths that fail a scan and cleared when a later, newer-
/// generation scan of the same path succeeds; the `ON DELETE CASCADE`
/// foreign key also drops a root's errors when the root is removed.
///
/// `settings` and `session_state` are simple key/value stores. This migration
/// only creates them; migrating the live data out of `state.json` is A-5 and
/// deliberately left for later, so both tables ship empty.
const SCAN_ERRORS_SETTINGS_SESSION: &str = "
CREATE TABLE IF NOT EXISTS scan_errors (
    source_root_id  INTEGER NOT NULL,
    scan_generation INTEGER NOT NULL,
    path            TEXT    NOT NULL,
    category        TEXT    NOT NULL,
    message         TEXT    NOT NULL,
    last_seen       INTEGER NOT NULL,
    PRIMARY KEY (source_root_id, scan_generation, path),
    FOREIGN KEY (source_root_id) REFERENCES source_roots(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_scan_errors_root_generation ON scan_errors(source_root_id, scan_generation);
CREATE INDEX IF NOT EXISTS idx_scan_errors_path            ON scan_errors(path);

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS session_state (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
";

/// Migration 5 — pre-normalized search keys (U-12).
///
/// SQLite's built-in case-insensitive LIKE handling is ASCII-only. Searchable
/// user text is therefore NFC-normalized and Unicode-casefolded in Rust, both
/// for these stored keys and for incoming queries.
const NORMALIZED_SEARCH_KEYS: &str = "
ALTER TABLE media ADD COLUMN filename_search TEXT NOT NULL DEFAULT '';
ALTER TABLE media ADD COLUMN basename_search TEXT NOT NULL DEFAULT '';
ALTER TABLE media ADD COLUMN path_search     TEXT NOT NULL DEFAULT '';

ALTER TABLE tags ADD COLUMN display_name_search TEXT NOT NULL DEFAULT '';
ALTER TABLE tags ADD COLUMN display_path_search TEXT NOT NULL DEFAULT '';

CREATE INDEX IF NOT EXISTS idx_media_filename_search ON media(filename_search);
CREATE INDEX IF NOT EXISTS idx_media_basename_search ON media(basename_search);
CREATE INDEX IF NOT EXISTS idx_media_path_search     ON media(path_search);
CREATE INDEX IF NOT EXISTS idx_tags_name_search     ON tags(display_name_search);
CREATE INDEX IF NOT EXISTS idx_tags_path_search     ON tags(display_path_search);
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
