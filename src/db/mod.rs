//! SQLite database for media entries, tags, and source roots.
//!
//! This module has zero GTK imports. All raw SQL is contained here —
//! the rest of the application uses the typed `Database` interface.

mod error;
mod media;
mod migrations;
mod models;
mod roots;
mod scan_errors;
mod schema;
mod search;
mod settings;
mod tags;

pub use error::DbError;
pub use models::*;

use rusqlite::{Connection, OpenFlags};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

/// Handle to the application's SQLite database.
///
/// All database access goes through this type. No raw SQL
/// is used outside the `db` module.
pub struct Database {
    // WAL allows concurrent reads without blocking writes, so we maintain separate reader/writer connections.
    pub(crate) writer: Mutex<Connection>,
    pub(crate) reader: Mutex<Connection>,
}

impl Database {
    pub(crate) fn lock_writer(&self) -> Result<MutexGuard<'_, Connection>, DbError> {
        self.writer
            .lock()
            .map_err(|_| DbError::MutexPoisoned("writer"))
    }

    pub(crate) fn lock_reader(&self) -> Result<MutexGuard<'_, Connection>, DbError> {
        self.reader
            .lock()
            .map_err(|_| DbError::MutexPoisoned("reader"))
    }

    /// Reruns pending schema migrations, then transactionally clears only
    /// database-derived library records. Source roots, settings, session state,
    /// and the migration ledger are preserved for the subsequent full re-index.
    pub fn prepare_library_index_rebuild(&self) -> Result<usize, DbError> {
        let mut writer = self.lock_writer()?;
        let migrations_applied = migrations::run(&mut writer)?;
        let transaction = writer.transaction()?;
        transaction.execute("DELETE FROM media", [])?;
        transaction.execute("DELETE FROM tags", [])?;
        transaction.execute("DELETE FROM scan_errors", [])?;
        transaction.commit()?;
        Ok(migrations_applied)
    }

    #[cfg(test)]
    pub fn forget_schema_migration_for_test(&self, version: i64) -> Result<(), DbError> {
        let writer = self.lock_writer()?;
        writer.execute(
            "DELETE FROM schema_migrations WHERE version = ?1",
            [version],
        )?;
        Ok(())
    }

    /// Opens (or creates) a database file at the given path and initializes the schema.
    pub fn open(path: &Path) -> Result<Self, DbError> {
        let mut writer = Connection::open(path)?;
        writer.execute_batch("PRAGMA journal_mode=WAL;")?;
        schema::initialize(&mut writer)?;

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

        let mut writer = Connection::open_with_flags(
            &uri,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_URI,
        )?;
        writer.execute_batch("PRAGMA journal_mode=WAL;")?;
        schema::initialize(&mut writer)?;

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_memory() {
        let db = Database::open_in_memory().unwrap();
        let roots = db.list_source_roots().unwrap();
        assert!(roots.is_empty());
    }
}
