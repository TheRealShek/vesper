pub mod config;
mod db;
mod events;
mod index;
mod scan;
pub mod state;
mod thumbnail;
mod ui;

use crate::events::AppEvent;
use crate::events::ChannelSendExt;
use crate::ui::window::UiEvent;
use libadwaita as adw;
use libadwaita::prelude::*;
use libadwaita::{Application, glib, gtk};
use std::sync::{Arc, Mutex};

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
            if let Some(app) = gtk::gio::Application::default().and_downcast::<gtk::Application>()
                && let Some(win) = app.active_window()
            {
                dialog.set_transient_for(Some(&win));
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

    // Bounded to prevent memory exhaustion during filesystem event storms.
    let (app_tx, mut app_rx) = tokio::sync::mpsc::channel::<AppEvent>(1024);
    let (ui_tx, ui_rx) = tokio::sync::mpsc::channel::<UiEvent>(256);
    let (thumb_tx, thumb_rx) =
        tokio::sync::mpsc::channel::<crate::thumbnail::ThumbnailRequest>(128);

    let db_res = dirs::data_dir()
        .ok_or_else(|| std::io::Error::other("Could not determine user data directory"))
        .and_then(|data_dir| {
            let vesper_dir = data_dir.join("vesper");
            std::fs::create_dir_all(&vesper_dir)?;
            let db_path = vesper_dir.join(crate::config::DB_NAME);
            crate::db::Database::open(&db_path).map_err(|e| std::io::Error::other(e.to_string()))
        });
    let state_res = std::panic::catch_unwind(crate::state::AppState::load);

    let (db_arc, state_arc) = match (db_res, state_res) {
        (Ok(db), Ok(state)) => (Arc::new(db), Arc::new(Mutex::new(state))),
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
    // notify's event delivery is synchronous, so it runs on a dedicated OS thread instead of tokio.
    std::thread::spawn(move || {
        while let Ok(res) = debouncer_rx.recv() {
            match res {
                Ok(events) => {
                    let events: Vec<notify_debouncer_mini::DebouncedEvent> = events;
                    for event in events {
                        let path = event.path;
                        if path.file_name().and_then(|n| n.to_str()) == Some(".galleryignore") {
                            if let Some(parent) = path.parent() {
                                app_tx_watcher.send_log(crate::events::AppEvent::RescanSubtree(
                                    parent.to_path_buf(),
                                ));
                            }
                        } else {
                            // Debouncer coalesces create/delete/modify; exists() is the ground truth.
                            let kind = if path.exists() {
                                crate::events::ChangeKind::Modified
                            } else {
                                crate::events::ChangeKind::Deleted
                            };
                            let _ = app_tx_watcher
                                .send(crate::events::AppEvent::FileChanged(path, kind));
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

        let mut initial_scan_done = false;
        let fetch_in_progress = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        while let Some(event) = app_rx.recv().await {
            match event {
                AppEvent::AddSourceRoot(display_path) => {
                    // Canonical path ensures stable uniqueness; display_path preserves user input format.
                    let canonical_path = match std::path::Path::new(&display_path).canonicalize() {
                        Ok(p) => p,
                        Err(e) => {
                            ui_tx_backend.send_log(UiEvent::ShowError(format!(
                                "Failed to canonicalize directory {}: {}",
                                display_path, e
                            )));
                            continue;
                        }
                    };
                    let canonical_str = canonical_path.to_string_lossy().to_string();

                    let mut success = false;
                    if let Ok(guard) = db_backend.lock() {
                        if guard.add_source_root(&canonical_str, &display_path).is_ok() {
                            if let Err(e) = debouncer.watcher().watch(
                                &canonical_path,
                                notify_debouncer_mini::notify::RecursiveMode::Recursive,
                            ) {
                                eprintln!("Watcher failed to watch {}: {}", canonical_str, e);
                                ui_tx_backend.send_log(UiEvent::ScanCompleted(
                                    1,
                                    vec![format!(
                                        "Live updates disabled for {}: {}",
                                        canonical_str, e
                                    )],
                                ));
                            }
                            success = true;
                        } else {
                            ui_tx_backend.send_log(UiEvent::ShowError(format!(
                                "Failed to add directory: {}",
                                display_path
                            )));
                        }
                    } else {
                        let _ = ui_tx_backend
                            .send(UiEvent::FatalError("Database lock poisoned".to_string()));
                    }
                    if success {
                        let (root_as_tag, global_rules) = match state_backend.lock() {
                            Ok(s) => (s.backend.root_as_tag, s.backend.global_ignore_rules.clone()),
                            Err(_) => (false, vec![]),
                        };
                        let db_c2 = db_backend.clone();
                        let ui_c2 = ui_tx_backend.clone();
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
                                ui_c2.send_log(UiEvent::ScanCompleted(
                                    res.failed_paths.len(),
                                    res.failed_paths,
                                ));
                            }
                            Err(e) => {
                                if let Ok(guard) = db_backend.lock()
                                    && let Ok(Some(sr)) =
                                        guard.find_source_root_by_path(&canonical_str)
                                {
                                    let _ = guard.remove_source_root(sr.id);
                                    let _ = guard.cleanup_orphaned_tags();
                                }
                                if let Err(e) = debouncer.watcher().unwatch(&canonical_path) {
                                    eprintln!("Watcher failed to unwatch {}: {}", canonical_str, e);
                                }
                                ui_c2.send_log(UiEvent::ShowError(format!(
                                    "Failed to scan directory: {}",
                                    e
                                )));
                                app_tx_backend.send_log(AppEvent::FetchData);
                            }
                        }
                    }
                }
                AppEvent::RemoveSourceRoot(id) => {
                    if let Ok(guard) = db_backend.lock() {
                        if let Some(root) = guard
                            .list_source_roots()
                            .unwrap_or_default()
                            .into_iter()
                            .find(|r| r.id == id)
                            && let Err(e) = debouncer
                                .watcher()
                                .unwatch(std::path::Path::new(&root.path))
                        {
                            eprintln!("Watcher failed to unwatch {}: {}", root.path, e);
                        }
                        if guard.remove_source_root(id).is_ok() {
                            let _ = guard.cleanup_orphaned_tags();
                        }
                    }
                    app_tx_backend.send_log(AppEvent::FetchData);
                }
                AppEvent::UpdateSettings(backend_state) => {
                    if let Ok(mut state) = state_backend.lock() {
                        state.backend = backend_state;
                        let _ = state.save();
                    }
                }
                AppEvent::RescanRoots => {
                    let mut roots_to_scan = Vec::new();
                    if let Ok(guard) = db_backend.lock()
                        && let Ok(roots) = guard.list_source_roots()
                    {
                        roots_to_scan = roots
                            .into_iter()
                            .filter(|r| r.is_available)
                            .map(|r| r.path)
                            .collect();
                    }
                    let (root_as_tag, global_rules) = match state_backend.lock() {
                        Ok(s) => (s.backend.root_as_tag, s.backend.global_ignore_rules.clone()),
                        Err(_) => (false, vec![]),
                    };
                    for path in roots_to_scan {
                        let db_c2 = db_backend.clone();
                        let ui_c2 = ui_tx_backend.clone();
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
                            ui_c2.send_log(UiEvent::ScanCompleted(
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

                    let (root_as_tag, global_rules) = match state_backend.lock() {
                        Ok(s) => (s.backend.root_as_tag, s.backend.global_ignore_rules.clone()),
                        Err(_) => (false, vec![]),
                    };
                    let db_c2 = db_backend.clone();
                    let ui_c2 = ui_tx_backend.clone();
                    let app_tx_c2 = app_tx_backend.clone();
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
                            ui_c2.send_log(UiEvent::ScanCompleted(
                                res.failed_paths.len(),
                                res.failed_paths,
                            ));
                            app_tx_c2.send_log(AppEvent::FetchData);
                        }
                        pending_c.lock().unwrap().remove(&path_c);
                    });
                }
                AppEvent::FileChanged(path, kind) => {
                    if kind != crate::events::ChangeKind::Deleted && path.is_dir() {
                        app_tx_backend.send_log(AppEvent::RescanSubtree(path));
                        continue;
                    }

                    let db_g = db_backend.clone();
                    let state_g = state_backend.clone();
                    let ui_c = ui_tx_backend.clone();
                    tokio::task::spawn_blocking(move || {
                        if kind == crate::events::ChangeKind::Deleted {
                            if let Ok(db) = db_g.lock() {
                                let path_str = path.to_string_lossy().to_string();
                                if db.remove_media_by_path(&path_str).unwrap_or(false) {
                                    ui_c.send_log(UiEvent::MediaRemoved(path_str));
                                    let tags = db
                                        .get_all_tags_with_counts()
                                        .unwrap_or_default()
                                        .into_iter()
                                        .map(|t| crate::events::UiTag {
                                            name: t.name,
                                            file_count: t.file_count,
                                        })
                                        .collect();
                                    ui_c.send_log(UiEvent::TagsUpdated(tags));
                                }
                            }
                        } else {
                            let mut should_process = false;
                            let mut root_id = 0;
                            let mut root_path_str = String::new();
                            let mut root_as_tag = false;
                            let mut global_patterns = Vec::new();

                            if let Ok(db) = db_g.lock()
                                && let Ok(roots) = db.list_source_roots()
                                && let Some(root) = roots
                                    .iter()
                                    .filter(|r| path.starts_with(&r.path))
                                    .max_by_key(|r| {
                                        std::path::Path::new(&r.path).components().count()
                                    })
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
                                    match crate::index::ignore_rules::build_global_rules(
                                        &global_patterns,
                                    ) {
                                        Ok(rules) => rules,
                                        Err(_) => {
                                            match ignore::gitignore::GitignoreBuilder::new("/")
                                                .build()
                                            {
                                                Ok(rules) => rules,
                                                Err(e) => {
                                                    eprintln!(
                                                        "Failed to build empty ignore rules: {}",
                                                        e
                                                    );
                                                    return;
                                                }
                                            }
                                        }
                                    };

                                let mut ignore_stack = Vec::new();
                                let mut current = root_path.to_path_buf();

                                if let Ok(Some(rules)) =
                                    crate::index::ignore_rules::load_directory_rules(&current)
                                {
                                    ignore_stack.push(rules);
                                }

                                if let Ok(rel) =
                                    path.parent().unwrap_or(&path).strip_prefix(root_path)
                                {
                                    for comp in rel.components() {
                                        current.push(comp);
                                        if let Ok(Some(rules)) =
                                            crate::index::ignore_rules::load_directory_rules(
                                                &current,
                                            )
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
                                    if let Ok(db) = db_g.lock() {
                                        let path_str = path.to_string_lossy().to_string();
                                        if let Ok(all_media) = db.get_all_media_with_tags()
                                            && let Some((row, mtags)) = all_media
                                                .into_iter()
                                                .find(|(r, _)| r.path == path_str)
                                        {
                                            let item = crate::events::UiMediaItem {
                                                id: row.id,
                                                path: row.path,
                                                filename: row.filename,
                                                tags: mtags,
                                                thumbnail_path: row
                                                    .thumbnail_path
                                                    .unwrap_or_default(),
                                                duration_secs: row.duration_secs.unwrap_or(-1),
                                                media_type: row.media_type,
                                                size_bytes: row.size_bytes,
                                                created_at: row.created_at,
                                                modified_at: row.modified_at,
                                                is_offline: false,
                                            };
                                            ui_c.send_log(UiEvent::MediaAdded(item));
                                            let tags = db
                                                .get_all_tags_with_counts()
                                                .unwrap_or_default()
                                                .into_iter()
                                                .map(|t| crate::events::UiTag {
                                                    name: t.name,
                                                    file_count: t.file_count,
                                                })
                                                .collect();
                                            ui_c.send_log(UiEvent::TagsUpdated(tags));
                                        }
                                    }
                                }
                            }
                        }
                    });
                }
                AppEvent::QueryMedia(q) => {
                    let db_c = db_backend.clone();
                    let ui_c = ui_tx_backend.clone();
                    tokio::task::spawn_blocking(move || {
                        if let Ok(db) = db_c.lock() {
                            match db.query_media(&q) {
                                Ok((items, total)) => {
                                    ui_c.send_log(UiEvent::QueryResult(items, total));
                                }
                                Err(e) => {
                                    eprintln!("Failed to query media: {}", e);
                                }
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
                    if let Ok(db_g) = db_backend.lock() {
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
                            if let Err(e) = debouncer.watcher().watch(
                                path,
                                notify_debouncer_mini::notify::RecursiveMode::Recursive,
                            ) {
                                eprintln!("Watcher failed to watch {}: {}", path.display(), e);
                                ui_tx_backend.send_log(UiEvent::ScanCompleted(
                                    1,
                                    vec![format!(
                                        "Live updates disabled for {}: {}",
                                        path.display(),
                                        e
                                    )],
                                ));
                            }
                        }
                        if root.is_available != is_avail
                            && let Ok(db_g) = db_backend.lock()
                        {
                            let _ = db_g.set_source_root_available(root.id, is_avail);
                        }
                    }

                    let db_c = db_backend.clone();
                    let ui_c = ui_tx_backend.clone();
                    let fetch_progress_c = fetch_in_progress.clone();

                    // Heavy reads block the async loop, so they run in the thread pool.
                    tokio::task::spawn_blocking(move || {
                        if let Ok(db_g) = db_c.lock() {
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
                            ui_c.send_log(UiEvent::DataFetched {
                                tags,
                                media,
                                roots: roots_list,
                                has_roots,
                            });

                            if offline_count > 0 {
                                ui_c.send_log(UiEvent::RootsOffline(offline_count));
                            } else {
                                ui_c.send_log(UiEvent::RootsOffline(0));
                            }
                        }
                        fetch_progress_c.store(false, std::sync::atomic::Ordering::SeqCst);
                    });

                    if !initial_scan_done {
                        initial_scan_done = true;
                        // Fire full rescan after first fetch to ensure UI is hydrated before I/O spins up.
                        app_tx_backend.send_log(AppEvent::RescanRoots);
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
                state_arc.clone(),
            );
        }
    });

    let ret = app.run();
    rt.shutdown_background();
    ret
}
