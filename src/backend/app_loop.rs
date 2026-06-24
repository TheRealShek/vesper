use crate::db::Database;
use crate::events::{AppEvent, ChannelSendExt};
use crate::state::AppState;
use crate::ui::window::UiEvent;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub fn start(
    mut app_rx: tokio::sync::mpsc::Receiver<AppEvent>,
    app_tx: tokio::sync::mpsc::Sender<AppEvent>,
    ui_tx: tokio::sync::mpsc::Sender<UiEvent>,
    db: Arc<Database>,
    state: Arc<Mutex<AppState>>,
    debouncer_tx: std::sync::mpsc::Sender<notify_debouncer_mini::DebounceEventResult>,
) {
    tokio::spawn(async move {
        let mut debouncer = match notify_debouncer_mini::new_debouncer(
            std::time::Duration::from_millis(crate::config::FS_DEBOUNCE_MS),
            debouncer_tx,
        ) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Failed to create debouncer: {}", e);
                std::process::exit(1);
            }
        };

        let pending_scans =
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashSet::new()));
        let mut watched_roots: HashSet<PathBuf> = HashSet::new();

        let mut initial_scan_done = false;
        let fetch_in_progress = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        while let Some(event) = app_rx.recv().await {
            match event {
                AppEvent::AddSourceRoot(display_path) => {
                    // Canonical path ensures stable uniqueness; display_path preserves user input format.
                    let canonical_path = match std::path::Path::new(&display_path).canonicalize() {
                        Ok(p) => p,
                        Err(e) => {
                            ui_tx.send_critical(UiEvent::BackendWarning(format!(
                                "Failed to canonicalize directory {}: {}",
                                display_path, e
                            )));
                            continue;
                        }
                    };
                    let canonical_str = canonical_path.to_string_lossy().to_string();

                    let mut success = false;
                    {
                        let guard = &*db;
                        if guard.add_source_root(&canonical_str, &display_path).is_ok() {
                            if !watched_roots.contains(&canonical_path) {
                                if let Err(e) = debouncer.watcher().watch(
                                    &canonical_path,
                                    notify_debouncer_mini::notify::RecursiveMode::Recursive,
                                ) {
                                    eprintln!("Watcher failed to watch {}: {}", canonical_str, e);
                                    ui_tx.send_critical(UiEvent::BackendWarning(format!(
                                        "Live updates disabled for {}: {}",
                                        canonical_str, e
                                    )));
                                } else {
                                    watched_roots.insert(canonical_path.clone());
                                }
                            }
                            success = true;
                        } else {
                            ui_tx.send_critical(UiEvent::BackendWarning(format!(
                                "Failed to add directory: {}",
                                display_path
                            )));
                        }
                    }
                    if success {
                        let (root_as_tag, global_rules) = match state.lock() {
                            Ok(s) => (s.backend.root_as_tag, s.backend.global_ignore_rules.clone()),
                            Err(_) => (false, vec![]),
                        };
                        let db_c2 = db.clone();
                        let ui_c2 = ui_tx.clone();
                        // Validate by running the scan; catching I/O/permissions during real work avoids TOCTOU races.
                        match crate::scan::run_scan(
                            canonical_path.clone(),
                            db_c2,
                            global_rules,
                            root_as_tag,
                            ui_c2.clone(),
                        )
                        .await
                        {
                            Ok(res) => {
                                ui_c2.send_critical(UiEvent::ScanCompleted(
                                    res.failed_paths.len(),
                                    res.failed_paths,
                                ));
                            }
                            Err(e) => {
                                let guard = &*db;
                                if let Ok(Some(sr)) = guard.find_source_root_by_path(&canonical_str)
                                {
                                    let _ = guard.remove_source_root(sr.id);
                                    let _ = guard.cleanup_orphaned_tags();
                                }
                                if let Err(e) = debouncer.watcher().unwatch(&canonical_path) {
                                    eprintln!("Watcher failed to unwatch {}: {}", canonical_str, e);
                                }
                                watched_roots.remove(&canonical_path);
                                ui_c2.send_critical(UiEvent::BackendWarning(format!(
                                    "Failed to scan directory: {}",
                                    e
                                )));
                                app_tx.send_critical(AppEvent::FetchData);
                            }
                        }
                    }
                }
                AppEvent::RemoveSourceRoot(id) => {
                    {
                        let guard = &*db;
                        if let Some(root) = guard
                            .list_source_roots()
                            .unwrap_or_default()
                            .into_iter()
                            .find(|r| r.id == id)
                            && let Err(e) = debouncer.watcher().unwatch(Path::new(&root.path))
                        {
                            eprintln!("Watcher failed to unwatch {}: {}", root.path, e);
                        }
                        if let Some(root) = guard
                            .list_source_roots()
                            .unwrap_or_default()
                            .into_iter()
                            .find(|r| r.id == id)
                        {
                            watched_roots.remove(Path::new(&root.path));
                        }
                        if guard.remove_source_root(id).is_ok() {
                            let _ = guard.cleanup_orphaned_tags();
                        }
                    }
                    app_tx.send_critical(AppEvent::FetchData);
                }
                AppEvent::UpdateSettings(backend_state) => {
                    if let Ok(mut s) = state.lock() {
                        s.backend = backend_state;
                        let _ = s.save();
                    }
                }
                AppEvent::RescanRoots => {
                    let mut roots_to_scan = Vec::new();
                    let guard = &*db;
                    if let Ok(roots) = guard.list_source_roots() {
                        roots_to_scan = roots
                            .into_iter()
                            .filter(|r| r.is_available)
                            .map(|r| r.path)
                            .collect();
                    }
                    let (root_as_tag, global_rules) = match state.lock() {
                        Ok(s) => (s.backend.root_as_tag, s.backend.global_ignore_rules.clone()),
                        Err(_) => (false, vec![]),
                    };
                    for path in roots_to_scan {
                        let db_c2 = db.clone();
                        let ui_c2 = ui_tx.clone();
                        let rules = global_rules.clone();
                        if let Ok(res) = crate::scan::run_scan(
                            std::path::PathBuf::from(path.clone()),
                            db_c2,
                            rules,
                            root_as_tag,
                            ui_c2.clone(),
                        )
                        .await
                        {
                            ui_c2.send_critical(UiEvent::ScanCompleted(
                                res.failed_paths.len(),
                                res.failed_paths,
                            ));
                        }
                    }
                }
                AppEvent::RescanSubtree(path) => {
                    let mut scans = pending_scans.lock().unwrap();
                    // Drop duplicates to avoid redundant I/O storms for heavily modified directories.
                    if !scans.insert(path.clone()) {
                        continue;
                    }
                    drop(scans);

                    let (root_as_tag, global_rules) = match state.lock() {
                        Ok(s) => (s.backend.root_as_tag, s.backend.global_ignore_rules.clone()),
                        Err(_) => (false, vec![]),
                    };
                    let db_c2 = db.clone();
                    let ui_c2 = ui_tx.clone();
                    let app_tx_c2 = app_tx.clone();
                    let pending_c = pending_scans.clone();
                    let path_c = path.clone();

                    // Main loop serializes state changes, but subtree I/O scales safely in parallel.
                    tokio::spawn(async move {
                        if let Ok(res) = crate::scan::run_subtree_scan(
                            path_c.clone(),
                            db_c2,
                            global_rules,
                            root_as_tag,
                            ui_c2.clone(),
                        )
                        .await
                        {
                            ui_c2.send_critical(UiEvent::ScanCompleted(
                                res.failed_paths.len(),
                                res.failed_paths,
                            ));
                            app_tx_c2.send_critical(AppEvent::FetchData);
                        }
                        pending_c.lock().unwrap().remove(&path_c);
                    });
                }
                AppEvent::FileChanged(path, kind) => {
                    super::live_update::process_file_changed(
                        path,
                        kind,
                        db.clone(),
                        state.clone(),
                        ui_tx.clone(),
                        app_tx.clone(),
                    );
                }
                AppEvent::QueryMedia(q) => {
                    let db_c = db.clone();
                    let ui_c = ui_tx.clone();
                    tokio::task::spawn_blocking(move || {
                        let db_g = &*db_c;
                        match db_g.query_media(&q) {
                            Ok((items, total)) => {
                                ui_c.send_critical(UiEvent::QueryResult(items, total));
                            }
                            Err(e) => {
                                eprintln!("Failed to query media: {}", e);
                            }
                        }
                    });
                }
                AppEvent::FetchData => {
                    // Coalesce rapid requests (e.g. repeated resizes/events) instead of piling up heavy queries.
                    if fetch_in_progress.swap(true, std::sync::atomic::Ordering::SeqCst) {
                        continue;
                    }

                    let mut roots = vec![];
                    {
                        let db_g = &*db;
                        roots = db_g.list_source_roots().unwrap_or_default();
                    }

                    let mut offline_roots = std::collections::HashSet::new();
                    let mut offline_count = 0;

                    for root in &roots {
                        let path = std::path::Path::new(&root.path);
                        let is_avail =
                            path.exists() && path.is_dir() && std::fs::read_dir(path).is_ok();
                        if !is_avail {
                            offline_roots.insert(root.id);
                            offline_count += 1;
                        } else {
                            let path_buf = path.to_path_buf();
                            if !watched_roots.contains(&path_buf) {
                                if let Err(e) = debouncer.watcher().watch(
                                    path,
                                    notify_debouncer_mini::notify::RecursiveMode::Recursive,
                                ) {
                                    eprintln!("Watcher failed to watch {}: {}", path.display(), e);
                                    ui_tx.send_critical(UiEvent::BackendWarning(format!(
                                        "Live updates disabled for {}: {}",
                                        path.display(),
                                        e
                                    )));
                                } else {
                                    watched_roots.insert(path_buf);
                                }
                            }
                        }
                        if root.is_available != is_avail {
                            let db_g = &*db;
                            let _ = db_g.set_source_root_available(root.id, is_avail);
                        }
                    }

                    let db_c = db.clone();
                    let ui_c = ui_tx.clone();
                    let fetch_progress_c = fetch_in_progress.clone();

                    // Heavy reads block the async loop, so they run in the thread pool.
                    tokio::task::spawn_blocking(move || {
                        {
                            let db_g = &*db_c;

                            let tags: Vec<crate::events::UiTag> = db_g
                                .get_all_tags_with_counts()
                                .unwrap_or_default()
                                .into_iter()
                                .map(|t| crate::events::UiTag {
                                    name: t.name,
                                    file_count: t.file_count,
                                })
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
                            let roots_list = roots
                                .into_iter()
                                .map(|r| crate::events::UiSourceRoot {
                                    id: r.id,
                                    name: std::path::Path::new(&r.display_path)
                                        .file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .to_string(),
                                    path: r.path,
                                    display_path: r.display_path,
                                    is_available: !offline_roots.contains(&r.id),
                                })
                                .collect();
                            ui_c.send_critical(UiEvent::DataFetched {
                                tags,
                                media,
                                roots: roots_list,
                                has_roots,
                            });

                            if offline_count > 0 {
                                ui_c.send_critical(UiEvent::RootsOffline(offline_count));
                            } else {
                                ui_c.send_critical(UiEvent::RootsOffline(0));
                            }
                        }
                        fetch_progress_c.store(false, std::sync::atomic::Ordering::SeqCst);
                    });

                    if !initial_scan_done {
                        initial_scan_done = true;
                        // Fire full rescan after first fetch to ensure UI is hydrated before I/O spins up.
                        app_tx.send_critical(AppEvent::RescanRoots);
                    }
                }
            }
        }
    });
}
