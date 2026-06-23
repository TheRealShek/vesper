use super::{Database, DbError, TagWithCount};
use rusqlite::{Connection, params};

impl Database {
    // ── Tags ────────────────────────────────────────────────────────

    /// Replaces all tags for a media entry with the given set of tag names.
    /// Creates new tag rows as needed. Runs inside a transaction.
    #[cfg(test)]
    pub fn sync_tags_for_media(&self, media_id: i64, tag_names: &[String]) -> Result<(), DbError> {
        let writer = self.writer.lock().unwrap();
        let tx = writer.unchecked_transaction()?;
        self.sync_tags_inner(&tx, media_id, tag_names)?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn sync_tags_inner(
        &self,
        writer: &Connection,
        media_id: i64,
        tag_names: &[String],
    ) -> Result<(), DbError> {
        // Deleting all and reinserting is simpler and often faster than diffing small sets of tags.
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

    /// Returns all tags with file counts, sorted by count descending (spec section 6).
    pub fn get_all_tags_with_counts(&self) -> Result<Vec<TagWithCount>, DbError> {
        let reader = self.reader.lock().unwrap();
        let mut stmt = reader.prepare(
            "SELECT name, (SELECT COUNT(*) FROM media_tags WHERE tag_id = id) as file_count FROM tags WHERE file_count > 0 ORDER BY file_count DESC, name ASC",
        )?;
        let tags = stmt
            .query_map([], |row| {
                Ok(TagWithCount {
                    name: row.get(0)?,
                    file_count: row.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(tags)
    }

    // Global cleanup runs after a full scan when we are certain all tag relationships are up to date.
    /// Removes orphaned tags that have no media associations.
    pub fn cleanup_orphaned_tags(&self) -> Result<usize, DbError> {
        let writer = self.writer.lock().unwrap();
        let changed = writer.execute(
            "DELETE FROM tags WHERE id NOT IN (SELECT DISTINCT tag_id FROM media_tags)",
            [],
        )?;
        Ok(changed)
    }

    // Scoped cleanup runs after subtree scans to avoid deleting tags still used outside the subtree but not yet scanned.
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
