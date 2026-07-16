use super::{Database, DbError, SourceRoot, system_time_to_epoch};
use rusqlite::params;
use std::time::SystemTime;

impl Database {
    // ── Source roots ────────────────────────────────────────────────

    pub fn add_source_root(&self, path: &str, display_path: &str) -> Result<i64, DbError> {
        let added_at = system_time_to_epoch(SystemTime::now());
        let writer = self.lock_writer()?;
        writer.execute(
            "INSERT INTO source_roots (path, display_path, added_at, is_available) VALUES (?1, ?2, ?3, 1)",
            params![path, display_path, added_at],
        )?;
        Ok(writer.last_insert_rowid())
    }

    /// Removes a source root and all its media (via ON DELETE CASCADE).
    pub fn remove_source_root(&self, id: i64) -> Result<(), DbError> {
        let writer = self.lock_writer()?;
        writer.execute("DELETE FROM source_roots WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Lists all source roots ordered by creation time.
    pub fn list_source_roots(&self) -> Result<Vec<SourceRoot>, DbError> {
        let reader = self.lock_reader()?;
        let mut stmt =
            reader.prepare("SELECT id, path, display_path, is_available FROM source_roots")?;
        let roots = stmt
            .query_map([], |row| {
                Ok(SourceRoot {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    display_path: row.get(2)?,
                    is_available: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(roots)
    }

    pub fn find_source_root_by_path(&self, path: &str) -> Result<Option<SourceRoot>, DbError> {
        let reader = self.lock_reader()?;
        match reader.query_row(
            "SELECT id, path, display_path, is_available FROM source_roots WHERE path = ?1",
            [path],
            |row| {
                Ok(SourceRoot {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    display_path: row.get(2)?,
                    is_available: row.get(3)?,
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
        let writer = self.lock_writer()?;
        writer.execute(
            "UPDATE source_roots SET is_available = ?1 WHERE id = ?2",
            params![available as i64, id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Database;

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
}
