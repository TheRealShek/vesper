use crate::events::{ChannelSendExt, UiEvent};
use anyhow::Result;

use crate::db::Database;
use crate::events::MediaType;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

pub struct ThumbnailRequest {
    pub media_id: i64,
    // Retained for the request API used by the UI. The worker now sources the
    // path, type, size, and mtime from the DB by `media_id` (T-1) so the cache
    // key and generation always reflect the current row, so these are unread here.
    #[allow(dead_code)]
    pub path: PathBuf,
    #[allow(dead_code)]
    pub media_type: MediaType,
    #[allow(dead_code)]
    pub modified_at: i64,
}

/// Coordinates cache maintenance across thumbnail workers and backend reads.
/// The visible set is populated by virtualized grid bind/unbind events.
pub struct ThumbnailCacheState {
    visible_media: Mutex<HashSet<i64>>,
    maintenance: Mutex<()>,
}

impl ThumbnailCacheState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            visible_media: Mutex::new(HashSet::new()),
            maintenance: Mutex::new(()),
        })
    }

    pub fn set_visible(&self, media_id: i64, visible: bool) {
        let Ok(mut items) = self.visible_media.lock() else {
            tracing::error!("thumbnail visibility guard is poisoned");
            return;
        };
        if visible {
            items.insert(media_id);
        } else {
            items.remove(&media_id);
        }
    }

    fn visible_snapshot(&self) -> HashSet<i64> {
        self.visible_media
            .lock()
            .map(|items| items.clone())
            .unwrap_or_default()
    }
}

pub fn start_thumbnail_worker(
    db: Arc<Database>,
    rx: mpsc::Receiver<ThumbnailRequest>,
    ui_sender: tokio::sync::mpsc::Sender<UiEvent>,
    coord: Arc<crate::backend::concurrency::BackendConcurrency>,
    cache_state: Arc<ThumbnailCacheState>,
) {
    let cache_dir = thumbnail_cache_dir();

    // Shared pull model so multiple workers can drain the same queue without a dedicated dispatcher.
    let rx_shared = Arc::new(tokio::sync::Mutex::new(rx));
    let num_workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        // Capped at 4 due to diminishing returns; ffmpeg is heavily CPU-bound and scales poorly beyond this.
        .min(4);

    let initial_db = db.clone();
    let initial_dir = cache_dir.clone();
    let initial_state = cache_state.clone();
    let initial_ui_sender = ui_sender.clone();
    tokio::spawn(async move {
        match maintain_disk_budget(initial_db, initial_dir, initial_state).await {
            Ok(media_ids) if !media_ids.is_empty() => {
                initial_ui_sender.send_log(UiEvent::ThumbnailsEvicted(media_ids));
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, "initial thumbnail cache maintenance failed");
            }
        }
    });

    for _ in 0..num_workers {
        let rx_clone = rx_shared.clone();
        let db_clone = db.clone();
        let ui_sender_clone = ui_sender.clone();
        let cache_dir_clone = cache_dir.clone();
        let coord_clone = coord.clone();
        let cache_state_clone = cache_state.clone();

        tokio::spawn(async move {
            loop {
                let req = {
                    let mut guard = rx_clone.lock().await;
                    guard.recv().await
                };

                let req = match req {
                    Some(r) => r,
                    None => break, // Channel closed
                };

                // UI queries take priority (B-7): defer this CPU-heavy job while
                // any query is in flight so query latency is never stuck behind
                // thumbnail generation.
                coord_clone.query_gate().wait_until_idle().await;

                // Key-addressed generation (T-1): compute the stable cache key,
                // render to it, and record success (cache key/path, clear
                // stale/failure) or failure. `force = false` reuses an existing
                // key-addressed file.
                match generate_and_record(&db_clone, &cache_dir_clone, req.media_id, false).await {
                    // A `None` result means the source changed during generation
                    // and the write was rejected as stale (NEW-2): nothing is
                    // published and the row stays stale for regeneration.
                    Ok(None) => {}
                    Ok(Some((thumb_path, duration))) => {
                        ui_sender_clone.send_log(UiEvent::ThumbnailReady(
                            req.media_id,
                            thumb_path,
                            duration,
                        ));
                        match maintain_disk_budget(
                            db_clone.clone(),
                            cache_dir_clone.clone(),
                            cache_state_clone.clone(),
                        )
                        .await
                        {
                            Ok(media_ids) if !media_ids.is_empty() => {
                                ui_sender_clone.send_log(UiEvent::ThumbnailsEvicted(media_ids))
                            }
                            Ok(_) => {}
                            Err(error) => {
                                tracing::warn!(%error, "thumbnail cache maintenance failed");
                            }
                        }
                    }
                    Err(e) => {
                        // The failure status is recorded in the DB by
                        // generate_and_record; the UI shows a placeholder (U/V).
                        tracing::warn!(
                            media_id = req.media_id,
                            error = %e,
                            "thumbnail generation failed"
                        );
                    }
                }
            }
        });
    }
}

/// Decodes a thumbnail read to RGBA off the GTK thread.
pub fn decode_thumbnail(media_id: i64, path: String) -> Result<crate::events::DecodedThumbnail> {
    let image = image::open(&path)?.to_rgba8();
    let (width, height) = image.dimensions();
    Ok(crate::events::DecodedThumbnail {
        media_id,
        path,
        width,
        height,
        pixels: image.into_raw(),
    })
}

/// Enforces the disk budget by evicting non-visible manifest entries in DB LRU
/// order. The budget parameter is injectable for focused tests.
pub fn enforce_disk_budget(
    db: &Database,
    cache_dir: &Path,
    budget_bytes: u64,
    state: &ThumbnailCacheState,
) -> Result<Vec<i64>> {
    let _maintenance = state
        .maintenance
        .lock()
        .map_err(|_| anyhow::anyhow!("thumbnail maintenance guard is poisoned"))?;
    let entries = db.list_thumbnail_cache_entries()?;
    let visible = state.visible_snapshot();
    let cache_dir = cache_dir.to_path_buf();

    let mut total = cache_disk_usage(&cache_dir)?;
    let mut evicted = Vec::new();
    for entry in entries {
        if total <= budget_bytes {
            break;
        }
        if visible.contains(&entry.media_id) {
            continue;
        }

        let path = cache_dir.join(format!("{}.jpg", entry.cache_key));
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        db.clear_evicted_thumbnail(entry.media_id, &entry.cache_key)?;
        total = total.saturating_sub(size);
        evicted.push(entry.media_id);
    }
    Ok(evicted)
}

async fn maintain_disk_budget(
    db: Arc<Database>,
    cache_dir: PathBuf,
    state: Arc<ThumbnailCacheState>,
) -> Result<Vec<i64>> {
    tokio::task::spawn_blocking(move || {
        enforce_disk_budget(
            &db,
            &cache_dir,
            crate::config::THUMBNAIL_DISK_BUDGET_BYTES,
            &state,
        )
    })
    .await?
}

fn cache_disk_usage(cache_dir: &Path) -> Result<u64> {
    let mut total = 0_u64;
    for item in std::fs::read_dir(cache_dir)? {
        let item = item?;
        if item.file_type()?.is_file() {
            total = total.saturating_add(item.metadata()?.len());
        }
    }
    Ok(total)
}

fn delete_cache_entries(
    cache_dir: &Path,
    entries: &[crate::db::ThumbnailCacheEntry],
) -> Result<()> {
    for entry in entries {
        let path = cache_dir.join(format!("{}.jpg", entry.cache_key));
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

/// Removes a media path (or directory subtree) and its cache files in the same
/// background job. Cache keys are captured before the rows disappear.
pub fn remove_media_and_cache(db: &Database, cache_dir: &Path, path: &str) -> Result<Vec<String>> {
    let entries = db.thumbnail_cache_entries_for_path_tree(path)?;
    delete_cache_entries(cache_dir, &entries)?;
    let removed = db.remove_media_and_descendants(path)?;
    Ok(removed)
}

/// Removes a source root and every thumbnail referenced by its cascading media
/// rows in the same background job.
pub fn remove_root_and_cache(db: &Database, cache_dir: &Path, root_id: i64) -> Result<()> {
    let entries = db.thumbnail_cache_entries_for_root(root_id)?;
    delete_cache_entries(cache_dir, &entries)?;
    db.remove_source_root(root_id)?;
    Ok(())
}

pub fn remove_stale_media_and_cache(
    db: &Database,
    cache_dir: &Path,
    root_id: i64,
    scan_gen: i64,
) -> Result<usize> {
    let entries = db.thumbnail_cache_entries_for_stale_generation(root_id, scan_gen)?;
    delete_cache_entries(cache_dir, &entries)?;
    let removed = db.remove_stale_media(root_id, scan_gen)?;
    Ok(removed)
}

pub fn remove_stale_subtree_and_cache(
    db: &Database,
    cache_dir: &Path,
    root_id: i64,
    subtree: &str,
    scan_gen: i64,
) -> Result<usize> {
    let entries = db.thumbnail_cache_entries_for_stale_subtree(root_id, subtree, scan_gen)?;
    delete_cache_entries(cache_dir, &entries)?;
    let removed = db.remove_stale_media_in_subtree(root_id, subtree, scan_gen)?;
    Ok(removed)
}

/// Thumbnail size variant tag, part of the stable cache key (T-1). Only the grid
/// variant exists in v1; the tag keeps keys distinct if variants are added later.
const GRID_VARIANT: &str = "grid256";

/// Returns the shared thumbnail cache directory, creating it if needed.
pub fn thumbnail_cache_dir() -> PathBuf {
    let dir = dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("vesper")
        .join("thumbnails");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Computes the stable cache key for a media item's thumbnail (T-1).
///
/// It hashes the canonical identity, the size variant, and a content fingerprint
/// (size + mtime). Addressing by canonical identity — not the raw access path —
/// keeps the key stable across symlink/move access, while the content
/// fingerprint yields a new key when the file changes, so a regenerated
/// thumbnail lands in a new file and the old one stays until the new one
/// succeeds.
pub fn cache_key_for(canonical_identity: &str, size_bytes: i64, modified_at: i64) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    canonical_identity.hash(&mut hasher);
    GRID_VARIANT.hash(&mut hasher);
    size_bytes.hash(&mut hasher);
    modified_at.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Generates a thumbnail for one media item and records the outcome (T-1).
///
/// On success: writes the key-addressed cache file, then sets the cache key,
/// path, and duration and clears the stale/failure flags. On failure: records
/// the failure and leaves the previous thumbnail untouched. `force` re-renders
/// even when a cache file already exists (used by explicit regeneration).
/// Returns the thumbnail path + duration on success, or `None` when the source
/// file changed during generation: the database rejects the stale write, the
/// obsolete cache file is removed, and the row stays stale for a later
/// regeneration against the current source version (NEW-2). Callers must not
/// publish a `None` result to the UI.
async fn generate_and_record(
    db: &Database,
    cache_dir: &Path,
    media_id: i64,
    force: bool,
) -> Result<Option<(String, Option<i64>)>> {
    let src = db
        .get_thumbnail_source(media_id)?
        .ok_or_else(|| anyhow::anyhow!("media {media_id} not found"))?;
    let cache_key = cache_key_for(&src.canonical_identity, src.size_bytes, src.modified_at);
    let thumb_path = cache_dir.join(format!("{cache_key}.jpg"));

    match generate_thumbnail_file(Path::new(&src.path), &src.media_type, &thumb_path, force).await {
        Ok(duration) => {
            let path_str = thumb_path.to_string_lossy().to_string();
            let updated = db.set_thumbnail_success(
                media_id,
                &cache_key,
                &path_str,
                src.modified_at,
                duration,
            )?;
            if !updated {
                let _ = tokio::fs::remove_file(&thumb_path).await;
                return Ok(None);
            }
            Ok(Some((path_str, duration)))
        }
        Err(e) => {
            // Record the failure so the UI can show a stable placeholder; keep
            // the old thumbnail in place.
            let _ = db.set_thumbnail_failure(media_id, &e.to_string());
            Err(e)
        }
    }
}

/// Explicitly (re)generates one media item's thumbnail (T-1).
///
/// This is the callable regeneration operation — B-6's maintenance UI will drive
/// it later. It forces a re-render even if a cache file exists, so it also
/// recovers stale and previously-failed thumbnails. On success the cache
/// key/path is updated and the stale/failure flags are cleared; on failure the
/// old thumbnail is preserved and the failure recorded.
///
/// Not yet wired into a production caller — B-6's maintenance flow will invoke it.
#[allow(dead_code)]
pub async fn regenerate_thumbnail(
    db: &Database,
    cache_dir: &Path,
    media_id: i64,
) -> Result<Option<(String, Option<i64>)>> {
    generate_and_record(db, cache_dir, media_id, true).await
}

/// Renders a thumbnail to the key-addressed `thumb_path` (T-1).
///
/// Because the path is content-addressed, an existing file already holds the
/// right content and is reused unless `force` is set (explicit regeneration).
/// Returns the video duration when applicable.
async fn generate_thumbnail_file(
    media_path: &Path,
    media_type: &MediaType,
    thumb_path: &Path,
    force: bool,
) -> Result<Option<i64>> {
    let mut duration_secs = None;
    if *media_type == MediaType::Video {
        let mut cmd = tokio::process::Command::new("ffprobe");
        cmd.args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(media_path)
        .kill_on_drop(true);

        // 30s timeout prevents hung subprocesses from permanently consuming limited worker slots.
        if let Ok(Ok(out)) =
            tokio::time::timeout(std::time::Duration::from_secs(30), cmd.output()).await
        {
            let s = String::from_utf8_lossy(&out.stdout);
            if let Ok(f) = s.trim().parse::<f64>() {
                duration_secs = Some(f.round() as i64);
            }
        } else {
            tracing::warn!(
                media = %crate::logging::redact_path(media_path),
                "ffprobe timed out or failed"
            );
        }
    }

    // The path is content-addressed (key includes size + mtime), so an existing
    // file already holds the right content — reuse it unless forced.
    if !force && thumb_path.exists() {
        return Ok(duration_secs);
    }

    match media_type {
        MediaType::Image => {
            let path_clone = media_path.to_path_buf();
            let thumb_path_clone = thumb_path.to_path_buf();
            tokio::task::spawn_blocking(move || -> Result<()> {
                let img = image::open(&path_clone)?;
                let resized = img.resize_to_fill(256, 256, image::imageops::FilterType::Triangle);
                resized.save_with_format(&thumb_path_clone, image::ImageFormat::Jpeg)?;
                Ok(())
            })
            .await??;
        }
        MediaType::Video => {
            let media_path_str = match media_path.to_str() {
                Some(s) => s,
                None => anyhow::bail!("Invalid UTF-8 in media path"),
            };
            let thumb_path_str = match thumb_path.to_str() {
                Some(s) => s,
                None => anyhow::bail!("Invalid UTF-8 in thumbnail path"),
            };

            let mut cmd = tokio::process::Command::new("ffmpeg");
            cmd.args([
                "-y",
                "-i",
                media_path_str,
                "-vf",
                "thumbnail,scale=256:256:force_original_aspect_ratio=increase,crop=256:256",
                "-frames:v",
                "1",
                "-q:v",
                "5",
                thumb_path_str,
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);

            // 30s timeout prevents hung subprocesses from permanently consuming limited worker slots.
            let status_res =
                tokio::time::timeout(std::time::Duration::from_secs(30), cmd.status()).await;
            match status_res {
                Ok(Ok(status)) if status.success() => {}
                Ok(Ok(_)) => anyhow::bail!("ffmpeg failed"),
                Ok(Err(e)) => anyhow::bail!("ffmpeg error: {}", e),
                Err(_) => {
                    tracing::warn!(
                        media = %crate::logging::redact_path(media_path),
                        "ffmpeg timed out"
                    );
                    anyhow::bail!("ffmpeg timed out");
                }
            }
        }
    }

    Ok(duration_secs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Database, MediaEntry};

    fn insert_image(
        db: &Database,
        root_id: i64,
        path: &str,
        size_bytes: i64,
        modified_at: i64,
    ) -> i64 {
        let entry = MediaEntry {
            path: path.into(),
            relative_path: path.into(),
            canonical_identity: path.into(),
            filename: "x".into(),
            source_root_id: root_id,
            media_type: MediaType::Image,
            size_bytes,
            created_at: None,
            modified_at,
        };
        let writer = db.writer.lock().unwrap();
        db.upsert_media_inner(&writer, &entry, 1).unwrap().unwrap()
    }

    #[tokio::test]
    async fn regeneration_clears_stale_and_updates_cache_key_and_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let img_path = dir.path().join("a.png");
        image::RgbImage::new(8, 8).save(&img_path).unwrap();
        let cache_dir = dir.path().join("cache");
        std::fs::create_dir_all(&cache_dir).unwrap();

        let db = Database::open_in_memory().unwrap();
        let root = dir.path().to_str().unwrap();
        let root_id = db.add_source_root(root, root).unwrap();
        let path = img_path.to_str().unwrap();
        let id = insert_image(&db, root_id, path, 16, 2000);

        // An initial thumbnail exists; then the file is modified (new mtime) and
        // re-indexed, flagging it stale while keeping the old thumbnail.
        db.set_thumbnail_success(id, "oldkey", "/cache/oldkey.jpg", 2000, None)
            .unwrap();
        insert_image(&db, root_id, path, 16, 3000); // same path → upsert, mtime 3000

        let stale = db.get_thumbnail_status(id).unwrap().unwrap();
        assert!(
            stale.stale,
            "the modified file is stale before regeneration"
        );
        assert_eq!(stale.cache_key.as_deref(), Some("oldkey"));

        // Explicit regeneration.
        let (thumb_path, _duration) = regenerate_thumbnail(&db, &cache_dir, id)
            .await
            .unwrap()
            .expect("regeneration for the current source version is not stale");

        let after = db.get_thumbnail_status(id).unwrap().unwrap();
        assert!(!after.stale, "regeneration clears the stale flag");
        assert!(after.failure.is_none());

        // The cache key is updated to the current content's key, and the file
        // exists on disk at the new key-addressed path.
        let expected_key = cache_key_for(path, 16, 3000);
        assert_ne!(
            after.cache_key.as_deref(),
            Some("oldkey"),
            "cache key updated"
        );
        assert_eq!(after.cache_key.as_deref(), Some(expected_key.as_str()));
        assert_eq!(after.thumbnail_path.as_deref(), Some(thumb_path.as_str()));
        assert!(
            std::path::Path::new(&thumb_path).exists(),
            "a new key-addressed thumbnail file was written"
        );
    }

    #[tokio::test]
    async fn generation_failure_records_status_and_keeps_old_thumbnail() {
        let dir = tempfile::TempDir::new().unwrap();
        // A file with a media extension but garbage contents → decode fails.
        let broken = dir.path().join("broken.jpg");
        std::fs::write(&broken, b"not a real image").unwrap();
        let cache_dir = dir.path().join("cache");
        std::fs::create_dir_all(&cache_dir).unwrap();

        let db = Database::open_in_memory().unwrap();
        let root = dir.path().to_str().unwrap();
        let root_id = db.add_source_root(root, root).unwrap();
        let path = broken.to_str().unwrap();
        let id = insert_image(&db, root_id, path, 10, 1000);

        // It already has a (previously good) thumbnail.
        db.set_thumbnail_success(id, "oldkey", "/cache/oldkey.jpg", 1000, None)
            .unwrap();

        let result = regenerate_thumbnail(&db, &cache_dir, id).await;
        assert!(result.is_err(), "decoding a non-image fails");

        // The failure status is queryable, and the old thumbnail is preserved so
        // the UI can keep showing it (or a placeholder — a U/V concern).
        let status = db.get_thumbnail_status(id).unwrap().unwrap();
        assert!(status.failure.is_some(), "failure reason is recorded");
        assert_eq!(
            status.thumbnail_path.as_deref(),
            Some("/cache/oldkey.jpg"),
            "the old thumbnail is kept after a failure"
        );
        assert_eq!(status.cache_key.as_deref(), Some("oldkey"));

        // A failed row is listed for explicit regeneration.
        assert!(
            db.list_media_needing_thumbnail_regen()
                .unwrap()
                .contains(&id)
        );
    }

    fn write_cache_file(dir: &Path, key: &str, bytes: usize) -> PathBuf {
        let path = dir.join(format!("{key}.jpg"));
        std::fs::write(&path, vec![0_u8; bytes]).unwrap();
        path
    }

    #[test]
    fn cache_eviction_triggers_when_budget_exceeded_and_respects_lru_order() {
        let dir = tempfile::TempDir::new().unwrap();
        let cache_dir = dir.path().join("cache");
        std::fs::create_dir_all(&cache_dir).unwrap();
        let db = Database::open_in_memory().unwrap();
        let root = dir.path().to_str().unwrap();
        let root_id = db.add_source_root(root, root).unwrap();
        let old_id = insert_image(&db, root_id, "/media/old.jpg", 1, 1);
        let new_id = insert_image(&db, root_id, "/media/new.jpg", 1, 1);
        let old_path = write_cache_file(&cache_dir, "old", 8);
        let new_path = write_cache_file(&cache_dir, "new", 8);
        db.set_thumbnail_success(old_id, "old", old_path.to_str().unwrap(), 1, None)
            .unwrap();
        db.set_thumbnail_success(new_id, "new", new_path.to_str().unwrap(), 1, None)
            .unwrap();
        assert!(db.record_thumbnail_access(old_id, 1_000_000).unwrap());
        assert!(db.record_thumbnail_access(new_id, 2_000_000).unwrap());

        let state = ThumbnailCacheState::new();
        let evicted = enforce_disk_budget(&db, &cache_dir, 8, &state).unwrap();

        assert_eq!(evicted, vec![old_id]);
        assert!(!old_path.exists());
        assert!(new_path.exists());
        assert!(
            db.get_thumbnail_status(old_id)
                .unwrap()
                .unwrap()
                .cache_key
                .is_none()
        );
        assert_eq!(
            db.get_thumbnail_status(new_id)
                .unwrap()
                .unwrap()
                .cache_key
                .as_deref(),
            Some("new")
        );
    }

    #[test]
    fn thumbnail_access_within_batch_window_skips_duplicate_timestamp_write() {
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root("/media", "/media").unwrap();
        let media_id = insert_image(&db, root_id, "/media/a.jpg", 1, 1);
        db.set_thumbnail_success(media_id, "key", "/cache/key.jpg", 1, None)
            .unwrap();

        assert!(db.record_thumbnail_access(media_id, 1_000_000).unwrap());
        assert!(!db.record_thumbnail_access(media_id, 1_000_001).unwrap());
        let stored: i64 = db
            .writer
            .lock()
            .unwrap()
            .query_row(
                "SELECT last_accessed_at FROM media WHERE id = ?1",
                [media_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored, 1_000_000);
    }

    #[test]
    fn deleting_media_row_removes_its_thumbnail_cache_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let cache_dir = dir.path().join("cache");
        std::fs::create_dir_all(&cache_dir).unwrap();
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root("/media", "/media").unwrap();
        let media_id = insert_image(&db, root_id, "/media/a.jpg", 1, 1);
        let cache_path = write_cache_file(&cache_dir, "key", 8);
        db.set_thumbnail_success(media_id, "key", cache_path.to_str().unwrap(), 1, None)
            .unwrap();

        let removed = remove_media_and_cache(&db, &cache_dir, "/media/a.jpg").unwrap();

        assert_eq!(removed, vec!["/media/a.jpg"]);
        assert!(!cache_path.exists());
        assert!(db.get_thumbnail_status(media_id).unwrap().is_none());
    }

    #[test]
    fn removing_root_removes_thumbnail_cache_files_for_all_media() {
        let dir = tempfile::TempDir::new().unwrap();
        let cache_dir = dir.path().join("cache");
        std::fs::create_dir_all(&cache_dir).unwrap();
        let db = Database::open_in_memory().unwrap();
        let root_id = db.add_source_root("/media", "/media").unwrap();
        let first_id = insert_image(&db, root_id, "/media/a.jpg", 1, 1);
        let second_id = insert_image(&db, root_id, "/media/b.jpg", 1, 1);
        let first_path = write_cache_file(&cache_dir, "first", 8);
        let second_path = write_cache_file(&cache_dir, "second", 8);
        db.set_thumbnail_success(first_id, "first", first_path.to_str().unwrap(), 1, None)
            .unwrap();
        db.set_thumbnail_success(second_id, "second", second_path.to_str().unwrap(), 1, None)
            .unwrap();

        remove_root_and_cache(&db, &cache_dir, root_id).unwrap();

        assert!(!first_path.exists());
        assert!(!second_path.exists());
        assert!(db.list_source_roots().unwrap().is_empty());
        assert_eq!(db.count_media().unwrap(), 0);
    }
}
