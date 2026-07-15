use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::db::Database;

/// `session_state` key holding the JSON-serialized [`UiState`].
const UI_STATE_KEY: &str = "ui_state";
/// `settings` key holding the JSON-serialized [`BackendState`].
const BACKEND_STATE_KEY: &str = "backend_state";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UiState {
    pub zoom_level: f64,
    pub sort_order: String,
    pub active_tags: Vec<String>,
    pub tag_filter_mode: String,
    // Stored as an item index because pixel offsets are unstable across sessions if window width or zoom level changes.
    pub scroll_position: u32,
    pub window_width: i32,
    pub window_height: i32,
    pub window_maximized: bool,
    // search_query is deliberately omitted: search is session-only to avoid confusing the user on next launch.
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BackendState {
    pub root_as_tag: bool,
    pub global_ignore_rules: Vec<String>,
}

// Separating UI and backend state allows the backend config to be safely sent across threads on UpdateSettings,
// while keeping UI state strictly confined to the main GTK thread.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AppState {
    #[serde(flatten)]
    pub ui: UiState,
    #[serde(flatten)]
    pub backend: BackendState,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            zoom_level: 2.0,
            sort_order: "Date modified (newest first)".to_string(),
            active_tags: Vec::new(),
            tag_filter_mode: "OR".to_string(),
            scroll_position: 0,
            window_width: 1024,
            window_height: 768,
            window_maximized: false,
        }
    }
}

impl Default for BackendState {
    fn default() -> Self {
        Self {
            root_as_tag: false,
            global_ignore_rules: crate::index::ignore_rules::DEFAULT_GLOBAL_PATTERNS
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }
}

impl AppState {
    pub fn config_path() -> PathBuf {
        let mut path = libadwaita::glib::user_config_dir();
        path.push("vesper");
        path
    }

    /// Path to the legacy `state.json`. Retained only for the one-time import
    /// in [`Self::load`]; the app no longer reads or writes it after A-5.
    pub fn state_file_path() -> PathBuf {
        let mut path = Self::config_path();
        path.push("state.json");
        path
    }

    /// Loads persisted state from SQLite. On the first launch after A-5, any
    /// values in a legacy `state.json` are imported into the tables (once) and
    /// the file is then left untouched on disk.
    ///
    /// A missing or corrupt value falls back to the default so the app always
    /// opens.
    pub fn load(db: &Database) -> Self {
        Self::import_legacy_if_needed(db, &Self::state_file_path());

        let backend = db
            .get_setting(BACKEND_STATE_KEY)
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str::<BackendState>(&json).ok())
            .unwrap_or_default();

        let ui = db
            .get_session_state(UI_STATE_KEY)
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str::<UiState>(&json).ok())
            .unwrap_or_default();

        Self { ui, backend }
    }

    /// Persists state into the `settings` and `session_state` tables.
    pub fn save(&self, db: &Database) -> Result<(), anyhow::Error> {
        db.set_setting(BACKEND_STATE_KEY, &serde_json::to_string(&self.backend)?)?;
        db.set_session_state(UI_STATE_KEY, &serde_json::to_string(&self.ui)?)?;
        Ok(())
    }

    /// One-time migration (A-5): if both persistence tables are empty and a
    /// legacy `state.json` exists and parses, copy its values into SQLite. The
    /// file is intentionally left on disk — it is simply no longer used.
    fn import_legacy_if_needed(db: &Database, legacy_path: &Path) {
        // Only import into a pristine store; once anything has been saved the
        // tables are authoritative and the legacy file is ignored.
        let tables_empty =
            db.settings_is_empty().unwrap_or(false) && db.session_state_is_empty().unwrap_or(false);
        if !tables_empty {
            return;
        }

        let Ok(contents) = std::fs::read_to_string(legacy_path) else {
            return;
        };
        let Ok(legacy) = serde_json::from_str::<AppState>(&contents) else {
            return;
        };
        // Best-effort: a failed import just leaves the defaults in place.
        let _ = legacy.save(db);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn one_time_import_from_legacy_state_json() {
        let db = Database::open_in_memory().unwrap();
        assert!(db.settings_is_empty().unwrap());
        assert!(db.session_state_is_empty().unwrap());

        // A legacy state.json with clearly non-default values.
        let legacy = AppState {
            ui: UiState {
                zoom_level: 3.0,
                sort_order: "Filename (A → Z)".to_string(),
                active_tags: vec!["Travel".to_string(), "2023".to_string()],
                tag_filter_mode: "AND".to_string(),
                scroll_position: 42,
                window_width: 1600,
                window_height: 900,
                window_maximized: true,
            },
            backend: BackendState {
                root_as_tag: true,
                global_ignore_rules: vec!["*.tmp".to_string()],
            },
        };

        let dir = TempDir::new().unwrap();
        let legacy_path = dir.path().join("state.json");
        std::fs::write(&legacy_path, serde_json::to_string_pretty(&legacy).unwrap()).unwrap();

        AppState::import_legacy_if_needed(&db, &legacy_path);

        // The values now live in SQLite.
        assert!(!db.settings_is_empty().unwrap());
        assert!(!db.session_state_is_empty().unwrap());

        let backend: BackendState =
            serde_json::from_str(&db.get_setting(BACKEND_STATE_KEY).unwrap().unwrap()).unwrap();
        assert!(backend.root_as_tag);
        assert_eq!(backend.global_ignore_rules, vec!["*.tmp".to_string()]);

        let ui: UiState =
            serde_json::from_str(&db.get_session_state(UI_STATE_KEY).unwrap().unwrap()).unwrap();
        assert_eq!(ui.zoom_level, 3.0);
        assert_eq!(
            ui.active_tags,
            vec!["Travel".to_string(), "2023".to_string()]
        );
        assert_eq!(ui.scroll_position, 42);
        assert!(ui.window_maximized);

        // The legacy file is left on disk, just no longer used.
        assert!(legacy_path.exists());
    }

    #[test]
    fn import_skipped_when_store_not_empty() {
        let db = Database::open_in_memory().unwrap();
        // Pre-seed the settings table so the store is authoritative.
        db.set_setting(
            BACKEND_STATE_KEY,
            "{\"root_as_tag\":false,\"global_ignore_rules\":[]}",
        )
        .unwrap();

        let legacy = AppState {
            ui: UiState::default(),
            backend: BackendState {
                root_as_tag: true,
                ..Default::default()
            },
        };
        let dir = TempDir::new().unwrap();
        let legacy_path = dir.path().join("state.json");
        std::fs::write(&legacy_path, serde_json::to_string(&legacy).unwrap()).unwrap();

        AppState::import_legacy_if_needed(&db, &legacy_path);

        // The pre-existing store must not be overwritten by the legacy file.
        let backend: BackendState =
            serde_json::from_str(&db.get_setting(BACKEND_STATE_KEY).unwrap().unwrap()).unwrap();
        assert!(!backend.root_as_tag);
    }
}
