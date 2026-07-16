//! Key/value persistence for the `settings` and `session_state` tables (A-5).
//!
//! `settings` holds global configuration (root-as-tag, serialized ignore
//! rules); `session_state` holds per-restart UI state (filters, sort, zoom,
//! scroll position, window size). Both are simple `(key, value)` string stores —
//! callers serialize their own payloads (see [`crate::state::AppState`]).

use super::{Database, DbError};
use rusqlite::{OptionalExtension, params};

impl Database {
    // ── settings ────────────────────────────────────────────────────

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, DbError> {
        self.get_kv("settings", key)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), DbError> {
        self.set_kv("settings", key, value)
    }

    pub fn settings_is_empty(&self) -> Result<bool, DbError> {
        self.table_is_empty("settings")
    }

    // ── session_state ───────────────────────────────────────────────

    pub fn get_session_state(&self, key: &str) -> Result<Option<String>, DbError> {
        self.get_kv("session_state", key)
    }

    pub fn set_session_state(&self, key: &str, value: &str) -> Result<(), DbError> {
        self.set_kv("session_state", key, value)
    }

    pub fn session_state_is_empty(&self) -> Result<bool, DbError> {
        self.table_is_empty("session_state")
    }

    // ── shared helpers ──────────────────────────────────────────────
    //
    // `table` is always an internal constant ("settings" / "session_state"), so
    // inlining it into the SQL is injection-safe; the key/value are still bound.

    fn get_kv(&self, table: &str, key: &str) -> Result<Option<String>, DbError> {
        let reader = self.lock_reader()?;
        let sql = format!("SELECT value FROM {table} WHERE key = ?1");
        Ok(reader.query_row(&sql, [key], |row| row.get(0)).optional()?)
    }

    fn set_kv(&self, table: &str, key: &str, value: &str) -> Result<(), DbError> {
        let writer = self.lock_writer()?;
        let sql = format!(
            "INSERT INTO {table} (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value"
        );
        writer.execute(&sql, params![key, value])?;
        Ok(())
    }

    fn table_is_empty(&self, table: &str) -> Result<bool, DbError> {
        let reader = self.lock_reader()?;
        let sql = format!("SELECT COUNT(*) FROM {table}");
        let count: i64 = reader.query_row(&sql, [], |row| row.get(0))?;
        Ok(count == 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_roundtrip_and_emptiness() {
        let db = Database::open_in_memory().unwrap();
        assert!(db.settings_is_empty().unwrap());
        assert!(db.get_setting("root_as_tag").unwrap().is_none());

        db.set_setting("root_as_tag", "true").unwrap();
        assert!(!db.settings_is_empty().unwrap());
        assert_eq!(
            db.get_setting("root_as_tag").unwrap().as_deref(),
            Some("true")
        );

        // Upsert replaces the value in place.
        db.set_setting("root_as_tag", "false").unwrap();
        assert_eq!(
            db.get_setting("root_as_tag").unwrap().as_deref(),
            Some("false")
        );
    }

    #[test]
    fn session_state_roundtrip_and_emptiness() {
        let db = Database::open_in_memory().unwrap();
        assert!(db.session_state_is_empty().unwrap());

        db.set_session_state("ui", "{\"zoom\":2}").unwrap();
        assert!(!db.session_state_is_empty().unwrap());
        assert_eq!(
            db.get_session_state("ui").unwrap().as_deref(),
            Some("{\"zoom\":2}")
        );
    }
}
