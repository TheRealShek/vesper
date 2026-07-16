//! Persistence for `scan_errors` (A-4).
//!
//! A scan records one row per path that failed, keyed by
//! `(source_root_id, scan_generation, path)`. Each scan replaces the error set
//! for the scope it covers: it clears the root's (or scanned subtree's) errors
//! via [`Database::clear_scan_errors_for_root`] /
//! [`Database::clear_scan_errors_in_subtree`], then records the current run's
//! failures. A path that now succeeds is therefore absent from the new failures
//! and its error disappears, while a path that still fails is re-recorded.

use super::{Database, DbError, ScanErrorEntry};
use rusqlite::params;

impl Database {
    // ── Scan errors ─────────────────────────────────────────────────

    /// Records scan failures. `last_seen` is stamped by the database. Re-recording
    /// the same `(source_root_id, scan_generation, path)` refreshes its category,
    /// message, and timestamp rather than failing on the primary key.
    pub fn record_scan_errors(&self, errors: &[ScanErrorEntry]) -> Result<(), DbError> {
        if errors.is_empty() {
            return Ok(());
        }
        let writer = self.lock_writer()?;
        let tx = writer.unchecked_transaction()?;
        for err in errors {
            tx.execute(
                "INSERT INTO scan_errors
                    (source_root_id, scan_generation, path, category, message, last_seen)
                 VALUES (?1, ?2, ?3, ?4, ?5, strftime('%s', 'now'))
                 ON CONFLICT(source_root_id, scan_generation, path) DO UPDATE SET
                   category  = excluded.category,
                   message   = excluded.message,
                   last_seen = excluded.last_seen",
                params![
                    err.source_root_id,
                    err.scan_generation,
                    err.path,
                    err.category,
                    err.message,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Clears every recorded error for a source root. A full scan calls this and
    /// then records its own failures, so a path that now succeeds (and is absent
    /// from the new failures) no longer surfaces an error. This replace-on-scan
    /// approach is used rather than a generation cutoff because the scan
    /// generation is derived from persisted media and can regress when a scan
    /// persists nothing.
    pub fn clear_scan_errors_for_root(&self, source_root_id: i64) -> Result<usize, DbError> {
        let writer = self.lock_writer()?;
        let count = writer.execute(
            "DELETE FROM scan_errors WHERE source_root_id = ?1",
            [source_root_id],
        )?;
        Ok(count)
    }

    /// Like [`Self::clear_scan_errors_for_root`] but scoped to a scanned subtree,
    /// so errors elsewhere in the root are left untouched.
    pub fn clear_scan_errors_in_subtree(
        &self,
        source_root_id: i64,
        subtree_prefix: &str,
    ) -> Result<usize, DbError> {
        let writer = self.lock_writer()?;
        let like_pattern = format!("{}%", subtree_prefix);
        let count = writer.execute(
            "DELETE FROM scan_errors WHERE source_root_id = ?1 AND path LIKE ?2",
            params![source_root_id, like_pattern],
        )?;
        Ok(count)
    }

    /// Returns the distinct paths that currently have a recorded scan error,
    /// most-recently-seen first. Used by the UI's scan-error surface.
    pub fn get_scan_error_paths(&self) -> Result<Vec<String>, DbError> {
        let reader = self.lock_reader()?;
        let mut stmt = reader
            .prepare("SELECT path FROM scan_errors GROUP BY path ORDER BY MAX(last_seen) DESC")?;
        let paths = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(paths)
    }

    /// Number of error rows for a source root. Test-only helper.
    #[cfg(test)]
    pub fn count_scan_errors_for_root(&self, source_root_id: i64) -> Result<i64, DbError> {
        let reader = self.reader.lock().unwrap();
        let count: i64 = reader.query_row(
            "SELECT COUNT(*) FROM scan_errors WHERE source_root_id = ?1",
            [source_root_id],
            |row| row.get(0),
        )?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(root_id: i64, generation: i64, path: &str) -> ScanErrorEntry {
        ScanErrorEntry {
            source_root_id: root_id,
            scan_generation: generation,
            path: path.to_string(),
            category: "unreadable".to_string(),
            message: "boom".to_string(),
        }
    }

    #[test]
    fn record_and_read_back() {
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root("/media", "/media").unwrap();

        db.record_scan_errors(&[entry(root_id, 1, "/media/bad.jpg")])
            .unwrap();

        assert_eq!(db.count_scan_errors_for_root(root_id).unwrap(), 1);
        assert_eq!(
            db.get_scan_error_paths().unwrap(),
            vec!["/media/bad.jpg".to_string()]
        );
    }

    #[test]
    fn re_recording_same_key_updates_in_place() {
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root("/media", "/media").unwrap();

        db.record_scan_errors(&[entry(root_id, 1, "/media/bad.jpg")])
            .unwrap();
        db.record_scan_errors(&[entry(root_id, 1, "/media/bad.jpg")])
            .unwrap();

        assert_eq!(db.count_scan_errors_for_root(root_id).unwrap(), 1);
    }

    #[test]
    fn clearing_root_removes_all_its_errors() {
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root("/media", "/media").unwrap();

        db.record_scan_errors(&[
            entry(root_id, 1, "/media/a.jpg"),
            entry(root_id, 1, "/media/b.jpg"),
        ])
        .unwrap();

        let removed = db.clear_scan_errors_for_root(root_id).unwrap();
        assert_eq!(removed, 2);
        assert_eq!(db.count_scan_errors_for_root(root_id).unwrap(), 0);
    }

    #[test]
    fn clearing_subtree_leaves_other_errors() {
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root("/media", "/media").unwrap();

        db.record_scan_errors(&[
            entry(root_id, 1, "/media/sub/a.jpg"),
            entry(root_id, 1, "/media/other/b.jpg"),
        ])
        .unwrap();

        let removed = db
            .clear_scan_errors_in_subtree(root_id, "/media/sub")
            .unwrap();
        assert_eq!(removed, 1);
        assert_eq!(
            db.get_scan_error_paths().unwrap(),
            vec!["/media/other/b.jpg".to_string()]
        );
    }

    #[test]
    fn removed_root_cascades_to_scan_errors() {
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root("/media", "/media").unwrap();
        db.record_scan_errors(&[entry(root_id, 1, "/media/bad.jpg")])
            .unwrap();

        db.remove_source_root(root_id).unwrap();
        assert_eq!(db.count_scan_errors_for_root(root_id).unwrap(), 0);
    }
}
