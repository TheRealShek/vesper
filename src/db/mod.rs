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

use rusqlite::{Connection, OpenFlags, params};
use std::sync::Mutex;

/// Handle to the application's SQLite database.
///
/// All database access goes through this type. No raw SQL
/// is used outside the `db` module.
pub struct Database {
    writer: Mutex<Connection>,
    reader: Mutex<Connection>,
}

impl Database {
    /// Opens (or creates) a database file at the given path and initializes the schema.
    pub fn open(path: &Path) -> Result<Self, DbError> {
        let writer = Connection::open(path)?;
        writer.execute_batch("PRAGMA journal_mode=WAL;")?;
        schema::initialize(&writer)?;

        let reader = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_URI
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;

        Ok(Self {
            writer: Mutex::new(writer),
            reader: Mutex::new(reader),
        })
    }

    /// Creates an in-memory database. Useful for tests.
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, DbError> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let uri = format!("file:memdb{}?mode=memory&cache=shared", id);

        let writer = Connection::open_with_flags(
            &uri,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_URI,
        )?;
        writer.execute_batch("PRAGMA journal_mode=WAL;")?;
        schema::initialize(&writer)?;

        let reader = Connection::open_with_flags(
            &uri,
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_URI
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;

        Ok(Self {
            writer: Mutex::new(writer),
            reader: Mutex::new(reader),
        })
    }

    // Dummy lock to avoid changing call sites in main.rs and scan.rs
    pub fn lock(&self) -> Result<&Self, core::convert::Infallible> {
        Ok(self)
    }

    // ── Source roots ────────────────────────────────────────────────

    pub fn add_source_root(&self, path: &str, display_path: &str) -> Result<i64, DbError> {
        let added_at = system_time_to_epoch(SystemTime::now());
        let writer = self.writer.lock().unwrap();
        writer.execute(
            "INSERT INTO source_roots (path, display_path, added_at, is_available) VALUES (?1, ?2, ?3, 1)",
            params![path, display_path, added_at],
        )?;
        Ok(writer.last_insert_rowid())
    }

    /// Removes a source root and all its media (via ON DELETE CASCADE).
    pub fn remove_source_root(&self, id: i64) -> Result<(), DbError> {
        let writer = self.writer.lock().unwrap();
        writer.execute("DELETE FROM source_roots WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Lists all source roots ordered by creation time.
    pub fn list_source_roots(&self) -> Result<Vec<SourceRoot>, DbError> {
        let reader = self.reader.lock().unwrap();
        let mut stmt = reader.prepare(
            "SELECT id, path, display_path, added_at, is_available FROM source_roots ORDER BY added_at",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(SourceRoot {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    display_path: row.get(2)?,
                    added_at: row.get(3)?,
                    is_available: row.get::<_, i64>(4)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn find_source_root_by_path(&self, path: &str) -> Result<Option<SourceRoot>, DbError> {
        let reader = self.reader.lock().unwrap();
        match reader.query_row(
            "SELECT id, path, display_path, added_at, is_available FROM source_roots WHERE path = ?1",
            [path],
            |row| {
                Ok(SourceRoot {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    display_path: row.get(2)?,
                    added_at: row.get(3)?,
                    is_available: row.get::<_, i64>(4)? != 0,
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
        let writer = self.writer.lock().unwrap();
        writer.execute(
            "UPDATE source_roots SET is_available = ?1 WHERE id = ?2",
            params![available as i64, id],
        )?;
        Ok(())
    }

    // ── Media ───────────────────────────────────────────────────────

    /// Inserts or updates a media entry keyed on `path`. Returns the row id.
    pub fn upsert_media(&self, entry: &MediaEntry) -> Result<i64, DbError> {
        let writer = self.writer.lock().unwrap();
        self.upsert_media_inner(&writer, entry)
    }

    fn upsert_media_inner(&self, writer: &Connection, entry: &MediaEntry) -> Result<i64, DbError> {
        writer.execute(
            "INSERT INTO media (path, filename, source_root_id, media_type,
                                size_bytes, created_at, modified_at, thumbnail_path, duration_secs, indexed_at, scan_generation)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, NULL, ?8, ?9)
             ON CONFLICT(path) DO UPDATE SET
               filename       = excluded.filename,
               source_root_id = excluded.source_root_id,
               media_type     = excluded.media_type,
               size_bytes     = excluded.size_bytes,
               created_at     = excluded.created_at,
               thumbnail_path = CASE WHEN modified_at != excluded.modified_at THEN NULL ELSE thumbnail_path END,
               modified_at    = excluded.modified_at,
               indexed_at     = excluded.indexed_at,
               scan_generation= excluded.scan_generation",
            params![
                entry.path,
                entry.filename,
                entry.source_root_id,
                entry.media_type.as_str(),
                entry.size_bytes,
                entry.created_at,
                entry.modified_at,
                entry.indexed_at,
                entry.scan_generation,
            ],
        )?;

        let id: i64 = writer.query_row(
            "SELECT id FROM media WHERE path = ?1",
            [&entry.path],
            |row| row.get(0),
        )?;
        Ok(id)
    }

    /// Inserts or updates multiple media entries and their associated tags in a single transaction.
    pub fn upsert_media_batch(&self, entries: &[(MediaEntry, Vec<String>)]) -> Result<(), DbError> {
        let mut writer = self.writer.lock().unwrap();
        let tx = writer.unchecked_transaction()?;

        for (entry, tags) in entries {
            let media_id = self.upsert_media_inner(&tx, entry)?;
            self.sync_tags_inner(&tx, media_id, tags)?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Removes a media entry by its filesystem path. Returns `true` if a row was deleted.
    pub fn remove_media_by_path(&self, path: &str) -> Result<bool, DbError> {
        let writer = self.writer.lock().unwrap();
        let changed = writer.execute("DELETE FROM media WHERE path = ?1", [path])?;
        Ok(changed > 0)
    }

    /// Sets the thumbnail path and duration for a media entry.
    pub fn set_thumbnail_and_duration(
        &self,
        media_id: i64,
        path: &str,
        modified_at: i64,
        thumb_path: &str,
        duration: Option<i64>,
    ) -> Result<(), DbError> {
        let writer = self.writer.lock().unwrap();
        let affected = writer.execute(
            "UPDATE media SET thumbnail_path = ?1, duration_secs = ?2 WHERE id = ?3 AND path = ?4 AND modified_at = ?5",
            params![thumb_path, duration, media_id, path, modified_at],
        )?;
        if affected == 0 {
            eprintln!(
                "Thumbnail update dropped: row missing or stale for media id {}",
                media_id
            );
        }
        Ok(())
    }

    /// Gets the maximum scan_generation currently in the database for the given source_root_id.
    pub fn get_max_scan_generation(&self, source_root_id: i64) -> Result<i64, DbError> {
        let reader = self.reader.lock().unwrap();
        let max_gen: i64 = reader.query_row(
            "SELECT COALESCE(MAX(scan_generation), 0) FROM media WHERE source_root_id = ?1",
            [source_root_id],
            |row| row.get(0),
        )?;
        Ok(max_gen)
    }

    /// Removes all media entries for the given source_root_id that have a different scan_generation.
    pub fn remove_stale_media(&self, source_root_id: i64, scan_gen: i64) -> Result<usize, DbError> {
        let writer = self.writer.lock().unwrap();
        let count = writer.execute(
            "DELETE FROM media WHERE source_root_id = ?1 AND scan_generation != ?2",
            params![source_root_id, scan_gen],
        )?;
        Ok(count)
    }

    /// Removes all media entries under a subtree prefix that have a different scan_generation.
    pub fn remove_stale_media_in_subtree(
        &self,
        source_root_id: i64,
        subtree_prefix: &str,
        scan_gen: i64,
    ) -> Result<usize, DbError> {
        let writer = self.writer.lock().unwrap();
        let like_pattern = format!("{}%", subtree_prefix);
        let count = writer.execute(
            "DELETE FROM media WHERE source_root_id = ?1 AND path LIKE ?2 AND scan_generation != ?3",
            params![source_root_id, like_pattern, scan_gen],
        )?;
        Ok(count)
    }

    /// Retrieves all media entries with their tags concatenated by commas.
    pub fn get_all_media_with_tags(&self) -> Result<Vec<(MediaRow, String)>, DbError> {
        let reader = self.reader.lock().unwrap();
        let mut stmt = reader.prepare(
            "SELECT m.id, m.path, m.filename, m.source_root_id, m.media_type, 
                    m.size_bytes, m.created_at, m.modified_at, m.thumbnail_path, m.duration_secs, m.indexed_at, m.scan_generation,
                    IFNULL(GROUP_CONCAT(t.name, ','), '') AS tags
             FROM media m
             LEFT JOIN media_tags mt ON m.id = mt.media_id
             LEFT JOIN tags t ON mt.tag_id = t.id
             GROUP BY m.id",
        )?;
        let rows = stmt
            .query_map([], |row| {
                let media_type_str: String = row.get(4)?;
                let media_type = crate::events::MediaType::from_db_str(&media_type_str)
                    .unwrap_or(crate::events::MediaType::Image); // fallback

                let media = MediaRow {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    filename: row.get(2)?,
                    source_root_id: row.get(3)?,
                    media_type,
                    size_bytes: row.get(5)?,
                    created_at: row.get(6)?,
                    modified_at: row.get(7)?,
                    thumbnail_path: row.get(8)?,
                    duration_secs: row.get(9)?,
                    indexed_at: row.get(10)?,
                    scan_generation: row.get(11)?,
                };
                let tags: String = row.get(12)?;
                Ok((media, tags))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn query_media(
        &self,
        q: &crate::events::MediaQuery,
    ) -> Result<(Vec<crate::events::UiMediaItem>, u32), DbError> {
        let reader = self.reader.lock().unwrap();

        let mut base_query = String::from("FROM media m");
        let mut where_clauses = Vec::new();
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        let mut arg_idx = 1;

        if !q.tags.is_empty() {
            base_query.push_str(" JOIN media_tags mt ON m.id = mt.media_id");
            base_query.push_str(" JOIN tags t ON mt.tag_id = t.id");

            let placeholders = (0..q.tags.len())
                .map(|i| format!("?{}", arg_idx + i))
                .collect::<Vec<_>>()
                .join(", ");

            where_clauses.push(format!("t.name IN ({})", placeholders));
            for tag in &q.tags {
                args.push(Box::new(tag.clone()));
            }
            arg_idx += q.tags.len();
        }

        if let Some(search) = &q.search {
            if !search.is_empty() {
                where_clauses.push(format!("m.filename LIKE ?{}", arg_idx));
                args.push(Box::new(format!("%{}%", search)));
                arg_idx += 1;
            }
        }

        let where_sql = if where_clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_clauses.join(" AND "))
        };

        let group_by = if !q.tags.is_empty() {
            if q.tag_mode == crate::events::TagMode::All {
                format!(
                    "GROUP BY m.id HAVING COUNT(DISTINCT t.id) = {}",
                    q.tags.len()
                )
            } else {
                "GROUP BY m.id".to_string()
            }
        } else {
            String::new()
        };

        let count_query = format!(
            "SELECT COUNT(*) FROM (SELECT m.id {} {} {})",
            base_query, where_sql, group_by
        );

        let args_ref: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();

        let total_count: u32 = reader.query_row(
            &count_query,
            rusqlite::params_from_iter(args_ref.iter()),
            |row| row.get(0),
        )?;

        let order_by = match q.sort {
            crate::events::SortOrder::DateModifiedDesc => "ORDER BY m.modified_at DESC, m.id DESC",
            crate::events::SortOrder::DateModifiedAsc => "ORDER BY m.modified_at ASC, m.id ASC",
            crate::events::SortOrder::DateCreatedDesc => "ORDER BY m.created_at DESC, m.id DESC",
            crate::events::SortOrder::DateCreatedAsc => "ORDER BY m.created_at ASC, m.id ASC",
            crate::events::SortOrder::FilenameAsc => "ORDER BY m.filename ASC, m.id ASC",
            crate::events::SortOrder::FilenameDesc => "ORDER BY m.filename DESC, m.id DESC",
            crate::events::SortOrder::FileSizeDesc => "ORDER BY m.size_bytes DESC, m.id DESC",
            crate::events::SortOrder::FileSizeAsc => "ORDER BY m.size_bytes ASC, m.id ASC",
        };

        let select_cols = "m.id, m.path, m.filename, m.source_root_id, m.media_type, \
                           m.size_bytes, m.created_at, m.modified_at, m.thumbnail_path, m.duration_secs, \
                           (SELECT GROUP_CONCAT(tags.name, ',') FROM tags JOIN media_tags ON tags.id = media_tags.tag_id WHERE media_tags.media_id = m.id) AS all_tags";

        let limit_offset = format!("LIMIT {} OFFSET {}", q.limit, q.offset);

        let data_query = format!(
            "SELECT {} {} {} {} {} {}",
            select_cols, base_query, where_sql, group_by, order_by, limit_offset
        );

        let mut stmt = reader.prepare(&data_query)?;

        let offline_roots: std::collections::HashSet<i64> = reader
            .prepare("SELECT id FROM source_roots WHERE is_available = 0")?
            .query_map([], |row| row.get(0))?
            .filter_map(Result::ok)
            .collect();

        let rows = stmt
            .query_map(rusqlite::params_from_iter(args_ref.iter()), |row| {
                let media_type_str: String = row.get(4)?;
                let media_type = crate::events::MediaType::from_db_str(&media_type_str)
                    .unwrap_or(crate::events::MediaType::Image);

                let root_id: i64 = row.get(3)?;
                let is_offline = offline_roots.contains(&root_id);

                let tags_str: Option<String> = row.get(10)?;

                Ok(crate::events::UiMediaItem {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    filename: row.get(2)?,
                    tags: tags_str.unwrap_or_default(),
                    thumbnail_path: row.get(8).unwrap_or_default(),
                    duration_secs: row.get(9).unwrap_or(-1),
                    media_type,
                    size_bytes: row.get(5)?,
                    created_at: row.get(6)?,
                    modified_at: row.get(7)?,
                    is_offline,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok((rows, total_count))
    }

    // ── Tags ────────────────────────────────────────────────────────

    /// Replaces all tags for a media entry with the given set of tag names.
    /// Creates new tag rows as needed. Runs inside a transaction.
    #[cfg(test)]
    pub fn sync_tags_for_media(&self, media_id: i64, tag_names: &[String]) -> Result<(), DbError> {
        let mut writer = self.writer.lock().unwrap();
        let tx = writer.unchecked_transaction()?;
        self.sync_tags_inner(&tx, media_id, tag_names)?;
        tx.commit()?;
        Ok(())
    }

    fn sync_tags_inner(
        &self,
        writer: &Connection,
        media_id: i64,
        tag_names: &[String],
    ) -> Result<(), DbError> {
        writer.execute("DELETE FROM media_tags WHERE media_id = ?1", [media_id])?;

        for name in tag_names {
            writer.execute(
                "INSERT OR IGNORE INTO tags (name) VALUES (?1)",
                [name.as_str()],
            )?;

            let tag_id: i64 = writer.query_row(
                "SELECT id FROM tags WHERE name = ?1",
                [name.as_str()],
                |row| row.get(0),
            )?;

            writer.execute(
                "INSERT INTO media_tags (media_id, tag_id) VALUES (?1, ?2)",
                params![media_id, tag_id],
            )?;
        }

        Ok(())
    }

    /// Returns all indexed file paths under a source root.
    /// Used in tests for verifying deletions.
    #[cfg(test)]
    pub fn get_all_paths_for_root(&self, source_root_id: i64) -> Result<Vec<String>, DbError> {
        let reader = self.reader.lock().unwrap();
        let mut stmt = reader.prepare("SELECT path FROM media WHERE source_root_id = ?1")?;
        let paths = stmt
            .query_map([source_root_id], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(paths)
    }

    /// Returns all tags with file counts, sorted by count descending (spec section 6).
    pub fn get_all_tags_with_counts(&self) -> Result<Vec<TagWithCount>, DbError> {
        let reader = self.reader.lock().unwrap();
        let mut stmt = reader.prepare(
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
        let writer = self.writer.lock().unwrap();
        let changed = writer.execute(
            "DELETE FROM tags WHERE id NOT IN (SELECT DISTINCT tag_id FROM media_tags)",
            [],
        )?;
        Ok(changed)
    }

    /// Removes orphaned tags scoped to a subtree.
    pub fn cleanup_orphaned_tags_in_subtree(&self, subtree_prefix: &str) -> Result<usize, DbError> {
        let writer = self.writer.lock().unwrap();
        let like_pattern = format!("{}%", subtree_prefix);
        let changed = writer.execute(
            "DELETE FROM tags WHERE id NOT IN (SELECT DISTINCT tag_id FROM media_tags)
             AND id IN (
                 SELECT DISTINCT tag_id FROM media_tags
                 WHERE media_id IN (SELECT id FROM media WHERE path LIKE ?1)
             )",
            params![like_pattern],
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

        let id = db
            .add_source_root("/home/user/photos", "/home/user/photos")
            .unwrap();
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

        db.add_source_root("/media", "/media").unwrap();
        let found = db.find_source_root_by_path("/media").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().path, "/media");
    }

    #[test]
    fn media_upsert_and_tags() {
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root("/media", "/media").unwrap();

        let entry = MediaEntry {
            path: "/media/Travel/Japan/photo.jpg".into(),
            filename: "photo.jpg".into(),
            source_root_id: root_id,
            media_type: MediaType::Image,
            size_bytes: 1024,
            created_at: Some(1000),
            modified_at: 2000,
            indexed_at: 3000,
            scan_generation: 1,
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
        let root_id = db.add_source_root("/media", "/media").unwrap();

        let entry = MediaEntry {
            path: "/media/photo.jpg".into(),
            filename: "photo.jpg".into(),
            source_root_id: root_id,
            media_type: MediaType::Image,
            size_bytes: 512,
            created_at: None,
            modified_at: 1000,
            indexed_at: 2000,
            scan_generation: 1,
        };
        db.upsert_media(&entry).unwrap();

        assert!(db.remove_media_by_path("/media/photo.jpg").unwrap());
        assert!(!db.remove_media_by_path("/media/photo.jpg").unwrap());
    }

    #[test]
    fn cascade_delete_on_source_root_removal() {
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root("/media", "/media").unwrap();

        let entry = MediaEntry {
            path: "/media/photo.jpg".into(),
            filename: "photo.jpg".into(),
            source_root_id: root_id,
            media_type: MediaType::Image,
            size_bytes: 512,
            created_at: None,
            modified_at: 1000,
            indexed_at: 2000,
            scan_generation: 1,
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
        let root_id = db.add_source_root("/media", "/media").unwrap();

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
                scan_generation: 1,
            };
            db.upsert_media(&entry).unwrap();
        }

        let paths = db.get_all_paths_for_root(root_id).unwrap();
        assert_eq!(paths.len(), 3);
    }

    #[test]
    fn set_thumbnail() {
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root("/media", "/media").unwrap();

        let entry = MediaEntry {
            path: "/media/photo.jpg".into(),
            filename: "photo.jpg".into(),
            source_root_id: root_id,
            media_type: MediaType::Image,
            size_bytes: 512,
            created_at: None,
            modified_at: 1000,
            indexed_at: 2000,
            scan_generation: 1,
        };
        let media_id = db.upsert_media(&entry).unwrap();
        db.set_thumbnail_and_duration(
            media_id,
            "/media/photo1.jpg",
            100,
            "/cache/thumb_123.jpg",
            None,
        )
        .unwrap();
    }
}
