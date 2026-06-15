//! Database schema initialization.
//!
//! Safe to call on every startup — all statements use `IF NOT EXISTS`.

use rusqlite::Connection;

use super::error::DbError;

/// Configures connection PRAGMAs and creates tables/indexes.
pub fn initialize(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch(PRAGMAS)?;
    conn.execute_batch(CREATE_TABLES)?;
    let _ = conn.execute("ALTER TABLE media ADD COLUMN duration_secs INTEGER", []);
    let _ = conn.execute(
        "ALTER TABLE media ADD COLUMN scan_generation INTEGER NOT NULL DEFAULT 0",
        [],
    );
    Ok(())
}

const PRAGMAS: &str = "\
    PRAGMA journal_mode = WAL;\n\
    PRAGMA foreign_keys = ON;\n\
    PRAGMA busy_timeout = 5000;\n";

const CREATE_TABLES: &str = "
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
