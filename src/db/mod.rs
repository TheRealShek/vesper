//! SQLite database for media entries, tags, and source roots.
//!
//! This module has zero GTK imports. All raw SQL is contained here —
//! the rest of the application uses the typed `Database` interface.

mod error;
mod models;
mod schema;

pub use error::DbError;
pub use models::*;

use std::path::Path;
use std::time::SystemTime;

use rusqlite::{params, Connection};

/// Handle to the application's SQLite database.
///
/// All database access goes through this type. No raw SQL
/// is used outside the `db` module.
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Opens (or creates) a database file at the given path and initializes the schema.
    pub fn open(path: &Path) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;
        schema::initialize(&conn)?;
        Ok(Self { conn })
    }

    /// Creates an in-memory database. Useful for tests.
    pub fn open_in_memory() -> Result<Self, DbError> {
        let conn = Connection::open_in_memory()?;
        schema::initialize(&conn)?;
        Ok(Self { conn })
    }

    // ── Source roots ────────────────────────────────────────────────

    /// Adds a new source root. Returns its database id.
    pub fn add_source_root(&self, path: &str) -> Result<i64, DbError> {
        let added_at = system_time_to_epoch(SystemTime::now());
        self.conn.execute(
            "INSERT INTO source_roots (path, added_at, is_available) VALUES (?1, ?2, 1)",
            params![path, added_at],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Removes a source root and all its media (via ON DELETE CASCADE).
    pub fn remove_source_root(&self, id: i64) -> Result<(), DbError> {
        self.conn
            .execute("DELETE FROM source_roots WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Lists all source roots ordered by creation time.
    pub fn list_source_roots(&self) -> Result<Vec<SourceRoot>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, path, added_at, is_available FROM source_roots ORDER BY added_at",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(SourceRoot {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    added_at: row.get(2)?,
                    is_available: row.get::<_, i64>(3)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Finds a source root by its filesystem path.
    pub fn find_source_root_by_path(&self, path: &str) -> Result<Option<SourceRoot>, DbError> {
        match self.conn.query_row(
            "SELECT id, path, added_at, is_available FROM source_roots WHERE path = ?1",
            [path],
            |row| {
                Ok(SourceRoot {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    added_at: row.get(2)?,
                    is_available: row.get::<_, i64>(3)? != 0,
                })
            },
        ) {
            Ok(root) => Ok(Some(root)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Marks a source root as available or unavailable.
    pub fn set_source_root_available(&self, id: i64, available: bool) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE source_roots SET is_available = ?1 WHERE id = ?2",
            params![available as i64, id],
        )?;
        Ok(())
    }

    // ── Media ───────────────────────────────────────────────────────

    /// Inserts or updates a media entry keyed on `path`. Returns the row id.
    pub fn upsert_media(&self, entry: &MediaEntry) -> Result<i64, DbError> {
        self.conn.execute(
            "INSERT INTO media (path, filename, source_root_id, media_type,
                                size_bytes, created_at, modified_at, indexed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(path) DO UPDATE SET
               filename       = excluded.filename,
               source_root_id = excluded.source_root_id,
               media_type     = excluded.media_type,
               size_bytes     = excluded.size_bytes,
               created_at     = excluded.created_at,
               modified_at    = excluded.modified_at,
               indexed_at     = excluded.indexed_at",
            params![
                entry.path,
                entry.filename,
                entry.source_root_id,
                entry.media_type.as_str(),
                entry.size_bytes,
                entry.created_at,
                entry.modified_at,
                entry.indexed_at,
            ],
        )?;

        // Fetch the id — works for both insert and conflict-update.
        let id: i64 = self.conn.query_row(
            "SELECT id FROM media WHERE path = ?1",
            [&entry.path],
            |row| row.get(0),
        )?;
        Ok(id)
    }

    /// Removes a media entry by its filesystem path. Returns `true` if a row was deleted.
    pub fn remove_media_by_path(&self, path: &str) -> Result<bool, DbError> {
        let changed = self
            .conn
            .execute("DELETE FROM media WHERE path = ?1", [path])?;
        Ok(changed > 0)
    }

    /// Removes all media for a source root. Returns the number of rows deleted.
    pub fn remove_media_by_source_root(&self, source_root_id: i64) -> Result<usize, DbError> {
        let changed = self.conn.execute(
            "DELETE FROM media WHERE source_root_id = ?1",
            [source_root_id],
        )?;
        Ok(changed)
    }

    /// Sets the thumbnail path for a media entry.
    pub fn set_thumbnail(&self, media_id: i64, thumb_path: &str) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE media SET thumbnail_path = ?1 WHERE id = ?2",
            params![thumb_path, media_id],
        )?;
        Ok(())
    }

    /// Returns all indexed file paths under a source root.
    /// Used for diffing against a fresh scan to detect removals.
    pub fn get_all_paths_for_root(&self, source_root_id: i64) -> Result<Vec<String>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT path FROM media WHERE source_root_id = ?1")?;
        let paths = stmt
            .query_map([source_root_id], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(paths)
    }

    // ── Tags ────────────────────────────────────────────────────────

    /// Replaces all tags for a media entry with the given set of tag names.
    /// Creates new tag rows as needed. Runs inside a transaction.
    pub fn sync_tags_for_media(&self, media_id: i64, tag_names: &[String]) -> Result<(), DbError> {
        self.conn.execute_batch("BEGIN")?;

        let result = self.sync_tags_inner(media_id, tag_names);

        match result {
            Ok(()) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(())
            }
            Err(e) => {
                // Best-effort rollback — ignore errors since we're already in a failure path.
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    fn sync_tags_inner(&self, media_id: i64, tag_names: &[String]) -> Result<(), DbError> {
        self.conn
            .execute("DELETE FROM media_tags WHERE media_id = ?1", [media_id])?;

        for name in tag_names {
            self.conn.execute(
                "INSERT OR IGNORE INTO tags (name) VALUES (?1)",
                [name.as_str()],
            )?;

            let tag_id: i64 = self.conn.query_row(
                "SELECT id FROM tags WHERE name = ?1",
                [name.as_str()],
                |row| row.get(0),
            )?;

            self.conn.execute(
                "INSERT INTO media_tags (media_id, tag_id) VALUES (?1, ?2)",
                params![media_id, tag_id],
            )?;
        }

        Ok(())
    }

    /// Returns all tags with file counts, sorted by count descending (spec section 6).
    pub fn get_all_tags_with_counts(&self) -> Result<Vec<TagWithCount>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.name, COUNT(mt.media_id) AS file_count
             FROM tags t
             JOIN media_tags mt ON t.id = mt.tag_id
             GROUP BY t.id, t.name
             ORDER BY file_count DESC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(TagWithCount {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    file_count: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Removes orphaned tags that have no media associations.
    pub fn cleanup_orphaned_tags(&self) -> Result<usize, DbError> {
        let changed = self.conn.execute(
            "DELETE FROM tags WHERE id NOT IN (SELECT DISTINCT tag_id FROM media_tags)",
            [],
        )?;
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::MediaType;

    #[test]
    fn open_in_memory() {
        let db = Database::open_in_memory().unwrap();
        let roots = db.list_source_roots().unwrap();
        assert!(roots.is_empty());
    }

    #[test]
    fn source_root_crud() {
        let db = Database::open_in_memory().unwrap();

        let id = db.add_source_root("/home/user/photos").unwrap();
        assert!(id > 0);

        let roots = db.list_source_roots().unwrap();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].path, "/home/user/photos");
        assert!(roots[0].is_available);

        db.set_source_root_available(id, false).unwrap();
        let roots = db.list_source_roots().unwrap();
        assert!(!roots[0].is_available);

        db.remove_source_root(id).unwrap();
        let roots = db.list_source_roots().unwrap();
        assert!(roots.is_empty());
    }

    #[test]
    fn find_source_root_by_path() {
        let db = Database::open_in_memory().unwrap();

        assert!(db.find_source_root_by_path("/nope").unwrap().is_none());

        db.add_source_root("/media").unwrap();
        let found = db.find_source_root_by_path("/media").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().path, "/media");
    }

    #[test]
    fn media_upsert_and_tags() {
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root("/media").unwrap();

        let entry = MediaEntry {
            path: "/media/Travel/Japan/photo.jpg".into(),
            filename: "photo.jpg".into(),
            source_root_id: root_id,
            media_type: MediaType::Image,
            size_bytes: 1024,
            created_at: Some(1000),
            modified_at: 2000,
            indexed_at: 3000,
        };

        let media_id = db.upsert_media(&entry).unwrap();
        assert!(media_id > 0);

        // Upsert same path again — should return same id.
        let media_id_2 = db.upsert_media(&entry).unwrap();
        assert_eq!(media_id, media_id_2);

        // Set tags.
        let tags = vec!["Travel".into(), "Japan".into()];
        db.sync_tags_for_media(media_id, &tags).unwrap();

        let tag_rows = db.get_all_tags_with_counts().unwrap();
        assert_eq!(tag_rows.len(), 2);

        let names: Vec<&str> = tag_rows.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"Travel"));
        assert!(names.contains(&"Japan"));

        // Replace tags — old ones removed.
        let new_tags = vec!["Travel".into(), "2023".into()];
        db.sync_tags_for_media(media_id, &new_tags).unwrap();
        let tag_rows = db.get_all_tags_with_counts().unwrap();
        assert_eq!(tag_rows.len(), 2);

        let names: Vec<&str> = tag_rows.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"Travel"));
        assert!(names.contains(&"2023"));
        assert!(!names.contains(&"Japan"));
    }

    #[test]
    fn media_removal_by_path() {
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root("/media").unwrap();

        let entry = MediaEntry {
            path: "/media/photo.jpg".into(),
            filename: "photo.jpg".into(),
            source_root_id: root_id,
            media_type: MediaType::Image,
            size_bytes: 512,
            created_at: None,
            modified_at: 1000,
            indexed_at: 2000,
        };
        db.upsert_media(&entry).unwrap();

        assert!(db.remove_media_by_path("/media/photo.jpg").unwrap());
        assert!(!db.remove_media_by_path("/media/photo.jpg").unwrap());
    }

    #[test]
    fn cascade_delete_on_source_root_removal() {
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root("/media").unwrap();

        let entry = MediaEntry {
            path: "/media/photo.jpg".into(),
            filename: "photo.jpg".into(),
            source_root_id: root_id,
            media_type: MediaType::Image,
            size_bytes: 512,
            created_at: None,
            modified_at: 1000,
            indexed_at: 2000,
        };
        let media_id = db.upsert_media(&entry).unwrap();
        db.sync_tags_for_media(media_id, &["root_tag".into()])
            .unwrap();

        // Removing source root cascades to media and media_tags.
        db.remove_source_root(root_id).unwrap();
        let paths = db.get_all_paths_for_root(root_id).unwrap();
        assert!(paths.is_empty());

        let cleaned = db.cleanup_orphaned_tags().unwrap();
        assert_eq!(cleaned, 1);
    }

    #[test]
    fn get_all_paths_for_root() {
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root("/media").unwrap();

        for name in &["a.jpg", "b.png", "c.mp4"] {
            let entry = MediaEntry {
                path: format!("/media/{name}"),
                filename: (*name).into(),
                source_root_id: root_id,
                media_type: if name.ends_with("mp4") {
                    MediaType::Video
                } else {
                    MediaType::Image
                },
                size_bytes: 100,
                created_at: None,
                modified_at: 1000,
                indexed_at: 2000,
            };
            db.upsert_media(&entry).unwrap();
        }

        let paths = db.get_all_paths_for_root(root_id).unwrap();
        assert_eq!(paths.len(), 3);
    }

    #[test]
    fn set_thumbnail() {
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root("/media").unwrap();

        let entry = MediaEntry {
            path: "/media/photo.jpg".into(),
            filename: "photo.jpg".into(),
            source_root_id: root_id,
            media_type: MediaType::Image,
            size_bytes: 512,
            created_at: None,
            modified_at: 1000,
            indexed_at: 2000,
        };
        let media_id = db.upsert_media(&entry).unwrap();
        db.set_thumbnail(media_id, "/cache/thumb_123.jpg").unwrap();
    }
}
