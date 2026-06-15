use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
#[derive(Debug, Serialize, Deserialize, Clone)]
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

impl Default for AppState {
    fn default() -> Self {
        Self {
            ui: UiState::default(),
            backend: BackendState {
                root_as_tag: false,
                global_ignore_rules: crate::index::ignore_rules::DEFAULT_GLOBAL_PATTERNS
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            },
        }
    }
}

impl AppState {
    pub fn config_path() -> PathBuf {
        let mut path = libadwaita::glib::user_config_dir();
        path.push("vesper");
        path
    }

    pub fn state_file_path() -> PathBuf {
        let mut path = Self::config_path();
        path.push("state.json");
        path
    }

    pub fn load() -> Self {
        let path = Self::state_file_path();
        if path.exists()
            && let Ok(contents) = std::fs::read_to_string(&path)
            && let Ok(state) = serde_json::from_str(&contents)
        {
            return state;
        }
        // Silently falling back to Default ensures a corrupt or missing state file never prevents the app from opening.
        Self::default()
    }

    pub fn save(&self) -> Result<(), anyhow::Error> {
        let path = Self::state_file_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = serde_json::to_string_pretty(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }
}
