use crate::backend::liveness::LivenessCommand;
use crate::db::Database;
use crate::events::{AppEvent, ChannelSendExt};
use crate::state::AppState;
use crate::ui::window::UiEvent;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub fn start(
    mut app_rx: tokio::sync::mpsc::Receiver<AppEvent>,
    app_tx: tokio::sync::mpsc::Sender<AppEvent>,
    ui_tx: tokio::sync::mpsc::Sender<UiEvent>,
    db: Arc<Database>,
    state: Arc<Mutex<AppState>>,
    liveness_tx: tokio::sync::mpsc::Sender<LivenessCommand>,
) {
    tokio::spawn(async move {
        let pending_scans =
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashSet::new()));

        let mut initial_scan_done = false;
        let fetch_in_progress = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        // Monotonic hydration generation (B-2): stamped on every hydration so the
        // UI applies only chunks from the newest hydration and discards stragglers
        // from a superseded one.
        let mut hydration_generation: u64 = 0;

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

                    // Reject overlapping/duplicate/nested roots (I-3): the new
                    // canonical path must not equal, be contained by, or contain
                    // any existing canonical root.
                    let overlaps_existing =
                        db.list_source_roots().unwrap_or_default().iter().any(|r| {
                            let existing = Path::new(&r.path);
                            canonical_path.starts_with(existing)
                                || existing.starts_with(&canonical_path)
                        });
                    if overlaps_existing {
                        ui_tx.send_critical(UiEvent::BackendWarning(
                            "This folder is already covered by an existing source directory."
                                .to_string(),
                        ));
                        continue;
                    }

                    let mut success = false;
                    {
                        let guard = &*db;
                        if guard.add_source_root(&canonical_str, &display_path).is_ok() {
                            // Watcher setup is owned by the liveness worker now;
                            // reconcile so the new root is probed and watched.
                            liveness_tx.send_critical(LivenessCommand::Probe);
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
                                ui_c2.send_critical(UiEvent::BackendWarning(format!(
                                    "Failed to scan directory: {}",
                                    e
                                )));
                                // FetchData triggers a liveness Probe, which
                                // unwatches the now-removed root.
                                app_tx.send_critical(AppEvent::FetchData);
                            }
                        }
                    }
                }
                AppEvent::RemoveSourceRoot(id) => {
                    {
                        let guard = &*db;
                        if guard.remove_source_root(id).is_ok() {
                            let _ = guard.cleanup_orphaned_tags();
                        }
                    }
                    // FetchData triggers a liveness Probe, which unwatches the
                    // root that is no longer in the library.
                    app_tx.send_critical(AppEvent::FetchData);
                }
                AppEvent::UpdateSettings(backend_state) => {
                    if let Ok(mut s) = state.lock() {
                        s.backend = backend_state;
                        let _ = s.save(&db);
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
                        match crate::scan::run_scan(
                            std::path::PathBuf::from(path.clone()),
                            db_c2,
                            rules,
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
                            // ARCH-003 / B-2 pt 6: surface the failure instead of
                            // silently swallowing the Err branch.
                            Err(e) => {
                                report_scan_failure(&db, &ui_c2, Path::new(&path), &e);
                            }
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
                        let db_err = db_c2.clone();
                        match crate::scan::run_subtree_scan(
                            path_c.clone(),
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
                                app_tx_c2.send_critical(AppEvent::FetchData);
                            }
                            // ARCH-003 / B-2 pt 6: a failed subtree scan used to be
                            // dropped silently; surface it as a structured error.
                            Err(e) => {
                                report_scan_failure(&db_err, &ui_c2, &path_c, &e);
                            }
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
                AppEvent::QueryMedia(q, generation) => {
                    let db_c = db.clone();
                    let ui_c = ui_tx.clone();
                    tokio::task::spawn_blocking(move || {
                        let db_g = &*db_c;
                        match db_g.query_media(&q) {
                            Ok((items, total)) => {
                                // Echo the generation so the UI can discard this
                                // result if a newer query has since superseded it.
                                ui_c.send_critical(UiEvent::QueryResult(items, total, generation));
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

                    // Hydration is now a database read: liveness probing, watcher
                    // setup, and availability writes are owned by the liveness
                    // worker (B-2, ARCH-004). Trigger a probe (transitional; made
                    // fully read-only in sub-step c) and read availability — kept
                    // current by that worker — from the DB rather than the fs.
                    liveness_tx.send_critical(LivenessCommand::Probe);

                    hydration_generation += 1;
                    let generation = hydration_generation;
                    let db_c = db.clone();
                    let ui_c = ui_tx.clone();
                    let fetch_progress_c = fetch_in_progress.clone();

                    // Heavy reads block the async loop, so they run in the thread pool.
                    tokio::task::spawn_blocking(move || {
                        {
                            let db_g = &*db_c;

                            let roots = db_g.list_source_roots().unwrap_or_default();

                            let tags: Vec<crate::events::UiTag> = db_g
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
                                    is_available: r.is_available,
                                })
                                .collect();

                            // Media is streamed in bounded, generation-tagged chunks
                            // instead of one full-library reload (B-2). The first
                            // chunk rides with DataFetched so the grid appears
                            // immediately; the rest follow as MediaChunk events.
                            let chunk_size = crate::config::HYDRATION_CHUNK_SIZE;
                            let first = db_g.hydrate_media_chunk(0, chunk_size).unwrap_or_default();
                            let mut delivered = first.len() as i64;

                            // RootsOffline is published by the liveness worker on
                            // each probe, so hydration no longer emits it here.
                            // `blocking_send` (not `send_critical`) preserves FIFO
                            // order so DataFetched always precedes its chunks.
                            if ui_c
                                .blocking_send(UiEvent::DataFetched {
                                    tags,
                                    media: first,
                                    roots: roots_list,
                                    has_roots,
                                    generation,
                                })
                                .is_err()
                            {
                                fetch_progress_c.store(false, std::sync::atomic::Ordering::SeqCst);
                                return;
                            }

                            loop {
                                let chunk = db_g
                                    .hydrate_media_chunk(delivered, chunk_size)
                                    .unwrap_or_default();
                                if chunk.is_empty() {
                                    break;
                                }
                                delivered += chunk.len() as i64;
                                let is_full = chunk.len() as i64 == chunk_size;
                                if ui_c
                                    .blocking_send(UiEvent::MediaChunk {
                                        generation,
                                        items: chunk,
                                    })
                                    .is_err()
                                {
                                    break;
                                }
                                if !is_full {
                                    break;
                                }
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

/// Surfaces a failed scan instead of silently swallowing the `Err` branch
/// (ARCH-003 / B-2 point 6).
///
/// Best-effort records the failure to the `scan_errors` table (A-4) under the
/// owning source root, and emits a structured backend error event so the UI's
/// scan-error surface reflects it. Previously a whole-scan failure was dropped.
fn report_scan_failure(
    db: &Database,
    ui_tx: &tokio::sync::mpsc::Sender<UiEvent>,
    scope: &Path,
    err: &anyhow::Error,
) {
    let message = format!("Scan failed for {}: {}", scope.display(), err);

    if let Some(root) = owning_root(db, scope) {
        let scan_generation = db.get_max_scan_generation(root.id).unwrap_or(0);
        let entry = crate::db::ScanErrorEntry {
            source_root_id: root.id,
            scan_generation,
            path: scope.to_string_lossy().to_string(),
            category: "scan-failed".to_string(),
            message: message.clone(),
        };
        let _ = db.record_scan_errors(&[entry]);
    }

    ui_tx.send_critical(UiEvent::BackendWarning(message));
}

/// Finds the source root that owns `path` — the root whose stored (canonical)
/// path is a prefix of `path`.
fn owning_root(db: &Database, path: &Path) -> Option<crate::db::SourceRoot> {
    db.list_source_roots()
        .unwrap_or_default()
        .into_iter()
        .find(|r| path.starts_with(&r.path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use std::sync::Arc;

    #[tokio::test]
    async fn subtree_scan_failure_emits_structured_error_event() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let root_id = db.add_source_root("/media", "/media").unwrap();
        let (ui_tx, mut ui_rx) = tokio::sync::mpsc::channel(4);

        // Stand in for a failed subtree scan (previously swallowed by `if let Ok`).
        let scope = Path::new("/media/sub");
        let err = anyhow::anyhow!("permission denied");
        report_scan_failure(&db, &ui_tx, scope, &err);

        // The structured backend error event fires rather than being dropped.
        let event = ui_rx.recv().await.expect("an error event must be emitted");
        if let UiEvent::BackendWarning(msg) = event {
            assert!(msg.contains("/media/sub"), "message names the failed scope");
            assert!(
                msg.contains("permission denied"),
                "message carries the cause"
            );
        } else {
            panic!("expected a BackendWarning error event");
        }

        // And it is persisted to scan_errors under the owning root.
        assert_eq!(db.count_scan_errors_for_root(root_id).unwrap(), 1);
    }
}
