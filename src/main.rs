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
use libadwaita::{glib, Application};
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
            dialog.add_response("close", "Close");
            dialog.connect_response(None, move |_, _| {
                std::process::exit(1);
            });
            dialog.present();
        });
    }));

    let app = Application::builder()
        .application_id("com.github.vesper.gallery")
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
            // Error handling will be done in the UI layer since we need the app loop
            // But we can't easily do it if we fail here. We'll just exit.
            eprintln!("Failed to load database or state");
            return glib::ExitCode::FAILURE;
        }
    };

    // Start Thumbnail Worker
    crate::thumbnail::start_thumbnail_worker(db_arc.clone(), thumb_rx, ui_tx.clone());

    // Backend Loop
    let db_backend = db_arc.clone();
    let ui_tx_backend = ui_tx.clone();
    let state_backend = state_arc.clone();
    let app_tx_backend = app_tx.clone();
    
    tokio::spawn(async move {
        while let Some(event) = app_rx.recv().await {
            match event {
                AppEvent::AddSourceRoot(path) => {
                    let mut success = false;
                    if let Ok(guard) = db_backend.lock() {
                        let _ = guard.add_source_root(&path);
                        success = true;
                    } else {
                        let _ = ui_tx_backend.send(UiEvent::FatalError("Database lock poisoned".to_string()));
                    }
                    if success {
                        let (root_as_tag, global_rules) = match state_backend.lock() {
                            Ok(s) => (s.root_as_tag, s.global_ignore_rules.clone()),
                            Err(_) => (false, vec![]),
                        };
                        let db_c2 = db_backend.clone();
                        let ui_c2 = ui_tx_backend.clone();
                        if let Ok(res) = crate::scan::run_scan(std::path::PathBuf::from(path), db_c2, global_rules, root_as_tag).await {
                            let _ = ui_c2.send(UiEvent::ScanCompleted(res.errors));
                        }
                    }
                }
                AppEvent::RemoveSourceRoot(id) => {
                    if let Ok(guard) = db_backend.lock() {
                        let _ = guard.remove_source_root(id);
                    }
                    let _ = app_tx_backend.send(AppEvent::FetchData);
                }
                AppEvent::RescanRoots => {
                    let mut roots_to_scan = Vec::new();
                    if let Ok(guard) = db_backend.lock() {
                        if let Ok(roots) = guard.list_source_roots() {
                            roots_to_scan = roots.into_iter().map(|r| r.path).collect();
                        }
                    }
                    let (root_as_tag, global_rules) = match state_backend.lock() {
                        Ok(s) => (s.root_as_tag, s.global_ignore_rules.clone()),
                        Err(_) => (false, vec![]),
                    };
                    for path in roots_to_scan {
                        let db_c2 = db_backend.clone();
                        let ui_c2 = ui_tx_backend.clone();
                        let rules = global_rules.clone();
                        if let Ok(res) = crate::scan::run_scan(std::path::PathBuf::from(path), db_c2, rules, root_as_tag).await {
                            let _ = ui_c2.send(UiEvent::ScanCompleted(res.errors));
                        }
                    }
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
                            }
                            if root.is_available != is_avail {
                                let _ = db_g.set_source_root_available(root.id, is_avail);
                            }
                        }
                        
                        let tags = db_g.get_all_tags_with_counts().unwrap_or_default();
                        let media = db_g.get_all_media_with_tags().unwrap_or_default();
                        let has_roots = !roots.is_empty();
                        let roots_list = roots.into_iter().map(|r| (r.id, r.path)).collect();
                        let _ = ui_tx_backend.send(UiEvent::DataFetched { tags, media, roots: roots_list, has_roots, offline_roots });
                        
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
