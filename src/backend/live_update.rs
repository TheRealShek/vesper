use crate::backend::liveness::LivenessCommand;
use crate::db::Database;
use crate::events::ChannelSendExt;
use crate::state::AppState;
use crate::ui::window::UiEvent;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub fn process_file_changed(
    path: PathBuf,
    kind: crate::events::ChangeKind,
    db_backend: Arc<Database>,
    state_backend: Arc<Mutex<AppState>>,
    ui_tx_backend: tokio::sync::mpsc::Sender<UiEvent>,
    app_tx_backend: tokio::sync::mpsc::Sender<crate::events::AppEvent>,
    liveness_tx_backend: tokio::sync::mpsc::Sender<LivenessCommand>,
) {
    if kind != crate::events::ChangeKind::Deleted && path.is_dir() {
        app_tx_backend.send_critical(crate::events::AppEvent::RescanSubtree(path));
        return;
    }

    if kind == crate::events::ChangeKind::Deleted {
        // Delete handling is DB-bound and does not sleep, so it stays on the
        // blocking pool.
        tokio::task::spawn_blocking(move || {
            process_delete_event(&path, &db_backend, &ui_tx_backend, &liveness_tx_backend);
        });
    } else {
        // Create/modify handling runs as an async task so the B-3 stability
        // probe and its retry backoff can sleep without pinning a blocking
        // thread.
        tokio::spawn(process_modify_event(
            path,
            db_backend,
            state_backend,
            ui_tx_backend,
        ));
    }
}

/// Handles a delete event: gates removal on the owning root being online (B-4),
/// then publishes the removals and refreshed tag counts when records are purged.
fn process_delete_event(
    path: &Path,
    db: &Database,
    ui_tx: &tokio::sync::mpsc::Sender<UiEvent>,
    liveness_tx: &tokio::sync::mpsc::Sender<LivenessCommand>,
) {
    // B-4: a watcher delete only tells us the path vanished, which also happens
    // when the whole root is unmounted. Gate removal on the owning root being
    // online before purging any records.
    let removed_paths = process_delete(path, db, |root_id| probe_root_online(liveness_tx, root_id));
    if removed_paths.is_empty() {
        return;
    }
    for p in removed_paths {
        ui_tx.send_critical(UiEvent::MediaRemoved(p));
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
    ui_tx.send_critical(UiEvent::TagsUpdated(tags));
}

/// Handles a create/modify event under the B-3 stability discipline.
///
/// The order is deliberate: the hardcoded scanner-level temp-file filter runs
/// first (an in-progress download or editor backup must never yield a record or
/// an error), then a cheap extension gate, then the stability probe — a file is
/// indexed only once it has come to rest, and never while it is still being
/// written.
async fn process_modify_event(
    path: PathBuf,
    db: Arc<Database>,
    state: Arc<Mutex<AppState>>,
    ui_tx: tokio::sync::mpsc::Sender<UiEvent>,
) {
    // Scanner-level temp filter, ahead of everything else and independent of
    // user ignore rules (B-3).
    if crate::index::media::is_temp_file(&path) {
        return;
    }

    // Cheap extension gate before the sleeping stability probe: non-media change
    // events are dropped without any metadata polling.
    if crate::index::media::classify(&path).is_none() {
        return;
    }

    // B-3: index a file only once two metadata reads 250ms apart agree, with a
    // bounded 1s/5s/30s retry backoff for files still being written.
    let stable = match await_stable(&path).await {
        Some(m) => m,
        None => {
            // Retry budget exhausted without the file ever settling: drop this
            // event. A later watcher event on the final write re-triggers us.
            tracing::warn!(
                file = %crate::logging::redact_path(&path),
                "live update: file never stabilized within the retry budget; dropping event"
            );
            return;
        }
    };

    let size_bytes = stable.len();
    let modified = stable
        .modified()
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    let created = stable.created().ok();

    // Heavy DB + classification work stays on the blocking pool.
    tokio::task::spawn_blocking(move || {
        index_stable_file(path, size_bytes, modified, created, db, state, ui_tx);
    });
}

/// Indexes a file that has passed the B-3 stability check, using the settled
/// metadata captured by the probe (never re-read here, so we can't catch a fresh
/// mid-write), then publishes the added item and refreshed tag counts.
fn index_stable_file(
    path: PathBuf,
    size_bytes: u64,
    modified: std::time::SystemTime,
    created: Option<std::time::SystemTime>,
    db: Arc<Database>,
    state: Arc<Mutex<AppState>>,
    ui_tx: tokio::sync::mpsc::Sender<UiEvent>,
) {
    // Locate the owning root; a change under no known root is nothing to index.
    let (root_id, root_path_str) = {
        let db = &*db;
        let Ok(roots) = db.list_source_roots() else {
            return;
        };
        match roots
            .iter()
            .filter(|r| path.starts_with(&r.path))
            .max_by_key(|r| std::path::Path::new(&r.path).components().count())
        {
            Some(root) => (root.id, root.path.clone()),
            None => return,
        }
    };

    let (root_as_tag, global_patterns) = match state.lock() {
        Ok(s) => (s.backend.root_as_tag, s.backend.global_ignore_rules.clone()),
        Err(_) => (false, Vec::new()),
    };

    let root_path = std::path::Path::new(&root_path_str);
    let global_rules = match crate::index::ignore_rules::build_global_rules(&global_patterns) {
        Ok(rules) => rules,
        Err(_) => match ignore::gitignore::GitignoreBuilder::new("/").build() {
            Ok(rules) => rules,
            Err(e) => {
                tracing::error!(error = %e, "failed to build empty ignore rules");
                return;
            }
        },
    };

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

    if crate::index::ignore_rules::is_ignored(&path, false, &ignore_stack, &global_rules) {
        return;
    }
    let Some(media_type) = crate::index::media::classify(&path) else {
        return;
    };

    let discovered = crate::events::DiscoveredMedia {
        path: path.clone(),
        media_type,
        size_bytes,
        modified,
        created,
    };
    let _ =
        crate::scan::process_single_file(&discovered, root_path, root_id, root_as_tag, db.clone());

    let db = &*db;
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
        ui_tx.send_critical(UiEvent::MediaAdded(item));
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
        ui_tx.send_critical(UiEvent::TagsUpdated(tags));
    }
}

/// The pause between the two metadata reads that decide write-stability (B-3).
const STABILITY_WINDOW: std::time::Duration = std::time::Duration::from_millis(250);

/// Bounded retry backoff applied when a file is still being written (B-3): an
/// unstable file is retried after 1s, then 5s, then 30s. If it never settles
/// across this schedule, the change event is dropped rather than published.
const STABILITY_RETRY_BACKOFF: [std::time::Duration; 3] = [
    std::time::Duration::from_secs(1),
    std::time::Duration::from_secs(5),
    std::time::Duration::from_secs(30),
];

/// Probes `path` for write-stability on the bounded retry schedule (B-3): an
/// initial probe, then a retry after each backoff delay. Returns the settled
/// metadata, or `None` if the file never came to rest within the budget (or
/// became unreadable).
async fn await_stable(path: &Path) -> Option<std::fs::Metadata> {
    retry_until_stable(&STABILITY_RETRY_BACKOFF, || probe_once(path)).await
}

/// Runs `probe` once, then again after each delay in `schedule`, returning the
/// first `Some` it yields or `None` once the schedule is exhausted. Generic over
/// the probe so the backoff policy is unit-testable without real files or waits.
async fn retry_until_stable<T, F, Fut>(schedule: &[std::time::Duration], mut probe: F) -> Option<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Option<T>>,
{
    if let Some(v) = probe().await {
        return Some(v);
    }
    for &delay in schedule {
        tokio::time::sleep(delay).await;
        if let Some(v) = probe().await {
            return Some(v);
        }
    }
    None
}

/// One stability probe: reads `path`'s metadata twice, [`STABILITY_WINDOW`]
/// apart, and returns the second read only if size and mtime are unchanged. A
/// file still being written (differing reads) or an unreadable one yields
/// `None`, so a mid-write record is never published (B-3).
async fn probe_once(path: &Path) -> Option<std::fs::Metadata> {
    let first = tokio::fs::metadata(path).await.ok()?;
    tokio::time::sleep(STABILITY_WINDOW).await;
    let second = tokio::fs::metadata(path).await.ok()?;

    let same_mtime = match (first.modified(), second.modified()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    };
    (first.len() == second.len() && same_mtime).then_some(second)
}

/// Gates a watcher delete event on the owning source root being online (B-4).
///
/// A delete event only tells us a path vanished — which happens both when a file
/// is genuinely deleted and when its whole source root is unmounted or
/// disconnected. Per 01 §4, source-root disappearance is treated as offline, not
/// deletion: the media records must be preserved in that case.
///
/// `probe_root_online` reports whether the owning root is currently reachable.
/// In production it is answered by the liveness worker (B-2), so there is a
/// single probing path and the worker also marks a vanished root offline as a
/// side effect of the probe. Records are removed only when the root is confirmed
/// online *and* the specific file is confirmed gone. Returns the removed paths
/// (empty when the delete is suppressed).
fn process_delete(
    path: &Path,
    db: &Database,
    probe_root_online: impl FnOnce(i64) -> bool,
) -> Vec<String> {
    let owning_root = db
        .list_source_roots()
        .unwrap_or_default()
        .into_iter()
        .filter(|r| path.starts_with(&r.path))
        .max_by_key(|r| Path::new(&r.path).components().count());

    let Some(root) = owning_root else {
        // Not under any known root — nothing we own to remove.
        return Vec::new();
    };

    if !probe_root_online(root.id) {
        // Root offline: preserve records (root disappearance is offline, not
        // deletion). The probe has already marked the root offline.
        return Vec::new();
    }

    // Root confirmed online — only remove records when the file is really gone,
    // guarding against a transient miss while the root was being probed.
    if path.exists() {
        return Vec::new();
    }

    let path_str = path.to_string_lossy().to_string();
    crate::thumbnail::remove_media_and_cache(
        db,
        &crate::thumbnail::thumbnail_cache_dir(),
        &path_str,
    )
    .unwrap_or_default()
}

/// Asks the liveness worker whether `root_id` is online, blocking until it
/// answers. Runs inside `spawn_blocking`, so it uses the blocking channel APIs.
/// Fails closed (treats the root as offline, preserving records) if the worker
/// is gone or drops the response.
fn probe_root_online(
    liveness_tx: &tokio::sync::mpsc::Sender<LivenessCommand>,
    root_id: i64,
) -> bool {
    let (respond, response) = tokio::sync::oneshot::channel();
    if liveness_tx
        .blocking_send(LivenessCommand::ProbeRoot { root_id, respond })
        .is_err()
    {
        return false;
    }
    response.blocking_recv().unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::MediaEntry;
    use crate::events::MediaType;

    fn insert_media_at(db: &Database, root_id: i64, path: &Path) {
        let writer = db.writer.lock().unwrap();
        let filename = path.file_name().unwrap().to_string_lossy().to_string();
        let entry = MediaEntry {
            path: path.to_string_lossy().to_string(),
            relative_path: filename.clone(),
            canonical_identity: path.to_string_lossy().to_string(),
            filename,
            source_root_id: root_id,
            media_type: MediaType::Image,
            size_bytes: 1,
            created_at: None,
            modified_at: 1000,
        };
        db.upsert_media_inner(&writer, &entry, 1).unwrap();
    }

    #[test]
    fn delete_suppressed_when_root_goes_offline_records_preserved() {
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root("/media", "/media").unwrap();
        for name in ["a.jpg", "b.jpg", "c.jpg"] {
            insert_media_at(&db, root_id, &PathBuf::from("/media").join(name));
        }

        // The root went offline mid-batch: the probe reports offline and — as the
        // real liveness worker's reconcile does — marks the root offline in the DB.
        let probe = |rid: i64| {
            db.set_source_root_available(rid, false).unwrap();
            false
        };
        let removed = process_delete(Path::new("/media/a.jpg"), &db, probe);

        assert!(
            removed.is_empty(),
            "delete must be suppressed while the owning root is offline"
        );
        // Records are preserved, not purged.
        assert_eq!(db.count_media().unwrap(), 3);
        // The root is marked offline instead (01 §4: disappearance is offline).
        let root = db
            .list_source_roots()
            .unwrap()
            .into_iter()
            .find(|r| r.id == root_id)
            .unwrap();
        assert!(
            !root.is_available,
            "root disappearance is treated as offline, not deletion"
        );
    }

    #[test]
    fn delete_removes_record_when_root_online_and_file_gone() {
        // A real, readable directory stands in for an online source root.
        let dir = tempfile::TempDir::new().unwrap();
        let root_path = dir.path().to_string_lossy().to_string();
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root(&root_path, &root_path).unwrap();

        // A record whose backing file genuinely does not exist on disk.
        let gone = dir.path().join("gone.jpg");
        insert_media_at(&db, root_id, &gone);
        assert_eq!(db.count_media().unwrap(), 1);

        // Root is confirmed online; the file is confirmed gone → remove.
        let removed = process_delete(&gone, &db, |_rid| true);

        assert_eq!(removed.len(), 1, "the deleted file's record is removed");
        assert_eq!(removed[0], gone.to_string_lossy());
        assert_eq!(db.count_media().unwrap(), 0);
    }

    // ── B-3: stability check for live-updated files ─────────────────

    #[tokio::test]
    async fn growing_file_is_not_stable_on_first_read() {
        // A file still being written keeps changing size across the 250ms
        // stability window, so the probe must report unstable (None) — the
        // record is never published on the first read.
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("clip.mp4");
        std::fs::write(&path, vec![0u8; 1024]).unwrap();

        // A writer that appends more bytes partway through the probe's window.
        let writer_path = path.clone();
        let writer = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&writer_path)
                .unwrap();
            f.write_all(&vec![0u8; 4096]).unwrap();
        });

        let result = probe_once(&path).await;
        writer.await.unwrap();

        assert!(
            result.is_none(),
            "a file still growing across the window must not be treated as stable"
        );
    }

    #[tokio::test]
    async fn settled_file_is_reported_stable() {
        // A file that is not being written stays identical across the window, so
        // the probe returns its metadata and indexing may proceed.
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("still.jpg");
        std::fs::write(&path, vec![7u8; 2048]).unwrap();

        let result = probe_once(&path).await;

        let meta = result.expect("a settled file must be reported stable");
        assert_eq!(meta.len(), 2048);
    }

    #[tokio::test(start_paused = true)]
    async fn retry_budget_exhaustion_drops_the_event() {
        // A file that never stabilizes is probed once, then retried after each
        // backoff delay (1s, 5s, 30s). When the whole schedule is exhausted the
        // probe yields None, so the caller drops the event (defined behavior).
        let calls = std::cell::Cell::new(0u32);
        let start = tokio::time::Instant::now();

        let result: Option<()> = retry_until_stable(&STABILITY_RETRY_BACKOFF, || {
            calls.set(calls.get() + 1);
            async { None }
        })
        .await;

        assert!(
            result.is_none(),
            "an eternally-unstable file yields no record"
        );
        assert_eq!(
            calls.get(),
            4,
            "one initial probe plus the three backoff retries"
        );
        assert_eq!(
            start.elapsed(),
            std::time::Duration::from_secs(1 + 5 + 30),
            "the 1s/5s/30s backoff schedule elapses before giving up"
        );
    }

    #[tokio::test]
    async fn temp_file_change_events_are_ignored_repeatedly() {
        // A .crdownload file never produces a record or an error, no matter how
        // many change events it emits: process_modify_event filters it before
        // the stability check even starts.
        let dir = tempfile::TempDir::new().unwrap();
        let db = Arc::new(Database::open_in_memory().unwrap());
        let root_path = dir.path().to_string_lossy().to_string();
        let root_id = db.add_source_root(&root_path, &root_path).unwrap();
        let state = Arc::new(Mutex::new(AppState::default()));
        let (ui_tx, mut ui_rx) = tokio::sync::mpsc::channel(8);

        // A real, fully-written temp file under the root.
        let partial = dir.path().join("movie.mp4.crdownload");
        std::fs::write(&partial, vec![0u8; 4096]).unwrap();

        // Fire several change events for it, as a live download would.
        for _ in 0..3 {
            process_modify_event(partial.clone(), db.clone(), state.clone(), ui_tx.clone()).await;
        }
        drop(ui_tx);

        // No record was inserted and no UI event (add/error) was emitted.
        assert_eq!(
            db.count_media().unwrap(),
            0,
            "temp file must not be indexed"
        );
        assert!(
            ui_rx.recv().await.is_none(),
            "temp file must not emit any UI event"
        );
        let _ = root_id;
    }
}
