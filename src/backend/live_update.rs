use crate::db::Database;
use crate::events::ChannelSendExt;
use crate::state::AppState;
use crate::ui::window::UiEvent;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub fn process_file_changed(
    path: PathBuf,
    kind: crate::events::ChangeKind,
    db_backend: Arc<Database>,
    state_backend: Arc<Mutex<AppState>>,
    ui_tx_backend: tokio::sync::mpsc::Sender<UiEvent>,
    app_tx_backend: tokio::sync::mpsc::Sender<crate::events::AppEvent>,
) {
    if kind != crate::events::ChangeKind::Deleted && path.is_dir() {
        app_tx_backend.send_critical(crate::events::AppEvent::RescanSubtree(path));
        return;
    }

    let db_g = db_backend.clone();
    let state_g = state_backend.clone();
    let ui_c = ui_tx_backend.clone();
    tokio::task::spawn_blocking(move || {
        if kind == crate::events::ChangeKind::Deleted {
            let db = &*db_g;
            let path_str = path.to_string_lossy().to_string();
            let removed_paths = db
                .remove_media_and_descendants(&path_str)
                .unwrap_or_default();
            if !removed_paths.is_empty() {
                for p in removed_paths {
                    ui_c.send_critical(UiEvent::MediaRemoved(p));
                }
                let tags = db
                    .get_all_tags_with_counts()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|t| crate::events::UiTag {
                        id: t.id,
                        source_root_id: t.source_root_id,
                        relative_folder_path: t.relative_folder_path,
                        display_name: t.display_name,
                        display_path: t.display_path,
                        file_count: t.file_count,
                    })
                    .collect();
                ui_c.send_critical(UiEvent::TagsUpdated(tags));
            }
        } else {
            let mut should_process = false;
            let mut root_id = 0;
            let mut root_path_str = String::new();
            let mut root_as_tag = false;
            let mut global_patterns = Vec::new();

            let db = &*db_g;
            if let Ok(roots) = db.list_source_roots()
                && let Some(root) = roots
                    .iter()
                    .filter(|r| path.starts_with(&r.path))
                    .max_by_key(|r| std::path::Path::new(&r.path).components().count())
            {
                root_id = root.id;
                root_path_str = root.path.clone();
                if let Ok(s) = state_g.lock() {
                    root_as_tag = s.backend.root_as_tag;
                    global_patterns = s.backend.global_ignore_rules.clone();
                }
                should_process = true;
            }

            if should_process {
                let root_path = std::path::Path::new(&root_path_str);
                let global_rules =
                    match crate::index::ignore_rules::build_global_rules(&global_patterns) {
                        Ok(rules) => rules,
                        Err(_) => match ignore::gitignore::GitignoreBuilder::new("/").build() {
                            Ok(rules) => rules,
                            Err(e) => {
                                eprintln!("Failed to build empty ignore rules: {}", e);
                                return;
                            }
                        },
                    };

                let mut ignore_stack = Vec::new();
                let mut current = root_path.to_path_buf();

                if let Ok(Some(rules)) = crate::index::ignore_rules::load_directory_rules(&current)
                {
                    ignore_stack.push(rules);
                }

                if let Ok(rel) = path.parent().unwrap_or(&path).strip_prefix(root_path) {
                    for comp in rel.components() {
                        current.push(comp);
                        if let Ok(Some(rules)) =
                            crate::index::ignore_rules::load_directory_rules(&current)
                        {
                            ignore_stack.push(rules);
                        }
                    }
                }

                if !crate::index::ignore_rules::is_ignored(
                    &path,
                    false,
                    &ignore_stack,
                    &global_rules,
                ) && let Some(media_type) = crate::index::media::classify(&path)
                    && let Ok(metadata) = std::fs::metadata(&path)
                {
                    let discovered = crate::events::DiscoveredMedia {
                        path: path.clone(),
                        media_type,
                        size_bytes: metadata.len(),
                        modified: metadata
                            .modified()
                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                        created: metadata.created().ok(),
                    };
                    let _ = crate::scan::process_single_file(
                        &discovered,
                        root_path,
                        root_id,
                        root_as_tag,
                        db_g.clone(),
                    );
                    {
                        let db = &*db_g;
                        let path_str = path.to_string_lossy().to_string();
                        if let Ok(Some((row, mtags))) = db.get_media_with_tags_by_path(&path_str) {
                            let item = crate::events::UiMediaItem {
                                id: row.id,
                                path: row.path,
                                filename: row.filename,
                                tags: mtags,
                                thumbnail_path: row.thumbnail_path.unwrap_or_default(),
                                duration_secs: row.duration_secs.unwrap_or(-1),
                                media_type: row.media_type,
                                size_bytes: row.size_bytes,
                                created_at: row.created_at,
                                modified_at: row.modified_at,
                                is_offline: false,
                            };
                            ui_c.send_critical(UiEvent::MediaAdded(item));
                            let tags = db
                                .get_all_tags_with_counts()
                                .unwrap_or_default()
                                .into_iter()
                                .map(|t| crate::events::UiTag {
                                    id: t.id,
                                    source_root_id: t.source_root_id,
                                    relative_folder_path: t.relative_folder_path,
                                    display_name: t.display_name,
                                    display_path: t.display_path,
                                    file_count: t.file_count,
                                })
                                .collect();
                            ui_c.send_critical(UiEvent::TagsUpdated(tags));
                        }
                    }
                }
            }
        }
    });
}
