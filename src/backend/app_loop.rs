use crate::backend::liveness::LivenessCommand;
use crate::db::Database;
use crate::events::{AppEvent, ChannelSendExt};
use crate::state::AppState;
use crate::ui::window::UiEvent;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

pub fn start(
    mut app_rx: tokio::sync::mpsc::Receiver<AppEvent>,
    app_tx: tokio::sync::mpsc::Sender<AppEvent>,
    ui_tx: tokio::sync::mpsc::Sender<UiEvent>,
    db: Arc<Database>,
    state: Arc<Mutex<AppState>>,
    liveness_tx: tokio::sync::mpsc::Sender<LivenessCommand>,
    services: Arc<crate::backend::BackendServices>,
) {
    tokio::spawn(async move {
        let pending_scans = Arc::new(Mutex::new(HashSet::new()));

        let mut initial_scan_done = false;
        let fetch_in_progress = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        // Monotonic hydration generation (B-2): stamped on every hydration so the
        // UI applies only chunks from the newest hydration and discards stragglers
        // from a superseded one.
        let mut hydration_generation: u64 = 0;
        let coord = &services.concurrency;
        let thumbnail_cache_state = &services.thumbnail_cache;

        while let Some(event) = app_rx.recv().await {
            match event {
                AppEvent::AddSourceRoot(display_path) => {
                    // I-4: validate the root itself before any insert — it must
                    // exist, canonicalize, be a directory, and be readable. This
                    // is a fast, non-recursive check on the root directory only.
                    // An invalid path is rejected with a recoverable Settings
                    // error and never stored (not even briefly), so a later
                    // transient scan error cannot be mistaken for an invalid root.
                    let canonical_path = match validate_source_root(&display_path) {
                        Ok(p) => p,
                        Err(msg) => {
                            ui_tx.send_critical(UiEvent::SettingsError(msg));
                            continue;
                        }
                    };
                    // Canonical path ensures stable uniqueness; display_path preserves user input format.
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
                        ui_tx.send_critical(UiEvent::SettingsError(
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
                            ui_tx.send_critical(UiEvent::SettingsError(format!(
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
                        // One active full-root scan at a time (B-7): hold the
                        // full-scan permit for the scan's duration.
                        let Some(_full_scan) = coord.acquire_full_scan().await else {
                            ui_c2.send_critical(UiEvent::BackendWarning(
                                "Unable to schedule the initial library scan.".to_string(),
                            ));
                            continue;
                        };
                        // The root was already validated before insert (I-4), so
                        // this initial scan is indexing work — not root validation.
                        // Its failure surfaces an error but must not delete the
                        // (valid) root.
                        let outcome = crate::scan::run_scan(
                            canonical_path.clone(),
                            db_c2,
                            global_rules,
                            root_as_tag,
                            ui_c2.clone(),
                        )
                        .await;
                        handle_initial_scan_outcome(outcome, &ui_c2, &app_tx);
                    }
                }
                AppEvent::RemoveSourceRoot(id) => {
                    // B-7: bump this root's job generation first, so any in-flight
                    // scan or job tagged with the old generation stops producing
                    // results before (and while) its DB rows are torn down.
                    coord.invalidate_root(id);
                    {
                        let guard = &*db;
                        if crate::thumbnail::remove_root_and_cache(
                            guard,
                            &crate::thumbnail::thumbnail_cache_dir(),
                            id,
                        )
                        .is_ok()
                        {
                            let _ = guard.cleanup_orphaned_tags();
                        }
                    }
                    // FetchData triggers a liveness Probe, which unwatches the
                    // root that is no longer in the library.
                    app_tx.send_critical(AppEvent::FetchData);
                }
                AppEvent::ThumbnailVisibility { media_id, visible } => {
                    thumbnail_cache_state.set_visible(media_id, visible);
                }
                AppEvent::ReadThumbnail { media_id, path } => {
                    let db = db.clone();
                    let ui_tx = ui_tx.clone();
                    let cache_state = thumbnail_cache_state.clone();
                    tokio::task::spawn_blocking(move || {
                        let now =
                            crate::db::system_time_to_epoch_millis(std::time::SystemTime::now());
                        if let Err(error) = db.record_thumbnail_access(media_id, now) {
                            tracing::warn!(media_id, %error, "thumbnail access update failed");
                        }
                        match crate::thumbnail::decode_thumbnail(media_id, path) {
                            Ok(decoded) => ui_tx.send_log(UiEvent::ThumbnailDecoded(decoded)),
                            Err(error) => tracing::warn!(media_id, %error, "thumbnail read failed"),
                        }
                        match crate::thumbnail::enforce_disk_budget(
                            &db,
                            &crate::thumbnail::thumbnail_cache_dir(),
                            crate::config::THUMBNAIL_DISK_BUDGET_BYTES,
                            &cache_state,
                        ) {
                            Ok(media_ids) if !media_ids.is_empty() => {
                                ui_tx.send_log(UiEvent::ThumbnailsEvicted(media_ids));
                            }
                            Ok(_) => {}
                            Err(error) => {
                                tracing::warn!(%error, "thumbnail cache maintenance failed");
                            }
                        }
                    });
                }
                AppEvent::UpdateSettings(backend_state) => {
                    if let Ok(mut s) = state.lock() {
                        s.backend = backend_state;
                        let _ = s.save(&db);
                    }
                }
                AppEvent::RescanRoots => {
                    start_maintenance(
                        crate::backend::maintenance::MaintenanceOperation::RescanLibrary,
                        &db,
                        &state,
                        &ui_tx,
                        &app_tx,
                        &services,
                    );
                }
                AppEvent::RegenerateThumbnails => {
                    start_maintenance(
                        crate::backend::maintenance::MaintenanceOperation::RegenerateThumbnails,
                        &db,
                        &state,
                        &ui_tx,
                        &app_tx,
                        &services,
                    );
                }
                AppEvent::RebuildLibraryIndex => {
                    start_maintenance(
                        crate::backend::maintenance::MaintenanceOperation::RebuildLibraryIndex,
                        &db,
                        &state,
                        &ui_tx,
                        &app_tx,
                        &services,
                    );
                }
                AppEvent::RescanSubtree(path) => {
                    let mut scans = lock_pending_scans(&pending_scans);
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
                    let coord_c = coord.clone();

                    // Tag this job with the owning root's current generation (B-7)
                    // so removing that root mid-scan drops the in-flight walker.
                    let cancel = match owning_root(&db, &path) {
                        Some(root) => {
                            coord.cancellation(root.id, coord.current_generation(root.id))
                        }
                        None => crate::backend::concurrency::Cancellation::never(),
                    };

                    // Main loop serializes state changes, but subtree I/O scales
                    // safely in parallel — bounded to min(4, parallelism) (B-7).
                    tokio::spawn(async move {
                        let Some(_permit) = coord_c.acquire_subtree().await else {
                            ui_c2.send_critical(UiEvent::BackendWarning(
                                "Unable to schedule a folder rescan.".to_string(),
                            ));
                            lock_pending_scans(&pending_c).remove(&path_c);
                            return;
                        };
                        let db_err = db_c2.clone();
                        match crate::scan::run_subtree_scan(
                            path_c.clone(),
                            db_c2,
                            global_rules,
                            root_as_tag,
                            ui_c2.clone(),
                            cancel,
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
                        lock_pending_scans(&pending_c).remove(&path_c);
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
                        liveness_tx.clone(),
                    );
                }
                AppEvent::QueryMedia(q, generation) => {
                    let db_c = db.clone();
                    let ui_c = ui_tx.clone();
                    // B-7: mark a query in flight so thumbnail workers defer to it.
                    // The guard clears the flag when the query task ends (even on
                    // panic), so thumbnail work is never starved past the query.
                    let query_guard = coord.begin_query();
                    tokio::task::spawn_blocking(move || {
                        let _query_guard = query_guard;
                        let db_g = &*db_c;
                        match db_g.query_media(&q) {
                            Ok((items, total)) => {
                                // Echo the generation so the UI can discard this
                                // result if a newer query has since superseded it.
                                ui_c.send_critical(UiEvent::QueryResult(items, total, generation));
                            }
                            Err(e) => {
                                tracing::error!(error = %e, "failed to query media");
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

fn start_maintenance(
    operation: crate::backend::maintenance::MaintenanceOperation,
    db: &Arc<Database>,
    state: &Arc<Mutex<AppState>>,
    ui_tx: &tokio::sync::mpsc::Sender<UiEvent>,
    app_tx: &tokio::sync::mpsc::Sender<AppEvent>,
    services: &Arc<crate::backend::BackendServices>,
) {
    let backend_state = state
        .lock()
        .map(|state| state.backend.clone())
        .unwrap_or_default();
    crate::backend::maintenance::start_operation(
        operation,
        db.clone(),
        backend_state,
        ui_tx.clone(),
        app_tx.clone(),
        services.clone(),
    );
}

/// Continues rescan coalescing after a task panic poisoned the mutex. The set
/// contains only deduplication state, so retaining its recovered contents is
/// safer than terminating the backend loop or allowing unbounded duplicate work.
fn lock_pending_scans(pending_scans: &Mutex<HashSet<PathBuf>>) -> MutexGuard<'_, HashSet<PathBuf>> {
    match pending_scans.lock() {
        Ok(scans) => scans,
        Err(poisoned) => {
            tracing::error!("pending subtree-scan mutex poisoned; continuing with recovered state");
            poisoned.into_inner()
        }
    }
}

/// Surfaces a failed scan instead of silently swallowing the `Err` branch
/// (ARCH-003 / B-2 point 6).
///
/// Best-effort records the failure to the `scan_errors` table (A-4) under the
/// owning source root, and emits a structured backend error event so the UI's
/// scan-error surface reflects it. Previously a whole-scan failure was dropped.
pub(crate) fn report_scan_failure(
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

/// Validates a candidate source root before it is stored (I-4).
///
/// The path must exist, canonicalize, be a directory, and be readable. This is a
/// fast, non-recursive check on the root directory only — never a scan. Returns
/// the canonical path on success, or a user-facing error message; the caller
/// rejects the add without ever inserting a row, so an invalid root is never
/// briefly visible.
fn validate_source_root(display_path: &str) -> Result<std::path::PathBuf, String> {
    let canonical = Path::new(display_path)
        .canonicalize()
        .map_err(|e| format!("Failed to canonicalize directory {display_path}: {e}"))?;
    if !canonical.is_dir() {
        return Err(format!("Not a directory: {display_path}"));
    }
    std::fs::read_dir(&canonical)
        .map_err(|e| format!("Cannot read directory {display_path}: {e}"))?;
    Ok(canonical)
}

/// Handles the outcome of a newly-added root's initial scan (I-4).
///
/// On success it publishes `ScanCompleted`. On failure it surfaces the error and
/// refreshes the UI but **keeps the root**: the root was already validated before
/// insert, so a scan failure is a transient/content error, not an invalid root,
/// and must never delete it.
fn handle_initial_scan_outcome(
    outcome: anyhow::Result<crate::scan::ScanResult>,
    ui_tx: &tokio::sync::mpsc::Sender<UiEvent>,
    app_tx: &tokio::sync::mpsc::Sender<AppEvent>,
) {
    match outcome {
        Ok(res) => {
            ui_tx.send_critical(UiEvent::ScanCompleted(
                res.failed_paths.len(),
                res.failed_paths,
            ));
        }
        Err(e) => {
            ui_tx.send_critical(UiEvent::BackendWarning(format!(
                "Failed to scan directory: {}",
                e
            )));
            app_tx.send_critical(AppEvent::FetchData);
        }
    }
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

    // ── I-4: root validation before insert ──────────────────────────

    #[test]
    fn invalid_paths_are_rejected_before_insert() {
        // A nonexistent path fails to canonicalize → rejected, never stored.
        let missing = "/definitely/does/not/exist/vesper-i4-check";
        assert!(
            validate_source_root(missing).is_err(),
            "a nonexistent path must be rejected before any insert"
        );

        // A regular file exists and canonicalizes but is not a directory.
        let file = tempfile::NamedTempFile::new().unwrap();
        let err = validate_source_root(file.path().to_str().unwrap()).unwrap_err();
        assert!(err.contains("Not a directory"), "got: {err}");

        // A real, readable directory is accepted and returns its canonical path.
        let dir = tempfile::TempDir::new().unwrap();
        let canonical = validate_source_root(dir.path().to_str().unwrap()).unwrap();
        assert!(canonical.is_dir());
    }

    #[tokio::test]
    async fn scan_failure_after_valid_insert_does_not_delete_the_root() {
        let db = Arc::new(Database::open_in_memory().unwrap());
        // The root already passed pre-insert validation and was stored.
        let root_id = db.add_source_root("/media", "/media").unwrap();
        let (ui_tx, mut ui_rx) = tokio::sync::mpsc::channel(4);
        let (app_tx, _app_rx) = tokio::sync::mpsc::channel(4);

        // The initial indexing scan fails (a transient/content error).
        handle_initial_scan_outcome(Err(anyhow::anyhow!("boom")), &ui_tx, &app_tx);

        // The validated root is preserved, not deleted.
        let roots = db.list_source_roots().unwrap();
        assert!(
            roots.iter().any(|r| r.id == root_id),
            "a scan failure must not delete a root that was valid at add-time"
        );

        // The failure is still surfaced to the UI rather than swallowed.
        let event = ui_rx.recv().await.expect("a warning must be emitted");
        assert!(
            matches!(event, UiEvent::BackendWarning(_)),
            "scan failure surfaces a recoverable warning"
        );
    }
}
