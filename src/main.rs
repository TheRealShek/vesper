pub mod config;
mod events;
mod db;
mod index;
mod scan;
mod ui;
mod thumbnail;
pub mod state;

use libadwaita as adw;
use libadwaita::prelude::*;
use libadwaita::{glib, gtk, Application};
use std::sync::{Arc, Mutex};
use crate::events::AppEvent;
use crate::ui::window::UiEvent;

fn main() -> glib::ExitCode {
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Failed to create tokio runtime: {}", e);
            return glib::ExitCode::FAILURE;
        }
    };
    let _guard = rt.enter();

    std::panic::set_hook(Box::new(move |info| {
        eprintln!("Panic occurred: {:?}", info);
        glib::MainContext::default().invoke(move || {
            let dialog = adw::MessageDialog::builder()
                .heading("Unexpected Error")
                .body("An unexpected error occurred. The application will close.")
                .build();
            if let Some(app) = gtk::gio::Application::default().and_downcast::<gtk::Application>() {
                if let Some(win) = app.active_window() {
                    dialog.set_transient_for(Some(&win));
                }
            }
            dialog.add_response("close", "Close");
            dialog.connect_response(None, move |_, _| {
                std::process::exit(1);
            });
            dialog.present();
        });
    }));

    let app = Application::builder()
        .application_id("io.github.TheRealShek.vesper")
        .build();

    let (app_tx, mut app_rx) = tokio::sync::mpsc::unbounded_channel::<AppEvent>();
    let (ui_tx, ui_rx) = tokio::sync::mpsc::unbounded_channel::<UiEvent>();
    let (thumb_tx, thumb_rx) = tokio::sync::mpsc::unbounded_channel::<crate::thumbnail::ThumbnailRequest>();

    let db_path_res = std::env::current_dir().map(|d| d.join(crate::config::DB_NAME));
    let db_res = db_path_res.and_then(|p| crate::db::Database::open(&p).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
    let state_res = std::panic::catch_unwind(|| crate::state::AppState::load());

    let (db_arc, state_arc) = match (db_res, state_res) {
        (Ok(db), Ok(state)) => {
            (Arc::new(Mutex::new(db)), Arc::new(Mutex::new(state)))
        }
        _ => {
            eprintln!("Failed to load database or state");
            app.connect_activate(move |app| {
                let dialog = adw::MessageDialog::builder()
                    .heading("Unexpected Error")
                    .body("An unexpected error occurred. The application will close.")
                    .build();
                dialog.add_response("close", "Close");
                let app_clone = app.clone();
                dialog.connect_response(None, move |_, _| {
                    app_clone.quit();
                    std::process::exit(1);
                });
                dialog.present();
            });
            return app.run();
        }
    };

    // Start Thumbnail Worker
    crate::thumbnail::start_thumbnail_worker(db_arc.clone(), thumb_rx, ui_tx.clone());

    // Backend Loop
    let db_backend = db_arc.clone();
    let ui_tx_backend = ui_tx.clone();
    let state_backend = state_arc.clone();
    let app_tx_backend = app_tx.clone();
    
    let (debouncer_tx, debouncer_rx) = std::sync::mpsc::channel();
    let app_tx_watcher = app_tx.clone();
    std::thread::spawn(move || {
        while let Ok(res) = debouncer_rx.recv() {
            match res {
                Ok(events) => {
                    let events: Vec<notify_debouncer_mini::DebouncedEvent> = events;
                    for event in events {
                        let path = event.path;
                        if path.file_name().and_then(|n| n.to_str()) == Some(".galleryignore") {
                            if let Some(parent) = path.parent() {
                                let _ = app_tx_watcher.send(crate::events::AppEvent::RescanSubtree(parent.to_path_buf()));
                            }
                        } else {
                            let kind = if path.exists() {
                                crate::events::ChangeKind::Modified
                            } else {
                                crate::events::ChangeKind::Deleted
                            };
                            let _ = app_tx_watcher.send(crate::events::AppEvent::FileChanged(path, kind));
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Watcher error: {:?}", e);
                }
            }
        }
    });
    
    tokio::spawn(async move {
        let mut debouncer = notify_debouncer_mini::new_debouncer(
            std::time::Duration::from_millis(crate::config::FS_DEBOUNCE_MS),
            debouncer_tx
        ).unwrap_or_else(|e| panic!("Failed to create debouncer: {}", e));

        while let Some(event) = app_rx.recv().await {
            match event {
                AppEvent::AddSourceRoot(path) => {
                    let mut success = false;
                    if let Ok(guard) = db_backend.lock() {
                        if guard.add_source_root(&path).is_ok() {
                            let _ = debouncer.watcher().watch(std::path::Path::new(&path), notify_debouncer_mini::notify::RecursiveMode::Recursive);
                            success = true;
                        } else {
                            let _ = ui_tx_backend.send(UiEvent::ShowError(format!("Failed to add directory: {}", path)));
                        }
                    } else {
                        let _ = ui_tx_backend.send(UiEvent::FatalError("Database lock poisoned".to_string()));
                    }
                    if success {
                        let (root_as_tag, global_rules) = match state_backend.lock() {
                            Ok(s) => (s.backend.root_as_tag, s.backend.global_ignore_rules.clone()),
                            Err(_) => (false, vec![]),
                        };
                        let db_c2 = db_backend.clone();
                        let ui_c2 = ui_tx_backend.clone();
                        if let Ok(res) = crate::scan::run_scan(std::path::PathBuf::from(path.clone()), db_c2, global_rules, root_as_tag).await {
                            let _ = ui_c2.send(UiEvent::ScanCompleted(res.failed_paths.len(), res.failed_paths));
                        }
                    }
                }
                AppEvent::RemoveSourceRoot(id) => {
                    if let Ok(guard) = db_backend.lock() {
                        if let Some(root) = guard.list_source_roots().unwrap_or_default().into_iter().find(|r| r.id == id) {
                            let _ = debouncer.watcher().unwatch(std::path::Path::new(&root.path));
                        }
                        if guard.remove_source_root(id).is_ok() {
                            let _ = guard.cleanup_orphaned_tags();
                        }
                    }
                    let _ = app_tx_backend.send(AppEvent::FetchData);
                }
                AppEvent::UpdateSettings(backend_state) => {
                    if let Ok(mut state) = state_backend.lock() {
                        state.backend = backend_state;
                        let _ = state.save();
                    }
                }
                AppEvent::RescanRoots => {
                    let mut roots_to_scan = Vec::new();
                    if let Ok(guard) = db_backend.lock() {
                        if let Ok(roots) = guard.list_source_roots() {
                            roots_to_scan = roots.into_iter().map(|r| r.path).collect();
                        }
                    }
                    let (root_as_tag, global_rules) = match state_backend.lock() {
                        Ok(s) => (s.backend.root_as_tag, s.backend.global_ignore_rules.clone()),
                        Err(_) => (false, vec![]),
                    };
                    for path in roots_to_scan {
                        let db_c2 = db_backend.clone();
                        let ui_c2 = ui_tx_backend.clone();
                        let rules = global_rules.clone();
                        if let Ok(res) = crate::scan::run_scan(std::path::PathBuf::from(path.clone()), db_c2, rules, root_as_tag).await {
                            let _ = ui_c2.send(UiEvent::ScanCompleted(res.failed_paths.len(), res.failed_paths));
                        }
                    }
                }
                AppEvent::RescanSubtree(path) => {
                    let (root_as_tag, global_rules) = match state_backend.lock() {
                        Ok(s) => (s.backend.root_as_tag, s.backend.global_ignore_rules.clone()),
                        Err(_) => (false, vec![]),
                    };
                    let db_c2 = db_backend.clone();
                    let ui_c2 = ui_tx_backend.clone();
                    let app_tx_c2 = app_tx_backend.clone();
                    tokio::spawn(async move {
                        if let Ok(res) = crate::scan::run_subtree_scan(path, db_c2, global_rules, root_as_tag).await {
                            let _ = ui_c2.send(UiEvent::ScanCompleted(res.failed_paths.len(), res.failed_paths));
                            let _ = app_tx_c2.send(AppEvent::FetchData);
                        }
                    });
                }
                AppEvent::FileChanged(path, kind) => {
                    let db_g = db_backend.clone();
                    let state_g = state_backend.clone();
                    let app_tx_c2 = app_tx_backend.clone();
                    tokio::task::spawn_blocking(move || {
                        if kind == crate::events::ChangeKind::Deleted {
                            if let Ok(db) = db_g.lock() {
                                let _ = db.remove_media_by_path(path.to_str().unwrap_or(""));
                                let _ = db.cleanup_orphaned_tags();
                            }
                        } else {
                            let mut should_process = false;
                            let mut root_id = 0;
                            let mut root_path_str = String::new();
                            let mut root_as_tag = false;
                            let mut global_patterns = Vec::new();

                            if let Ok(db) = db_g.lock() {
                                if let Ok(roots) = db.list_source_roots() {
                                    if let Some(root) = roots.iter().find(|r| path.starts_with(&r.path)) {
                                        root_id = root.id;
                                        root_path_str = root.path.clone();
                                        if let Ok(s) = state_g.lock() {
                                            root_as_tag = s.backend.root_as_tag;
                                            global_patterns = s.backend.global_ignore_rules.clone();
                                        }
                                        should_process = true;
                                    }
                                }
                            }

                            if should_process {
                                let root_path = std::path::Path::new(&root_path_str);
                                let global_rules = crate::index::ignore_rules::build_global_rules(&global_patterns)
                                    .unwrap_or_else(|_| ignore::gitignore::GitignoreBuilder::new("/").build().unwrap());
                                
                                let mut ignore_stack = Vec::new();
                                let mut current = root_path.to_path_buf();
                                
                                if let Ok(Some(rules)) = crate::index::ignore_rules::load_directory_rules(&current) {
                                    ignore_stack.push(rules);
                                }
                                
                                if let Ok(rel) = path.parent().unwrap_or(&path).strip_prefix(root_path) {
                                    for comp in rel.components() {
                                        current.push(comp);
                                        if let Ok(Some(rules)) = crate::index::ignore_rules::load_directory_rules(&current) {
                                            ignore_stack.push(rules);
                                        }
                                    }
                                }
                                
                                if !crate::index::ignore_rules::is_ignored(&path, false, &ignore_stack, &global_rules) {
                                    if let Some(media_type) = crate::index::media::classify(&path) {
                                        if let Ok(metadata) = std::fs::metadata(&path) {
                                            let discovered = crate::events::DiscoveredMedia {
                                                path: path.clone(),
                                                media_type,
                                                size_bytes: metadata.len() as u64,
                                                modified: metadata.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                                                created: metadata.created().ok(),
                                            };
                                            let _ = crate::scan::process_single_file(&discovered, root_path, root_id, root_as_tag, db_g.clone());
                                        }
                                    }
                                }
                            }
                        }
                        let _ = app_tx_c2.send(AppEvent::FetchData);
                    });
                }
                AppEvent::FetchData => {
                    if let Ok(db_g) = db_backend.lock() {
                        let roots = db_g.list_source_roots().unwrap_or_default();
                        let mut offline_roots = std::collections::HashSet::new();
                        let mut offline_count = 0;
                        
                        for root in &roots {
                            let path = std::path::Path::new(&root.path);
                            let is_avail = path.exists();
                            if !is_avail {
                                offline_roots.insert(root.id);
                                offline_count += 1;
                            } else {
                                let _ = debouncer.watcher().watch(path, notify_debouncer_mini::notify::RecursiveMode::Recursive);
                            }
                            if root.is_available != is_avail {
                                let _ = db_g.set_source_root_available(root.id, is_avail);
                            }
                        }
                        
                        let tags: Vec<crate::events::UiTag> = db_g.get_all_tags_with_counts().unwrap_or_default()
                            .into_iter()
                            .map(|t| crate::events::UiTag { name: t.name, file_count: t.file_count })
                            .collect();
                        let db_media = db_g.get_all_media_with_tags().unwrap_or_default();
                        let media: Vec<crate::events::UiMediaItem> = db_media
                            .into_iter()
                            .map(|(row, mtags)| crate::events::UiMediaItem {
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
                                is_offline: offline_roots.contains(&row.source_root_id),
                            })
                            .collect();
                        let has_roots = !roots.is_empty();
                        let roots_list = roots.into_iter().map(|r| crate::events::UiSourceRoot {
                            id: r.id,
                            name: std::path::Path::new(&r.path).file_name().unwrap_or_default().to_string_lossy().to_string(),
                            path: r.path,
                            is_available: !offline_roots.contains(&r.id),
                        }).collect();
                        let _ = ui_tx_backend.send(UiEvent::DataFetched { tags, media, roots: roots_list, has_roots });
                        
                        if offline_count > 0 {
                            let _ = ui_tx_backend.send(UiEvent::RootsOffline(offline_count));
                        } else {
                            let _ = ui_tx_backend.send(UiEvent::RootsOffline(0));
                        }
                    }
                }
            }
        }
    });

    let ui_rx_cell = std::rc::Rc::new(std::cell::RefCell::new(Some(ui_rx)));

    app.connect_activate(move |app| {
        let rx = ui_rx_cell.borrow_mut().take();
        if let Some(rx) = rx {
            ui::build_ui(
                app,
                app_tx.clone(),
                ui_tx.clone(),
                rx,
                thumb_tx.clone(),
                state_arc.clone()
            );
        }
    });

    let ret = app.run();
    rt.shutdown_background();
    ret
}
