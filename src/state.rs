use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::db::Database;

/// `session_state` key holding the JSON-serialized [`UiState`].
const UI_STATE_KEY: &str = "ui_state";
/// `settings` key holding the JSON-serialized [`BackendState`].
const BACKEND_STATE_KEY: &str = "backend_state";

/// Stable scroll anchor (A-6). A raw item index is meaningless once the result
/// set is reordered or filtered between sessions, so instead of an index we pin
/// the *identity* of the item at the top of the viewport plus enough context to
/// resolve it: the media id, the pixel offset of the viewport top within that
/// item's row, and a hash of the sort/filter context the anchor was captured in.
#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct ScrollAnchor {
    /// Id of the media item at the top of the viewport. `None` means "no
    /// meaningful anchor" (fresh session, or scrolled to the very top), which
    /// resolves to top-of-grid.
    pub media_id: Option<i64>,
    /// Vertical pixel offset of the viewport top within the anchor item's row.
    /// Only meaningful when the restore context matches [`Self::context_hash`],
    /// since a different sort/filter can place the item in a different row.
    pub offset_within_cell: f64,
    /// Hash of the ordering context (sort order + active tags + filter mode)
    /// the anchor was captured under. Lets the restorer tell whether the stored
    /// offset is still trustworthy; resolution by id works regardless.
    pub context_hash: u64,
}

impl ScrollAnchor {
    /// Order-independent hash of the ordering context. Two sessions whose sort
    /// order, filter mode, and (unordered) active-tag set match produce the same
    /// hash; anything else produces a different one.
    pub fn context_hash(sort_order: &str, active_tags: &[String], tag_filter_mode: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        sort_order.hash(&mut hasher);
        tag_filter_mode.hash(&mut hasher);
        // Sort the tags so selection order doesn't perturb the hash.
        let mut tags: Vec<&str> = active_tags.iter().map(String::as_str).collect();
        tags.sort_unstable();
        tags.hash(&mut hasher);
        hasher.finish()
    }

    /// Resolves the anchor against the current result set (media ids in display
    /// order) and returns the index of the anchored item, or `None` when the
    /// item is absent — deleted, filtered out, or on an offline root — in which
    /// case the caller falls back to top-of-grid. Resolution is purely by id, so
    /// it survives a sort/filter reordering between sessions.
    pub fn resolve(&self, ordered_ids: &[i64]) -> Option<usize> {
        let target = self.media_id?;
        ordered_ids.iter().position(|&id| id == target)
    }
}

/// A persisted tag filter (A-7). Since A-2 a tag's identity is
/// `source_root_id + relative_folder_path`, not a bare display string — two
/// folders sharing a basename are different tags. Persisting the identity lets
/// hydration reconcile the filter against the live source-root set: a filter
/// whose root was removed is discarded, one whose root is offline is suspended.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct TagFilter {
    pub source_root_id: i64,
    pub relative_folder_path: String,
    /// Display name kept alongside the identity so the UI can render and match
    /// the filter without a second lookup; the identity remains authoritative.
    pub display_name: String,
}

/// Availability of a source root at reconciliation time. A root that was
/// *removed* is absent from the map entirely (see [`reconcile_tag_filters`]);
/// this enum only distinguishes roots that still exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootStatus {
    Online,
    Offline,
}

/// Outcome of reconciling persisted tag filters against the live library.
/// Filters whose root was removed are dropped entirely (neither list).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReconciledFilters {
    /// Root online and tag still present — applied to the grid this session.
    pub active: Vec<TagFilter>,
    /// Root offline — hidden from active filtering but retained so the filter
    /// restores automatically once the root returns and is rescanned.
    pub suspended: Vec<TagFilter>,
}

impl ReconciledFilters {
    /// The set to persist back to `session_state`: active ∪ suspended. Discarded
    /// filters are intentionally excluded so a removed root does not linger.
    pub fn to_persist(&self) -> Vec<TagFilter> {
        self.active
            .iter()
            .chain(self.suspended.iter())
            .cloned()
            .collect()
    }

    /// Display names of the active filters, in order — the strings the existing
    /// display-name-keyed filter pipeline consumes.
    pub fn active_display_names(&self) -> Vec<String> {
        self.active.iter().map(|f| f.display_name.clone()).collect()
    }
}

/// Reconciles persisted tag filters against the current library state at
/// hydration (A-7).
///
/// - `roots`: every source root that currently exists, mapped to its status.
///   A root that was removed is simply absent from this map.
/// - `online_tags`: identities `(source_root_id, relative_folder_path)` of tags
///   visible in the online library.
///
/// Rules (02 §8 / 04 §10): a filter whose root was removed is discarded; one
/// whose root is offline is suspended; otherwise, if the tag still exists it is
/// active, and if the root is online but the folder is gone the filter is
/// discarded.
pub fn reconcile_tag_filters(
    persisted: &[TagFilter],
    roots: &std::collections::HashMap<i64, RootStatus>,
    online_tags: &std::collections::HashSet<(i64, String)>,
) -> ReconciledFilters {
    let mut out = ReconciledFilters::default();
    for filter in persisted {
        match roots.get(&filter.source_root_id) {
            // Root removed → discard silently.
            None => {}
            // Root offline → suspend (kept for auto-restore on return).
            Some(RootStatus::Offline) => out.suspended.push(filter.clone()),
            // Root online → active if the tag still exists, else the folder is
            // gone and the filter is discarded.
            Some(RootStatus::Online) => {
                let key = (filter.source_root_id, filter.relative_folder_path.clone());
                if online_tags.contains(&key) {
                    out.active.push(filter.clone());
                }
            }
        }
    }
    out
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UiState {
    pub zoom_level: f64,
    pub sort_order: String,
    // Persisted as identity-qualified filters (A-7), reconciled against the live
    // source roots at hydration so offline-root filters suspend and removed-root
    // filters are discarded.
    pub active_tags: Vec<TagFilter>,
    pub tag_filter_mode: String,
    // A stable identity anchor rather than a raw item index: an index is
    // meaningless once the result set is reordered/filtered across sessions.
    pub scroll_anchor: ScrollAnchor,
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
            scroll_anchor: ScrollAnchor::default(),
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
                active_tags: vec![
                    TagFilter {
                        source_root_id: 1,
                        relative_folder_path: "Travel".to_string(),
                        display_name: "Travel".to_string(),
                    },
                    TagFilter {
                        source_root_id: 1,
                        relative_folder_path: "Travel/2023".to_string(),
                        display_name: "2023".to_string(),
                    },
                ],
                tag_filter_mode: "AND".to_string(),
                scroll_anchor: ScrollAnchor {
                    media_id: Some(42),
                    offset_within_cell: 7.5,
                    context_hash: 99,
                },
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
        assert_eq!(ui.active_tags.len(), 2);
        assert_eq!(ui.active_tags[0].relative_folder_path, "Travel");
        assert_eq!(ui.active_tags[1].display_name, "2023");
        assert_eq!(ui.scroll_anchor.media_id, Some(42));
        assert_eq!(ui.scroll_anchor.offset_within_cell, 7.5);
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

    #[test]
    fn anchor_resolves_by_identity_after_sort_order_changes() {
        // Captured last session under a "newest first" order where item 30 sat
        // at the top of the viewport, three items down the result set.
        let anchor = ScrollAnchor {
            media_id: Some(30),
            offset_within_cell: 12.0,
            context_hash: ScrollAnchor::context_hash("Date modified (newest first)", &[], "OR"),
        };
        let newest_first = [50_i64, 40, 30, 20, 10];
        assert_eq!(anchor.resolve(&newest_first), Some(2));

        // This session the sort flipped to "oldest first", so the very same
        // media id now lives at a different index. Resolving by identity must
        // follow the item to its new position rather than reusing the old index.
        let oldest_first = [10_i64, 20, 30, 40, 50];
        assert_eq!(anchor.resolve(&oldest_first), Some(2));
        // (Index 2 here is a coincidence of the symmetric data; prove it tracks
        // identity by using an asymmetric ordering too.)
        let reordered = [30_i64, 10, 50, 20, 40];
        assert_eq!(anchor.resolve(&reordered), Some(0));

        // The context hash changed with the sort order, signalling the caller
        // that the stored pixel offset is no longer authoritative.
        let new_context = ScrollAnchor::context_hash("Date modified (oldest first)", &[], "OR");
        assert_ne!(anchor.context_hash, new_context);
    }

    #[test]
    fn anchor_falls_back_to_top_when_item_missing() {
        // The anchored item was deleted / filtered out / lives on an offline
        // root, so it isn't in the current result set.
        let anchor = ScrollAnchor {
            media_id: Some(999),
            offset_within_cell: 4.0,
            context_hash: 0,
        };
        let current = [10_i64, 20, 30];
        assert_eq!(anchor.resolve(&current), None);

        // An anchor that was never set (fresh session) also resolves to the top.
        let empty = ScrollAnchor::default();
        assert_eq!(empty.resolve(&current), None);
    }

    // ── A-7: offline / removed tag-filter reconciliation ────────────────

    use std::collections::{HashMap, HashSet};

    fn filter(root: i64, folder: &str) -> TagFilter {
        TagFilter {
            source_root_id: root,
            relative_folder_path: folder.to_string(),
            display_name: folder.rsplit('/').next().unwrap_or(folder).to_string(),
        }
    }

    #[test]
    fn filter_discarded_when_root_removed() {
        let persisted = vec![filter(1, "Travel"), filter(2, "Work")];

        // Root 2 was removed entirely: it is absent from the roots map, and its
        // tag identity is nowhere in the online set.
        let mut roots = HashMap::new();
        roots.insert(1, RootStatus::Online);
        let mut online_tags = HashSet::new();
        online_tags.insert((1, "Travel".to_string()));

        let result = reconcile_tag_filters(&persisted, &roots, &online_tags);

        // Root 1's filter stays active; root 2's is silently discarded — neither
        // active nor suspended — so it is not persisted back.
        assert_eq!(result.active, vec![filter(1, "Travel")]);
        assert!(result.suspended.is_empty());
        assert_eq!(result.to_persist(), vec![filter(1, "Travel")]);
    }

    #[test]
    fn filter_suspended_when_root_offline() {
        let persisted = vec![filter(1, "Travel"), filter(2, "Work")];

        // Root 2 still exists but is offline; its media (and tag counts) are
        // hidden, so we must not drop the filter — we suspend it.
        let mut roots = HashMap::new();
        roots.insert(1, RootStatus::Online);
        roots.insert(2, RootStatus::Offline);
        let mut online_tags = HashSet::new();
        online_tags.insert((1, "Travel".to_string()));

        let result = reconcile_tag_filters(&persisted, &roots, &online_tags);

        assert_eq!(result.active, vec![filter(1, "Travel")]);
        assert_eq!(result.suspended, vec![filter(2, "Work")]);
        // Suspended filters are omitted from active filtering but retained in the
        // persisted set so they survive to the next session.
        assert_eq!(result.active_display_names(), vec!["Travel".to_string()]);
        assert_eq!(
            result.to_persist(),
            vec![filter(1, "Travel"), filter(2, "Work")]
        );
    }

    #[test]
    fn filter_auto_restores_when_root_comes_back_online() {
        let persisted = vec![filter(2, "Work")];

        // Session 1: root 2 is offline → the filter is suspended, but preserved.
        let mut roots = HashMap::new();
        roots.insert(2, RootStatus::Offline);
        let offline_result = reconcile_tag_filters(&persisted, &roots, &HashSet::new());
        assert!(offline_result.active.is_empty());
        assert_eq!(offline_result.suspended, vec![filter(2, "Work")]);
        let carried = offline_result.to_persist();
        assert_eq!(carried, vec![filter(2, "Work")]);

        // Session 2: root 2 has returned and been rescanned, so its tag is back
        // in the online set. Reconciling the carried-over filter reactivates it
        // automatically with no user action.
        let mut roots = HashMap::new();
        roots.insert(2, RootStatus::Online);
        let mut online_tags = HashSet::new();
        online_tags.insert((2, "Work".to_string()));
        let online_result = reconcile_tag_filters(&carried, &roots, &online_tags);
        assert_eq!(online_result.active, vec![filter(2, "Work")]);
        assert!(online_result.suspended.is_empty());
    }
}
