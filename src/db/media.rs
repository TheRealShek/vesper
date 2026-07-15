use super::{Database, DbError, MediaEntry, MediaItem, MediaRow, TagIdentity};
use rusqlite::{Connection, params};

impl Database {
    // ── Media ───────────────────────────────────────────────────────

    pub(crate) fn upsert_media_inner(
        &self,
        writer: &Connection,
        entry: &MediaEntry,
        scan_gen: i64,
    ) -> Result<i64, DbError> {
        writer.execute(
            "INSERT INTO media (path, filename, source_root_id, media_type,
                                size_bytes, created_at, modified_at, thumbnail_path, duration_secs, indexed_at, scan_generation)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, NULL, strftime('%s', 'now'), ?8)
             ON CONFLICT(path) DO UPDATE SET
               filename       = excluded.filename,
               source_root_id = excluded.source_root_id,
               media_type     = excluded.media_type,
               size_bytes     = excluded.size_bytes,
               created_at     = excluded.created_at,
               -- Conditionally null thumbnail_path on upsert to detect and regenerate stale thumbnails on file change.
               thumbnail_path = CASE WHEN modified_at != excluded.modified_at THEN NULL ELSE thumbnail_path END,
               modified_at    = excluded.modified_at,
               scan_generation= excluded.scan_generation",
            params![
                entry.path,
                entry.filename,
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
        let writer = self.writer.lock().unwrap();
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

    // Separate from subtree removal because full scans authoritative-delete across the whole root.
    /// Removes all media entries for the given source_root_id that have a strictly older scan_generation.
    pub fn remove_stale_media(&self, source_root_id: i64, scan_gen: i64) -> Result<usize, DbError> {
        let writer = self.writer.lock().unwrap();
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
        let writer = self.writer.lock().unwrap();
        let like_pattern = format!("{}%", subtree_prefix);
        let count = writer.execute(
            "DELETE FROM media WHERE source_root_id = ?1 AND path LIKE ?2 AND scan_generation < ?3",
            params![source_root_id, like_pattern, scan_gen],
        )?;
        Ok(count)
    }

    /// Removes a media entry and any descendants if it was a directory. Returns the paths of deleted items.
    pub fn remove_media_and_descendants(&self, path: &str) -> Result<Vec<String>, DbError> {
        let writer = self.writer.lock().unwrap();
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

    /// Retrieves all media entries with their tags concatenated by commas.
    pub fn get_all_media_with_tags(&self) -> Result<Vec<(MediaItem, String)>, DbError> {
        let reader = self.reader.lock().unwrap();
        let mut stmt = reader.prepare(
            "
            SELECT m.id, m.path, m.filename, m.source_root_id, m.media_type, m.size_bytes, m.created_at, m.modified_at,
                   GROUP_CONCAT(t.display_name, ',') as tags,
                   m.thumbnail_path, m.duration_secs
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
                    thumbnail_path: row.get(9)?,
                    duration_secs: row.get(10)?,
                };
                let tags: String = row.get(8).unwrap_or_default();
                Ok((media.into(), tags))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Retrieves a single media entry with its tags concatenated by commas.
    pub fn get_media_with_tags_by_path(
        &self,
        path: &str,
    ) -> Result<Option<(MediaItem, String)>, DbError> {
        let reader = self.reader.lock().unwrap();
        let mut stmt = reader.prepare(
            "
            SELECT m.id, m.path, m.filename, m.source_root_id, m.media_type, m.size_bytes, m.created_at, m.modified_at,
                   GROUP_CONCAT(t.display_name, ',') as tags,
                   m.thumbnail_path, m.duration_secs
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
        db.set_thumbnail_and_duration(
            media_id,
            "/media/photo1.jpg",
            100,
            "/cache/thumb_123.jpg",
            None,
        )
        .unwrap();
    }

    #[test]
    fn remove_media_and_descendants_with_wildcards() {
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root("/media", "/media").unwrap();

        let entry1 = MediaEntry {
            path: "/media/My%Folder/photo1.jpg".into(),
            filename: "photo1.jpg".into(),
            source_root_id: root_id,
            media_type: MediaType::Image,
            size_bytes: 100,
            created_at: None,
            modified_at: 1000,
        };
        let entry2 = MediaEntry {
            path: "/media/My1Folder/photo2.jpg".into(),
            filename: "photo2.jpg".into(),
            source_root_id: root_id,
            media_type: MediaType::Image,
            size_bytes: 100,
            created_at: None,
            modified_at: 1000,
        };
        let entry3 = MediaEntry {
            path: "/media/My%Folder".into(),
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
}
