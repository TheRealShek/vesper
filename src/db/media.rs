use super::{Database, DbError, MediaEntry, MediaItem, MediaRow, TagIdentity};
use rusqlite::{Connection, params};

fn thumbnail_entries_matching(
    connection: &Connection,
    predicate: &str,
    params: impl rusqlite::Params,
) -> Result<Vec<crate::db::ThumbnailCacheEntry>, DbError> {
    let sql = format!(
        "SELECT id, thumbnail_cache_key, thumbnail_path, last_accessed_at
           FROM media
          WHERE thumbnail_cache_key IS NOT NULL
            AND thumbnail_path IS NOT NULL
            AND ({predicate})
          ORDER BY id"
    );
    let mut stmt = connection.prepare(&sql)?;
    let entries = stmt
        .query_map(params, |row| {
            Ok(crate::db::ThumbnailCacheEntry {
                media_id: row.get(0)?,
                cache_key: row.get(1)?,
                thumbnail_path: row.get(2)?,
                last_accessed_at: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(entries)
}

impl Database {
    // ── Media ───────────────────────────────────────────────────────

    pub(crate) fn upsert_media_inner(
        &self,
        writer: &Connection,
        entry: &MediaEntry,
        scan_gen: i64,
    ) -> Result<i64, DbError> {
        let filename_search = super::search_normalization::normalize_search_text(&entry.filename);
        let basename_search = super::search_normalization::normalized_basename(&entry.filename);
        let path_search = super::search_normalization::normalize_search_text(&entry.path);
        writer.execute(
            // date_added is set only on first insert and deliberately left out of the
            // ON CONFLICT update so it is preserved across rescans and metadata-only
            // updates (02 §4 "Date added" semantics).
            "INSERT INTO media (path, relative_path, canonical_identity, filename, filename_search,
                                basename_search, path_search, source_root_id, media_type, size_bytes,
                                created_at, modified_at, thumbnail_path, duration_secs, date_added, scan_generation)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, NULL, NULL, strftime('%s', 'now'), ?13)
             ON CONFLICT(path) DO UPDATE SET
               relative_path      = excluded.relative_path,
               canonical_identity = excluded.canonical_identity,
               filename           = excluded.filename,
               filename_search    = excluded.filename_search,
               basename_search    = excluded.basename_search,
               path_search        = excluded.path_search,
               source_root_id     = excluded.source_root_id,
               media_type         = excluded.media_type,
               size_bytes         = excluded.size_bytes,
               created_at         = excluded.created_at,
               -- T-1: on a content change, flag the thumbnail stale but KEEP the
               -- old thumbnail_path/cache_key so it stays visible until an
               -- explicit regeneration succeeds (02 §4). Never blank it here.
               thumbnail_stale    = CASE WHEN modified_at != excluded.modified_at THEN 1 ELSE thumbnail_stale END,
               modified_at        = excluded.modified_at,
               scan_generation    = excluded.scan_generation",
            params![
                entry.path,
                entry.relative_path,
                entry.canonical_identity,
                entry.filename,
                filename_search,
                basename_search,
                path_search,
                entry.source_root_id,
                entry.media_type.as_str(),
                entry.size_bytes,
                entry.created_at,
                entry.modified_at,
                scan_gen,
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
    pub fn upsert_media_batch(
        &self,
        entries: &[(MediaEntry, Vec<TagIdentity>)],
        scan_gen: i64,
    ) -> Result<(), DbError> {
        let writer = self.lock_writer()?;
        // unchecked_transaction avoids taking &mut self, matching the thread-safe &self signature required by Arc.
        let tx = writer.unchecked_transaction()?;

        for (entry, tags) in entries {
            let media_id = self.upsert_media_inner(&tx, entry, scan_gen)?;
            self.sync_tags_inner(&tx, media_id, tags)?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Removes a media entry by its filesystem path. Returns `true` if a row was deleted.
    pub fn remove_media_by_path(&self, path: &str) -> Result<bool, DbError> {
        let writer = self.lock_writer()?;
        let changed = writer.execute("DELETE FROM media WHERE path = ?1", [path])?;
        Ok(changed > 0)
    }

    /// Records a successfully generated thumbnail (T-1): sets the stable cache
    /// key, path, and duration, and clears the stale/failure flags.
    ///
    /// Guarded on `modified_at` so a thumbnail generated for an older version of
    /// the file cannot overwrite a row that has since been modified — its stale
    /// flag then stays set for a later regeneration. Returns whether a row was
    /// updated.
    pub fn set_thumbnail_success(
        &self,
        media_id: i64,
        cache_key: &str,
        thumb_path: &str,
        modified_at: i64,
        duration: Option<i64>,
    ) -> Result<bool, DbError> {
        let writer = self.lock_writer()?;
        let affected = writer.execute(
            "UPDATE media
                SET thumbnail_cache_key = ?1,
                    thumbnail_path      = ?2,
                    duration_secs       = ?3,
                    thumbnail_stale     = 0,
                    thumbnail_failure   = NULL
              WHERE id = ?4 AND modified_at = ?5",
            params![cache_key, thumb_path, duration, media_id, modified_at],
        )?;
        Ok(affected > 0)
    }

    /// Records a thumbnail generation failure (T-1).
    ///
    /// The reason is stored in `thumbnail_failure` so the UI can show a stable
    /// placeholder. The previous thumbnail (path + cache key) is deliberately
    /// left in place, so a kept-old thumbnail keeps showing.
    pub fn set_thumbnail_failure(&self, media_id: i64, reason: &str) -> Result<(), DbError> {
        let writer = self.lock_writer()?;
        writer.execute(
            "UPDATE media SET thumbnail_failure = ?1 WHERE id = ?2",
            params![reason, media_id],
        )?;
        Ok(())
    }

    /// Persists a read-based LRU timestamp only when the per-item batching
    /// window has elapsed. Returns whether SQLite performed a write.
    pub fn record_thumbnail_access(
        &self,
        media_id: i64,
        accessed_at: i64,
    ) -> Result<bool, DbError> {
        let writer = self.lock_writer()?;
        let affected = writer.execute(
            "UPDATE media
                SET last_accessed_at = ?1
              WHERE id = ?2
                AND thumbnail_cache_key IS NOT NULL
                AND (last_accessed_at IS NULL OR last_accessed_at <= ?3)",
            params![
                accessed_at,
                media_id,
                accessed_at - crate::config::THUMBNAIL_ACCESS_BATCH_MS
            ],
        )?;
        Ok(affected > 0)
    }

    /// Lists disk-cache entries in least-recently-used order. Never uses file
    /// atime/mtime for ordering; those are unreliable on common Linux mounts.
    pub fn list_thumbnail_cache_entries(
        &self,
    ) -> Result<Vec<crate::db::ThumbnailCacheEntry>, DbError> {
        let reader = self.lock_reader()?;
        let mut stmt = reader.prepare(
            "SELECT id, thumbnail_cache_key, thumbnail_path, last_accessed_at
               FROM media
              WHERE thumbnail_cache_key IS NOT NULL AND thumbnail_path IS NOT NULL
              ORDER BY COALESCE(last_accessed_at, 0), id",
        )?;
        let entries = stmt
            .query_map([], |row| {
                Ok(crate::db::ThumbnailCacheEntry {
                    media_id: row.get(0)?,
                    cache_key: row.get(1)?,
                    thumbnail_path: row.get(2)?,
                    last_accessed_at: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(entries)
    }

    /// Clears a manifest entry after its disk file has been evicted. The key
    /// guard prevents an old maintenance pass from clearing a regenerated row.
    pub fn clear_evicted_thumbnail(&self, media_id: i64, cache_key: &str) -> Result<bool, DbError> {
        let writer = self.lock_writer()?;
        let affected = writer.execute(
            "UPDATE media
                SET thumbnail_cache_key = NULL, thumbnail_path = NULL,
                    thumbnail_stale = 0, last_accessed_at = NULL
              WHERE id = ?1 AND thumbnail_cache_key = ?2",
            params![media_id, cache_key],
        )?;
        Ok(affected > 0)
    }

    pub(crate) fn thumbnail_cache_entries_for_path_tree(
        &self,
        path: &str,
    ) -> Result<Vec<crate::db::ThumbnailCacheEntry>, DbError> {
        let reader = self.lock_reader()?;
        let escaped = path
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let prefix = format!("{escaped}/%");
        thumbnail_entries_matching(
            &reader,
            "path = ?1 OR path LIKE ?2 ESCAPE '\\'",
            params![path, prefix],
        )
    }

    pub(crate) fn thumbnail_cache_entries_for_root(
        &self,
        root_id: i64,
    ) -> Result<Vec<crate::db::ThumbnailCacheEntry>, DbError> {
        let reader = self.lock_reader()?;
        thumbnail_entries_matching(&reader, "source_root_id = ?1", [root_id])
    }

    pub(crate) fn thumbnail_cache_entries_for_stale_generation(
        &self,
        root_id: i64,
        scan_gen: i64,
    ) -> Result<Vec<crate::db::ThumbnailCacheEntry>, DbError> {
        let reader = self.lock_reader()?;
        thumbnail_entries_matching(
            &reader,
            "source_root_id = ?1 AND scan_generation < ?2",
            params![root_id, scan_gen],
        )
    }

    pub(crate) fn thumbnail_cache_entries_for_stale_subtree(
        &self,
        root_id: i64,
        subtree_prefix: &str,
        scan_gen: i64,
    ) -> Result<Vec<crate::db::ThumbnailCacheEntry>, DbError> {
        let reader = self.lock_reader()?;
        let like_pattern = format!("{subtree_prefix}%");
        thumbnail_entries_matching(
            &reader,
            "source_root_id = ?1 AND path LIKE ?2 AND scan_generation < ?3",
            params![root_id, like_pattern, scan_gen],
        )
    }

    /// Reads the thumbnail cache status for a media row (T-1). `None` if no such
    /// row exists.
    pub fn get_thumbnail_status(
        &self,
        media_id: i64,
    ) -> Result<Option<crate::db::ThumbnailStatus>, DbError> {
        let reader = self.lock_reader()?;
        let mut stmt = reader.prepare(
            "SELECT thumbnail_cache_key, thumbnail_path, thumbnail_stale, thumbnail_failure
               FROM media WHERE id = ?1",
        )?;
        let result = stmt.query_row([media_id], |row| {
            Ok(crate::db::ThumbnailStatus {
                cache_key: row.get(0)?,
                thumbnail_path: row.get(1)?,
                stale: row.get::<_, i64>(2)? != 0,
                failure: row.get(3)?,
            })
        });
        match result {
            Ok(status) => Ok(Some(status)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Reads the fields needed to (re)generate a thumbnail and compute its stable
    /// cache key (T-1). `None` if no such row exists.
    pub fn get_thumbnail_source(
        &self,
        media_id: i64,
    ) -> Result<Option<crate::db::ThumbnailSource>, DbError> {
        let reader = self.lock_reader()?;
        let mut stmt = reader.prepare(
            "SELECT path, canonical_identity, media_type, size_bytes, modified_at
               FROM media WHERE id = ?1",
        )?;
        let result = stmt.query_row([media_id], |row| {
            let path: String = row.get(0)?;
            let canonical: Option<String> = row.get(1)?;
            let media_type: String = row.get(2)?;
            let size_bytes: i64 = row.get(3)?;
            let modified_at: i64 = row.get(4)?;
            Ok((path, canonical, media_type, size_bytes, modified_at))
        });
        match result {
            Ok((path, canonical, media_type, size_bytes, modified_at)) => {
                let media_type = crate::events::MediaType::from_db_str(&media_type)
                    .unwrap_or(crate::events::MediaType::Image);
                // Fall back to the raw path when canonical identity is unknown so
                // a key can still be computed.
                let canonical_identity = canonical.unwrap_or_else(|| path.clone());
                Ok(Some(crate::db::ThumbnailSource {
                    media_id,
                    path,
                    canonical_identity,
                    media_type,
                    size_bytes,
                    modified_at,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Lists media ids whose thumbnails need regeneration (T-1): stale, or the
    /// last attempt failed. The explicit-regeneration operation iterates these;
    /// B-6's maintenance UI will drive it later.
    pub fn list_media_needing_thumbnail_regen(&self) -> Result<Vec<i64>, DbError> {
        let reader = self.lock_reader()?;
        let mut stmt = reader.prepare(
            "SELECT id FROM media
              WHERE thumbnail_stale = 1 OR thumbnail_failure IS NOT NULL
              ORDER BY id",
        )?;
        let ids = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<i64>, _>>()?;
        Ok(ids)
    }

    /// Gets the maximum scan_generation currently in the database for the given source_root_id.
    pub fn get_max_scan_generation(&self, source_root_id: i64) -> Result<i64, DbError> {
        let reader = self.lock_reader()?;
        let max_gen: i64 = reader.query_row(
            "SELECT COALESCE(MAX(scan_generation), 0) FROM media WHERE source_root_id = ?1",
            [source_root_id],
            |row| row.get(0),
        )?;
        Ok(max_gen)
    }

    // Separate from subtree removal because full scans authoritative-delete across the whole root.
    /// Removes all media entries for the given source_root_id that have a strictly older scan_generation.
    pub fn remove_stale_media(&self, source_root_id: i64, scan_gen: i64) -> Result<usize, DbError> {
        let writer = self.lock_writer()?;
        let count = writer.execute(
            "DELETE FROM media WHERE source_root_id = ?1 AND scan_generation < ?2",
            params![source_root_id, scan_gen],
        )?;
        Ok(count)
    }

    // Separate to scope deletions only to the scanned subtree, leaving unrelated stale files untouched.
    /// Removes all media entries under a subtree prefix that have a strictly older scan_generation.
    pub fn remove_stale_media_in_subtree(
        &self,
        source_root_id: i64,
        subtree_prefix: &str,
        scan_gen: i64,
    ) -> Result<usize, DbError> {
        let writer = self.lock_writer()?;
        let like_pattern = format!("{}%", subtree_prefix);
        let count = writer.execute(
            "DELETE FROM media WHERE source_root_id = ?1 AND path LIKE ?2 AND scan_generation < ?3",
            params![source_root_id, like_pattern, scan_gen],
        )?;
        Ok(count)
    }

    /// Removes a media entry and any descendants if it was a directory. Returns the paths of deleted items.
    pub fn remove_media_and_descendants(&self, path: &str) -> Result<Vec<String>, DbError> {
        let writer = self.lock_writer()?;
        let escaped_path = path
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let prefix = format!("{}/%", escaped_path);
        let mut stmt =
            writer.prepare("SELECT path FROM media WHERE path = ?1 OR path LIKE ?2 ESCAPE '\\'")?;
        let paths = stmt
            .query_map(params![path, prefix], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        if !paths.is_empty() {
            writer.execute(
                "DELETE FROM media WHERE path = ?1 OR path LIKE ?2 ESCAPE '\\'",
                params![path, prefix],
            )?;
        }
        Ok(paths)
    }

    /// Total number of media rows. Used to plan bounded hydration chunks (B-2).
    pub fn count_media(&self) -> Result<i64, DbError> {
        let reader = self.lock_reader()?;
        let count: i64 = reader.query_row("SELECT COUNT(*) FROM media", [], |row| row.get(0))?;
        Ok(count)
    }

    /// Reads one bounded window of media for UI hydration, ordered stably by id,
    /// with tags concatenated and offline state derived from the owning root's
    /// availability (B-2 sub-step c).
    ///
    /// This is a pure database read: it does not probe the filesystem, touch the
    /// watcher, or write the database. It replaces the former
    /// `get_all_media_with_tags` full-library reload — hydration now streams
    /// these bounded chunks instead of loading the whole store in one event
    /// (02 §5, 03 §9, ARCH-004).
    pub fn hydrate_media_chunk(
        &self,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<crate::events::UiMediaItem>, DbError> {
        let reader = self.lock_reader()?;
        let mut stmt = reader.prepare(
            "
            SELECT m.id, m.path, m.filename, m.media_type, m.size_bytes, m.created_at, m.modified_at,
                   m.thumbnail_path, m.duration_secs, m.source_root_id,
                   (SELECT GROUP_CONCAT(t.display_name, ',') FROM tags t
                      JOIN media_tags mt ON t.id = mt.tag_id
                     WHERE mt.media_id = m.id) AS tags,
                   sr.is_available, m.date_added
             FROM media m
             JOIN source_roots sr ON sr.id = m.source_root_id
             ORDER BY m.id
             LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt
            .query_map(params![limit, offset], |row| {
                let media_type_str: String = row.get(3)?;
                let media_type = crate::events::MediaType::from_db_str(&media_type_str)
                    .unwrap_or(crate::events::MediaType::Image);
                let tags: Option<String> = row.get(10)?;
                let is_available: bool = row.get(11)?;
                Ok(crate::events::UiMediaItem {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    filename: row.get(2)?,
                    tags: tags.unwrap_or_default(),
                    thumbnail_path: row.get::<_, Option<String>>(7)?.unwrap_or_default(),
                    duration_secs: row.get::<_, Option<i64>>(8)?.unwrap_or(-1),
                    media_type,
                    size_bytes: row.get(4)?,
                    created_at: row.get(5)?,
                    date_added: row.get(12)?,
                    modified_at: row.get(6)?,
                    is_offline: !is_available,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Retrieves a single media entry with its tags concatenated by commas.
    pub fn get_media_with_tags_by_path(
        &self,
        path: &str,
    ) -> Result<Option<(MediaItem, String)>, DbError> {
        let reader = self.lock_reader()?;
        let mut stmt = reader.prepare(
            "
            SELECT m.id, m.path, m.filename, m.source_root_id, m.media_type, m.size_bytes, m.created_at, m.modified_at,
                   GROUP_CONCAT(t.display_name, ',') as tags,
                   m.thumbnail_path, m.duration_secs, m.date_added
             FROM media m
             LEFT JOIN media_tags mt ON m.id = mt.media_id
             LEFT JOIN tags t ON mt.tag_id = t.id
             WHERE m.path = ?1
             GROUP BY m.id",
        )?;

        match stmt.query_row([path], |row| {
            let media_type_str: String = row.get(4)?;
            let media_type = crate::events::MediaType::from_db_str(&media_type_str)
                .unwrap_or(crate::events::MediaType::Image);

            let media = MediaRow {
                id: row.get(0)?,
                path: row.get(1)?,
                filename: row.get(2)?,
                source_root_id: row.get(3)?,
                media_type,
                size_bytes: row.get(5)?,
                created_at: row.get(6)?,
                modified_at: row.get(7)?,
                date_added: row.get(11)?,
                thumbnail_path: row.get(9)?,
                duration_secs: row.get(10)?,
            };
            let tags: String = row.get(8).unwrap_or_default();
            Ok((media.into(), tags))
        }) {
            Ok(media) => Ok(Some(media)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
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
}

#[cfg(test)]
mod tests {
    use super::Database;
    use crate::db::{MediaEntry, TagIdentity};
    use crate::events::MediaType;

    /// Builds a simple single-level tag identity (relative path == display name).
    fn tag_ident(root_id: i64, name: &str) -> TagIdentity {
        TagIdentity {
            source_root_id: root_id,
            relative_folder_path: name.to_string(),
            display_name: name.to_string(),
            display_path: name.to_string(),
        }
    }

    #[test]
    fn media_upsert_and_tags() {
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root("/media", "/media").unwrap();

        let entry = MediaEntry {
            path: "/media/Travel/Japan/photo.jpg".into(),
            relative_path: "Travel/Japan/photo.jpg".into(),
            canonical_identity: "/media/Travel/Japan/photo.jpg".into(),
            filename: "photo.jpg".into(),
            source_root_id: root_id,
            media_type: MediaType::Image,
            size_bytes: 1024,
            created_at: Some(1000),
            modified_at: 1000,
        };

        let media_id = {
            let writer = db.writer.lock().unwrap();
            db.upsert_media_inner(&writer, &entry, 1).unwrap()
        };
        assert!(media_id > 0);

        // Upsert same path again — should return same id.
        let media_id_2 = {
            let writer = db.writer.lock().unwrap();
            db.upsert_media_inner(&writer, &entry, 1).unwrap()
        };
        assert_eq!(media_id, media_id_2);

        // Set tags.
        let tags = vec![tag_ident(root_id, "Travel"), tag_ident(root_id, "Japan")];
        db.sync_tags_for_media(media_id, &tags).unwrap();

        let tag_rows = db.get_all_tags_with_counts().unwrap();
        assert_eq!(tag_rows.len(), 2);

        let names: Vec<&str> = tag_rows.iter().map(|t| t.display_name.as_str()).collect();
        assert!(names.contains(&"Travel"));
        assert!(names.contains(&"Japan"));

        // Replace tags — old ones removed.
        let new_tags = vec![tag_ident(root_id, "Travel"), tag_ident(root_id, "2023")];
        db.sync_tags_for_media(media_id, &new_tags).unwrap();
        let tag_rows = db.get_all_tags_with_counts().unwrap();
        assert_eq!(tag_rows.len(), 2);

        let names: Vec<&str> = tag_rows.iter().map(|t| t.display_name.as_str()).collect();
        assert!(names.contains(&"Travel"));
        assert!(names.contains(&"2023"));
        assert!(!names.contains(&"Japan"));
    }

    #[test]
    fn get_media_with_tags_by_path_returns_single_row() {
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root("/media", "/media").unwrap();

        let entry = MediaEntry {
            path: "/media/Travel/Japan/photo.jpg".into(),
            relative_path: "Travel/Japan/photo.jpg".into(),
            canonical_identity: "/media/Travel/Japan/photo.jpg".into(),
            filename: "photo.jpg".into(),
            source_root_id: root_id,
            media_type: MediaType::Image,
            size_bytes: 1024,
            created_at: Some(1000),
            modified_at: 2000,
        };

        db.upsert_media_batch(
            &[(
                entry,
                vec![tag_ident(root_id, "Travel"), tag_ident(root_id, "Japan")],
            )],
            1,
        )
        .unwrap();

        let (media, tags) = db
            .get_media_with_tags_by_path("/media/Travel/Japan/photo.jpg")
            .unwrap()
            .unwrap();

        assert_eq!(media.filename, "photo.jpg");
        assert_eq!(media.source_root_id, root_id);
        assert!(tags.split(',').any(|tag| tag == "Travel"));
        assert!(tags.split(',').any(|tag| tag == "Japan"));
        assert!(
            db.get_media_with_tags_by_path("/media/missing.jpg")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn media_removal_by_path() {
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root("/media", "/media").unwrap();

        let entry = MediaEntry {
            path: "/media/photo.jpg".into(),
            relative_path: "photo.jpg".into(),
            canonical_identity: "/media/photo.jpg".into(),
            filename: "photo.jpg".into(),
            source_root_id: root_id,
            media_type: MediaType::Image,
            size_bytes: 512,
            created_at: None,
            modified_at: 1000,
        };
        {
            let writer = db.writer.lock().unwrap();
            db.upsert_media_inner(&writer, &entry, 1).unwrap();
        }

        assert!(db.remove_media_by_path("/media/photo.jpg").unwrap());
        assert!(!db.remove_media_by_path("/media/photo.jpg").unwrap());
    }

    #[test]
    fn cascade_delete_on_source_root_removal() {
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root("/media", "/media").unwrap();

        let entry = MediaEntry {
            path: "/media/photo.jpg".into(),
            relative_path: "photo.jpg".into(),
            canonical_identity: "/media/photo.jpg".into(),
            filename: "photo.jpg".into(),
            source_root_id: root_id,
            media_type: MediaType::Image,
            size_bytes: 512,
            created_at: None,
            modified_at: 1000,
        };
        let media_id = {
            let writer = db.writer.lock().unwrap();
            db.upsert_media_inner(&writer, &entry, 1).unwrap()
        };
        db.sync_tags_for_media(media_id, &[tag_ident(root_id, "root_tag")])
            .unwrap();

        // Removing source root cascades to media and media_tags.
        db.remove_source_root(root_id).unwrap();
        let paths = db.get_all_paths_for_root(root_id).unwrap();
        assert!(paths.is_empty());

        // Tags are now owned by the source root (FK cascade), so they are removed
        // with it — cleanup finds no orphans left behind.
        let cleaned = db.cleanup_orphaned_tags().unwrap();
        assert_eq!(cleaned, 0);
        assert!(db.get_all_tags_with_counts().unwrap().is_empty());
    }

    #[test]
    fn get_all_paths_for_root() {
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root("/media", "/media").unwrap();

        for name in &["a.jpg", "b.png", "c.mp4"] {
            let entry = MediaEntry {
                path: format!("/media/{name}"),
                relative_path: (*name).into(),
                canonical_identity: format!("/media/{name}"),
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
            };
            {
                let writer = db.writer.lock().unwrap();
                db.upsert_media_inner(&writer, &entry, 1).unwrap();
            }
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
            relative_path: "photo.jpg".into(),
            canonical_identity: "/media/photo.jpg".into(),
            filename: "photo.jpg".into(),
            source_root_id: root_id,
            media_type: MediaType::Image,
            size_bytes: 512,
            created_at: None,
            modified_at: 1000,
        };
        let media_id = {
            let writer = db.writer.lock().unwrap();
            db.upsert_media_inner(&writer, &entry, 1).unwrap()
        };
        // Guarded on the row's current modified_at (1000); a matching write lands.
        let updated = db
            .set_thumbnail_success(media_id, "cachekey123", "/cache/thumb_123.jpg", 1000, None)
            .unwrap();
        assert!(updated, "a matching thumbnail write updates the row");

        let status = db.get_thumbnail_status(media_id).unwrap().unwrap();
        assert_eq!(status.cache_key.as_deref(), Some("cachekey123"));
        assert_eq!(
            status.thumbnail_path.as_deref(),
            Some("/cache/thumb_123.jpg")
        );
        assert!(!status.stale);
        assert!(status.failure.is_none());

        // A write for a stale modified_at is dropped (guard fails).
        let stale_write = db
            .set_thumbnail_success(media_id, "other", "/cache/other.jpg", 999, None)
            .unwrap();
        assert!(
            !stale_write,
            "a write for an outdated modified_at is dropped"
        );
    }

    #[test]
    fn modifying_a_file_marks_thumbnail_stale_and_keeps_old_thumbnail() {
        // T-1: a content change flags the thumbnail stale but must NOT blank the
        // existing thumbnail — the old one stays visible until an explicit
        // regeneration succeeds.
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root("/media", "/media").unwrap();

        let entry = MediaEntry {
            path: "/media/photo.jpg".into(),
            relative_path: "photo.jpg".into(),
            canonical_identity: "/media/photo.jpg".into(),
            filename: "photo.jpg".into(),
            source_root_id: root_id,
            media_type: MediaType::Image,
            size_bytes: 512,
            created_at: None,
            modified_at: 1000,
        };
        let media_id = {
            let writer = db.writer.lock().unwrap();
            db.upsert_media_inner(&writer, &entry, 1).unwrap()
        };
        db.set_thumbnail_success(media_id, "keyA", "/cache/keyA.jpg", 1000, None)
            .unwrap();

        let before = db.get_thumbnail_status(media_id).unwrap().unwrap();
        assert!(!before.stale);
        assert_eq!(before.thumbnail_path.as_deref(), Some("/cache/keyA.jpg"));

        // Re-index the same path with a newer modified_at (the file changed).
        let modified = MediaEntry {
            modified_at: 2000,
            ..entry
        };
        {
            let writer = db.writer.lock().unwrap();
            db.upsert_media_inner(&writer, &modified, 2).unwrap();
        }

        let after = db.get_thumbnail_status(media_id).unwrap().unwrap();
        assert!(after.stale, "modification flags the thumbnail stale");
        assert_eq!(
            after.thumbnail_path.as_deref(),
            Some("/cache/keyA.jpg"),
            "the old thumbnail is kept, not blanked"
        );
        assert_eq!(
            after.cache_key.as_deref(),
            Some("keyA"),
            "the old cache key is kept for the still-visible thumbnail"
        );

        // The stale row is listed for explicit regeneration.
        assert!(
            db.list_media_needing_thumbnail_regen()
                .unwrap()
                .contains(&media_id)
        );
    }

    #[test]
    fn remove_media_and_descendants_with_wildcards() {
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root("/media", "/media").unwrap();

        let entry1 = MediaEntry {
            path: "/media/My%Folder/photo1.jpg".into(),
            relative_path: "My%Folder/photo1.jpg".into(),
            canonical_identity: "/media/My%Folder/photo1.jpg".into(),
            filename: "photo1.jpg".into(),
            source_root_id: root_id,
            media_type: MediaType::Image,
            size_bytes: 100,
            created_at: None,
            modified_at: 1000,
        };
        let entry2 = MediaEntry {
            path: "/media/My1Folder/photo2.jpg".into(),
            relative_path: "My1Folder/photo2.jpg".into(),
            canonical_identity: "/media/My1Folder/photo2.jpg".into(),
            filename: "photo2.jpg".into(),
            source_root_id: root_id,
            media_type: MediaType::Image,
            size_bytes: 100,
            created_at: None,
            modified_at: 1000,
        };
        let entry3 = MediaEntry {
            path: "/media/My%Folder".into(),
            relative_path: "My%Folder".into(),
            canonical_identity: "/media/My%Folder".into(),
            filename: "My%Folder".into(),
            source_root_id: root_id,
            media_type: MediaType::Image,
            size_bytes: 100,
            created_at: None,
            modified_at: 1000,
        };

        {
            let writer = db.writer.lock().unwrap();
            db.upsert_media_inner(&writer, &entry1, 1).unwrap();
            db.upsert_media_inner(&writer, &entry2, 1).unwrap();
            db.upsert_media_inner(&writer, &entry3, 1).unwrap();
        }

        let removed = db.remove_media_and_descendants("/media/My%Folder").unwrap();
        assert_eq!(removed.len(), 2);
        assert!(removed.contains(&"/media/My%Folder/photo1.jpg".to_string()));
        assert!(removed.contains(&"/media/My%Folder".to_string()));

        let remaining = db.get_all_paths_for_root(root_id).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0], "/media/My1Folder/photo2.jpg");
    }

    // ── A-3 schema guarantees ───────────────────────────────────────

    #[test]
    fn unique_relative_path_per_root_is_enforced() {
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root("/media", "/media").unwrap();

        let first = MediaEntry {
            path: "/media/a.jpg".into(),
            relative_path: "same.jpg".into(),
            canonical_identity: "/media/a.jpg".into(),
            filename: "a.jpg".into(),
            source_root_id: root_id,
            media_type: MediaType::Image,
            size_bytes: 1,
            created_at: None,
            modified_at: 1000,
        };
        // Same (source_root_id, relative_path), but distinct path and
        // canonical_identity so only the (root, relative_path) unique index can fire.
        let second = MediaEntry {
            path: "/media/b.jpg".into(),
            relative_path: "same.jpg".into(),
            canonical_identity: "/media/b.jpg".into(),
            filename: "b.jpg".into(),
            source_root_id: root_id,
            media_type: MediaType::Image,
            size_bytes: 1,
            created_at: None,
            modified_at: 1000,
        };

        let writer = db.writer.lock().unwrap();
        db.upsert_media_inner(&writer, &first, 1).unwrap();
        let err = db.upsert_media_inner(&writer, &second, 1).unwrap_err();
        assert!(
            err.to_string().contains("relative_path"),
            "expected a (source_root_id, relative_path) unique violation, got: {err}"
        );
    }

    #[test]
    fn unique_canonical_identity_is_enforced() {
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root("/media", "/media").unwrap();

        let first = MediaEntry {
            path: "/media/a.jpg".into(),
            relative_path: "a.jpg".into(),
            canonical_identity: "/media/shared-target.jpg".into(),
            filename: "a.jpg".into(),
            source_root_id: root_id,
            media_type: MediaType::Image,
            size_bytes: 1,
            created_at: None,
            modified_at: 1000,
        };
        // Distinct path and relative_path, but the same canonical_identity, so
        // only the canonical_identity unique index can fire.
        let second = MediaEntry {
            path: "/media/b.jpg".into(),
            relative_path: "b.jpg".into(),
            canonical_identity: "/media/shared-target.jpg".into(),
            filename: "b.jpg".into(),
            source_root_id: root_id,
            media_type: MediaType::Image,
            size_bytes: 1,
            created_at: None,
            modified_at: 1000,
        };

        let writer = db.writer.lock().unwrap();
        db.upsert_media_inner(&writer, &first, 1).unwrap();
        let err = db.upsert_media_inner(&writer, &second, 1).unwrap_err();
        assert!(
            err.to_string().contains("canonical_identity"),
            "expected a canonical_identity unique violation, got: {err}"
        );
    }

    #[test]
    fn required_media_indexes_exist() {
        let db = Database::open_in_memory().unwrap();
        let reader = db.reader.lock().unwrap();

        // Collect the ordered column list of every index on the media table.
        let index_names: Vec<String> = reader
            .prepare("SELECT name FROM pragma_index_list('media')")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let mut index_columns: Vec<Vec<String>> = Vec::new();
        for name in &index_names {
            // Index names are internal constants, so inlining is injection-safe and
            // avoids the bound-parameter restrictions of table-valued pragmas.
            let sql = format!("SELECT name FROM pragma_index_info('{name}') ORDER BY seqno");
            let cols: Vec<String> = reader
                .prepare(&sql)
                .unwrap()
                .query_map([], |row| row.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            index_columns.push(cols);
        }

        // True when an index whose columns exactly match `target` (in order) exists.
        let has_index = |target: &[&str]| {
            index_columns
                .iter()
                .any(|cols| cols.iter().map(String::as_str).eq(target.iter().copied()))
        };

        for target in [
            &["date_added"][..],
            &["size_bytes"][..],
            &["media_type"][..],
            &["last_accessed_at"][..],
            &["source_root_id", "scan_generation"][..],
        ] {
            assert!(
                has_index(target),
                "missing required media index on {target:?}"
            );
        }
    }

    fn insert_media(db: &Database, root_id: i64, count: usize) {
        let writer = db.writer.lock().unwrap();
        for i in 0..count {
            let entry = MediaEntry {
                path: format!("/media/f{i}.jpg"),
                relative_path: format!("f{i}.jpg"),
                canonical_identity: format!("/media/f{i}.jpg"),
                filename: format!("f{i}.jpg"),
                source_root_id: root_id,
                media_type: MediaType::Image,
                size_bytes: 1,
                created_at: None,
                modified_at: 1000 + i as i64,
            };
            db.upsert_media_inner(&writer, &entry, 1).unwrap();
        }
    }

    #[test]
    fn hydration_reads_media_in_bounded_chunks_not_one_full_reload() {
        // B-2 sub-step c: hydration streams bounded windows via
        // `hydrate_media_chunk` rather than the removed full-library reload.
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root("/media", "/media").unwrap();
        insert_media(&db, root_id, 5);

        assert_eq!(db.count_media().unwrap(), 5);

        // Walk the store in chunks of 2 — each read is bounded to the limit.
        let c0 = db.hydrate_media_chunk(0, 2).unwrap();
        let c1 = db.hydrate_media_chunk(2, 2).unwrap();
        let c2 = db.hydrate_media_chunk(4, 2).unwrap();
        let c3 = db.hydrate_media_chunk(6, 2).unwrap();

        assert_eq!(c0.len(), 2);
        assert_eq!(c1.len(), 2);
        assert_eq!(c2.len(), 1);
        assert!(c3.is_empty(), "reads past the end return an empty chunk");

        // The chunks reassemble into the whole store, in stable id order, with no
        // duplicates or gaps — i.e. equivalent coverage to the old full reload.
        let mut ids: Vec<i64> = c0.iter().chain(&c1).chain(&c2).map(|m| m.id).collect();
        ids.dedup();
        assert_eq!(ids.len(), 5);
    }

    #[test]
    fn hydration_read_does_not_probe_filesystem_or_write_db() {
        // The root path does not exist on disk, yet the DB marks it available.
        // A hydration read is pure: it must report offline state from the DB
        // (is_offline == false here), never re-probe the filesystem, and never
        // write the database (B-2: FetchData is a read-only hydration).
        let db = Database::open_in_memory().unwrap();
        let root_id = db
            .add_source_root("/nonexistent-root", "/nonexistent-root")
            .unwrap();
        insert_media(&db, root_id, 1);

        let chunk = db.hydrate_media_chunk(0, 10).unwrap();
        assert_eq!(chunk.len(), 1);
        assert!(
            !chunk[0].is_offline,
            "offline state must come from the DB, not a fresh fs probe"
        );

        // Availability is untouched: hydration performed no write side effect.
        let roots = db.list_source_roots().unwrap();
        assert!(roots.iter().all(|r| r.is_available));
    }
}
