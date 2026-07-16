use super::{Database, DbError, TagIdentity, TagWithCount};
use rusqlite::{Connection, params};

impl Database {
    // ── Tags ────────────────────────────────────────────────────────

    /// Replaces all tags for a media entry with the given set of tag identities.
    /// Creates new tag rows as needed. Runs inside a transaction.
    #[cfg(test)]
    pub fn sync_tags_for_media(&self, media_id: i64, tags: &[TagIdentity]) -> Result<(), DbError> {
        let writer = self.writer.lock().unwrap();
        let tx = writer.unchecked_transaction()?;
        self.sync_tags_inner(&tx, media_id, tags)?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn sync_tags_inner(
        &self,
        writer: &Connection,
        media_id: i64,
        tags: &[TagIdentity],
    ) -> Result<(), DbError> {
        // Deleting all and reinserting is simpler and often faster than diffing small sets of tags.
        writer.execute("DELETE FROM media_tags WHERE media_id = ?1", [media_id])?;

        for tag in tags {
            let display_name_search =
                super::search_normalization::normalize_search_text(&tag.display_name);
            let display_path_search =
                super::search_normalization::normalize_search_text(&tag.display_path);
            // Identity is (source_root_id, relative_folder_path); display fields
            // are refreshed on conflict so rows written under an older
            // display-path scheme converge on the current derivation (NEW-5).
            writer.execute(
                "INSERT INTO tags (source_root_id, relative_folder_path, display_name,
                                   display_path, display_name_search, display_path_search)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(source_root_id, relative_folder_path) DO UPDATE SET
                   display_name        = excluded.display_name,
                   display_path        = excluded.display_path,
                   display_name_search = excluded.display_name_search,
                   display_path_search = excluded.display_path_search",
                params![
                    tag.source_root_id,
                    tag.relative_folder_path,
                    tag.display_name,
                    tag.display_path,
                    display_name_search,
                    display_path_search,
                ],
            )?;

            let tag_id: i64 = writer.query_row(
                "SELECT id FROM tags WHERE source_root_id = ?1 AND relative_folder_path = ?2",
                params![tag.source_root_id, tag.relative_folder_path],
                |row| row.get(0),
            )?;

            writer.execute(
                "INSERT INTO media_tags (media_id, tag_id) VALUES (?1, ?2)",
                params![media_id, tag_id],
            )?;
        }

        Ok(())
    }

    /// Returns all tags with file counts in the canonical sidebar order.
    pub fn get_all_tags_with_counts(&self) -> Result<Vec<TagWithCount>, DbError> {
        let reader = self.lock_reader()?;
        let mut stmt = reader.prepare(
            "SELECT id, source_root_id, relative_folder_path, display_name, display_path,
                    (SELECT COUNT(*) FROM media_tags mt
                       JOIN media m ON m.id = mt.media_id
                       JOIN source_roots sr ON sr.id = m.source_root_id
                      WHERE mt.tag_id = tags.id AND sr.is_available = 1) as file_count
             FROM tags
             WHERE file_count > 0
             ORDER BY file_count DESC,
                      display_name COLLATE NOCASE ASC,
                      source_root_id ASC,
                      relative_folder_path ASC",
        )?;
        let tags = stmt
            .query_map([], |row| {
                Ok(TagWithCount {
                    id: row.get(0)?,
                    source_root_id: row.get(1)?,
                    relative_folder_path: row.get(2)?,
                    display_name: row.get(3)?,
                    display_path: row.get(4)?,
                    file_count: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(tags)
    }

    /// Removes orphaned tags that have no media associations.
    pub fn cleanup_orphaned_tags(&self) -> Result<usize, DbError> {
        let writer = self.lock_writer()?;
        let changed = writer.execute(
            "DELETE FROM tags WHERE id NOT IN (SELECT DISTINCT tag_id FROM media_tags)",
            [],
        )?;
        Ok(changed)
    }
}
