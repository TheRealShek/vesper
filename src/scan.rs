//! Scan coordinator: wires `index/` walker → `db/` storage.
//!
//! Runs a full scan of a source root, upserting discovered media,
//! deriving tags from folder structure, and cleaning up files that
//! no longer exist on disk.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};

use crate::backend::concurrency::Cancellation;
use crate::db::{Database, MediaEntry, ScanErrorEntry, TagIdentity, system_time_to_epoch};
use crate::events::{ChannelSendExt, DiscoveredMedia, ScanEvent};
use crate::index;

/// Summary of a completed scan operation.
#[derive(Debug)]
#[allow(dead_code)]
pub struct ScanResult {
    /// The source root that was scanned.
    pub root: PathBuf,
    /// Total media files found on disk.
    pub files_found: u64,
    /// Files upserted into the database.
    pub files_upserted: u64,
    /// Files removed from the database (no longer on disk).
    pub files_removed: u64,
    /// Paths that failed to be processed or inserted.
    pub failed_paths: Vec<String>,
}

/// Runs a full scan of a source root directory.
///
/// 1. Walks the filesystem (blocking, on a spawned thread).
/// 2. Upserts discovered media into the database.
/// 3. Derives and syncs tags from folder structure.
/// 4. Removes DB entries for files no longer on disk.
///
/// Individual file errors are counted but do not abort the scan.
/// The future is `'static` and safe to `tokio::spawn`.
pub async fn run_scan(
    root: PathBuf,
    db: Arc<Database>,
    global_patterns: Vec<String>,
    root_as_tag: bool,
    ui_tx: tokio::sync::mpsc::Sender<crate::ui::window::UiEvent>,
) -> Result<ScanResult> {
    let global_rules = index::build_global_rules(&global_patterns)
        .context("failed to build global ignore rules")?;

    // Ensure source root exists in DB.
    let root_str = root
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("source root path is not valid UTF-8"))?
        .to_owned();

    let (source_root_id, scan_gen) = {
        let db = &*db;
        let root_id = match db
            .find_source_root_by_path(&root_str)
            .context("failed to look up source root")?
        {
            Some(sr) => sr.id,
            None => db
                .add_source_root(&root_str, &root_str)
                .context("failed to add source root")?,
        };
        // Replaces O(n) in-memory HashSets for stale file tracking to prevent unbounded memory growth.
        let new_scan_gen = db.get_max_scan_generation(root_id).unwrap_or(0) + 1;
        (root_id, new_scan_gen)
    };

    // Channel: walker (blocking) → coordinator (async).
    // Bounded streaming channel limits memory pressure; files process as fast as I/O allows.
    let (tx, mut rx) = tokio::sync::mpsc::channel::<ScanEvent>(1024);
    let scan_root = root.clone();
    // All source-root boundaries, so the walker can reject file symlinks whose
    // target resolves outside every root (I-2).
    let source_roots = source_root_boundaries(&db);

    // Spawn the blocking filesystem walker.
    // When this closure returns, `tx` is dropped, closing the channel.
    // Runs on the spawn_blocking pool because ignore::Walk relies entirely on blocking OS filesystem APIs.
    let walker_handle = tokio::task::spawn_blocking(move || {
        index::scan_source_root(&scan_root, &global_rules, Vec::new(), &source_roots, &tx)
    });

    // Process events as they stream in.
    let mut files_upserted: u64 = 0;
    let mut failed_paths: Vec<String> = Vec::new();
    // Persisted to the scan_errors table at the end of the scan (A-4).
    let mut scan_errors: Vec<ScanErrorEntry> = Vec::new();
    // Batching amortizes SQLite transaction overhead while bounding RAM footprint.
    let mut batch_buffer: Vec<(MediaEntry, Vec<TagIdentity>)> = Vec::with_capacity(500);

    let mut files_found_count: usize = 0;

    while let Some(event) = rx.recv().await {
        match event {
            ScanEvent::FileFound(media) => {
                files_found_count += 1;
                if files_found_count.is_multiple_of(50) {
                    ui_tx.send_log(crate::ui::window::UiEvent::ScanProgress(files_found_count));
                }

                match prepare_file_entry(&media, &root, source_root_id, root_as_tag, scan_gen) {
                    Ok((_, entry, tags)) => {
                        batch_buffer.push((entry, tags));

                        if batch_buffer.len() >= 500 {
                            retain_existing_files(&mut batch_buffer);
                            let db_guard = &*db;
                            if let Err(e) = db_guard.upsert_media_batch(&batch_buffer, scan_gen) {
                                tracing::error!(error = %e, "batch upsert failed");
                                let message = e.to_string();
                                for (m, _) in &batch_buffer {
                                    failed_paths.push(m.path.clone());
                                    scan_errors.push(scan_error(
                                        source_root_id,
                                        scan_gen,
                                        m.path.clone(),
                                        "database",
                                        message.clone(),
                                    ));
                                }
                            } else {
                                files_upserted += batch_buffer.len() as u64;
                            }
                            batch_buffer.clear();
                        }
                    }
                    Err(e) => {
                        let path_str = media.path.display().to_string();
                        failed_paths.push(path_str.clone());
                        scan_errors.push(scan_error(
                            source_root_id,
                            scan_gen,
                            path_str,
                            "unreadable",
                            e.to_string(),
                        ));
                    }
                }
            }
            ScanEvent::Error { path, message } => {
                let path_str = path.display().to_string();
                failed_paths.push(path_str.clone());
                scan_errors.push(scan_error(
                    source_root_id,
                    scan_gen,
                    path_str,
                    "unreadable",
                    message,
                ));
            }
            ScanEvent::Started { .. } => {
                ui_tx.send_critical(crate::ui::window::UiEvent::ScanStarted);
            }
            ScanEvent::Completed { .. } => {}
        }
    }

    if !batch_buffer.is_empty() {
        retain_existing_files(&mut batch_buffer);
        let db_guard = &*db;
        if let Err(e) = db_guard.upsert_media_batch(&batch_buffer, scan_gen) {
            tracing::error!(error = %e, "batch upsert failed");
            let message = e.to_string();
            for (m, _) in &batch_buffer {
                failed_paths.push(m.path.clone());
                scan_errors.push(scan_error(
                    source_root_id,
                    scan_gen,
                    m.path.clone(),
                    "database",
                    message.clone(),
                ));
            }
        } else {
            files_upserted += batch_buffer.len() as u64;
        }
    }

    // Channel exhausted — walker finished. Collect its result.
    let walker_result = walker_handle.await.context("walker task panicked")?;
    let summary = walker_result.map_err(|e| anyhow::anyhow!("walker failed: {e}"))?;
    let files_found = summary.files_found;

    // I-6 / 02 §5: a partial walk (a directory could not be read — permissions,
    // or the root going offline mid-scan) must never drive the deletion sweep.
    // Its undiscovered files were unreachable, not deleted, so we keep every
    // existing record for this generation and skip reconciliation entirely.
    let files_removed = if summary.partial {
        tracing::warn!(
            root = %crate::logging::redact_path(&root),
            "partial scan: skipping stale-media sweep"
        );
        0
    } else {
        let db = &*db;
        let removed = db
            .remove_stale_media(source_root_id, scan_gen)
            .context("failed to remove stale media")?;
        db.cleanup_orphaned_tags()
            .context("failed to clean up orphaned tags")?;
        removed as u64
    };

    // Replace this root's error set: clear all, then record the current scan's
    // failures, so paths that have since succeeded or disappeared no longer
    // surface an error (A-4).
    {
        let db = &*db;
        if let Err(e) = db.clear_scan_errors_for_root(source_root_id) {
            tracing::error!(error = %e, "failed to clear scan errors");
        }
        if let Err(e) = db.record_scan_errors(&scan_errors) {
            tracing::error!(error = %e, "failed to record scan errors");
        }
    }

    crate::logging::scan_completed(&root, files_found, files_upserted, files_removed);
    Ok(ScanResult {
        root,
        files_found,
        files_upserted,
        files_removed,
        failed_paths,
    })
}

/// Runs a scan on a specific subtree, preserving database entries outside of it.
///
/// `cancel` lets an in-flight subtree scan be dropped when its owning root is
/// removed (B-7): once it fires, the scan stops consuming walker events and
/// returns without running stale-media cleanup (a partial scan must never delete
/// records it did not reach). Pass [`Cancellation::never`] when there is nothing
/// to cancel against.
pub async fn run_subtree_scan(
    subtree: PathBuf,
    db: Arc<Database>,
    global_patterns: Vec<String>,
    root_as_tag: bool,
    ui_tx: tokio::sync::mpsc::Sender<crate::ui::window::UiEvent>,
    cancel: Cancellation,
) -> Result<ScanResult> {
    let global_rules = index::build_global_rules(&global_patterns)
        .context("failed to build global ignore rules")?;

    let (source_root_id, source_root_path, scan_gen) = {
        let db = &*db;
        let roots = db.list_source_roots().context("failed to list roots")?;
        let root = roots
            .into_iter()
            .filter(|r| subtree.starts_with(&r.path))
            .max_by_key(|r| std::path::Path::new(&r.path).components().count())
            .ok_or_else(|| anyhow::anyhow!("subtree does not belong to any source root"))?;
        let new_scan_gen = db.get_max_scan_generation(root.id).unwrap_or(0) + 1;
        (root.id, PathBuf::from(root.path), new_scan_gen)
    };

    let (tx, mut rx) = tokio::sync::mpsc::channel::<ScanEvent>(1024);
    let scan_subtree = subtree.clone();

    let mut initial_ignore_stack = Vec::new();
    // Preload ancestors to ensure .galleryignore rules from parent directories cascade into the subtree.
    if let Ok(relative) = scan_subtree.strip_prefix(&source_root_path) {
        let mut path_to_check = source_root_path.clone();
        for component in relative.components() {
            if let Ok(Some(rules)) = index::ignore_rules::load_directory_rules(&path_to_check) {
                initial_ignore_stack.push(rules);
            }
            path_to_check.push(component);
        }
    }

    // All source-root boundaries for the file-symlink boundary check (I-2).
    let source_roots = source_root_boundaries(&db);

    let walker_handle = tokio::task::spawn_blocking(move || {
        index::scan_source_root(
            &scan_subtree,
            &global_rules,
            initial_ignore_stack,
            &source_roots,
            &tx,
        )
    });

    let mut files_upserted: u64 = 0;
    let mut failed_paths: Vec<String> = Vec::new();
    // Persisted to the scan_errors table at the end of the scan (A-4).
    let mut scan_errors: Vec<ScanErrorEntry> = Vec::new();
    let mut batch_buffer: Vec<(MediaEntry, Vec<TagIdentity>)> = Vec::with_capacity(500);

    let mut files_found_count: usize = 0;

    while let Some(event) = rx.recv().await {
        // B-7: the owning root was removed (or otherwise invalidated) while this
        // scan was in flight. Stop consuming events and drop the receiver so the
        // walker unwinds on its next send; return the partial counts without
        // running stale-media cleanup, which must not act on an incomplete scan.
        if cancel.is_cancelled() {
            return Ok(ScanResult {
                root: subtree,
                files_found: 0,
                files_upserted,
                files_removed: 0,
                failed_paths,
            });
        }
        match event {
            ScanEvent::FileFound(media) => {
                files_found_count += 1;
                if files_found_count.is_multiple_of(50) {
                    ui_tx.send_log(crate::ui::window::UiEvent::ScanProgress(files_found_count));
                }

                match prepare_file_entry(
                    &media,
                    &source_root_path,
                    source_root_id,
                    root_as_tag,
                    scan_gen,
                ) {
                    Ok((_, entry, tags)) => {
                        batch_buffer.push((entry, tags));

                        if batch_buffer.len() >= 500 {
                            retain_existing_files(&mut batch_buffer);
                            let db_guard = &*db;
                            if let Err(e) = db_guard.upsert_media_batch(&batch_buffer, scan_gen) {
                                tracing::error!(error = %e, "batch upsert failed");
                                let message = e.to_string();
                                for (m, _) in &batch_buffer {
                                    failed_paths.push(m.path.clone());
                                    scan_errors.push(scan_error(
                                        source_root_id,
                                        scan_gen,
                                        m.path.clone(),
                                        "database",
                                        message.clone(),
                                    ));
                                }
                            } else {
                                files_upserted += batch_buffer.len() as u64;
                            }
                            batch_buffer.clear();
                        }
                    }
                    Err(e) => {
                        let path_str = media.path.display().to_string();
                        failed_paths.push(path_str.clone());
                        scan_errors.push(scan_error(
                            source_root_id,
                            scan_gen,
                            path_str,
                            "unreadable",
                            e.to_string(),
                        ));
                    }
                }
            }
            ScanEvent::Error { path, message } => {
                let path_str = path.display().to_string();
                failed_paths.push(path_str.clone());
                scan_errors.push(scan_error(
                    source_root_id,
                    scan_gen,
                    path_str,
                    "unreadable",
                    message,
                ));
            }
            ScanEvent::Started { .. } => {
                ui_tx.send_critical(crate::ui::window::UiEvent::ScanStarted);
            }
            ScanEvent::Completed { .. } => {}
        }
    }

    if !batch_buffer.is_empty() {
        retain_existing_files(&mut batch_buffer);
        let db_guard = &*db;
        if let Err(e) = db_guard.upsert_media_batch(&batch_buffer, scan_gen) {
            tracing::error!(error = %e, "batch upsert failed");
            let message = e.to_string();
            for (m, _) in &batch_buffer {
                failed_paths.push(m.path.clone());
                scan_errors.push(scan_error(
                    source_root_id,
                    scan_gen,
                    m.path.clone(),
                    "database",
                    message.clone(),
                ));
            }
        } else {
            files_upserted += batch_buffer.len() as u64;
        }
    }

    let walker_result = walker_handle.await.context("walker task panicked")?;
    let summary = walker_result.map_err(|e| anyhow::anyhow!("walker failed: {e}"))?;
    let files_found = summary.files_found;

    // I-6 / 02 §5: a partial subtree walk (an unreadable directory) must not run
    // the deletion sweep — undiscovered files were unreachable, not deleted.
    let files_removed = if summary.partial {
        tracing::warn!(
            subtree = %crate::logging::redact_path(&subtree),
            "partial subtree scan: skipping stale-media sweep"
        );
        0
    } else {
        let db = &*db;
        let subtree_str = subtree.to_str().unwrap_or("");
        let removed = db
            .remove_stale_media_in_subtree(source_root_id, subtree_str, scan_gen)
            .context("failed to remove stale media in subtree")?;
        db.cleanup_orphaned_tags()
            .context("failed to clean up orphaned tags")?;
        removed as u64
    };

    // Replace the scanned subtree's error set: clear it, then record this scan's
    // failures, leaving errors elsewhere in the root untouched (A-4).
    {
        let db = &*db;
        let subtree_str = subtree.to_str().unwrap_or("");
        if let Err(e) = db.clear_scan_errors_in_subtree(source_root_id, subtree_str) {
            tracing::error!(error = %e, "failed to clear scan errors");
        }
        if let Err(e) = db.record_scan_errors(&scan_errors) {
            tracing::error!(error = %e, "failed to record scan errors");
        }
    }

    crate::logging::scan_completed(&subtree, files_found, files_upserted, files_removed);
    Ok(ScanResult {
        root: subtree,
        files_found,
        files_upserted,
        files_removed,
        failed_paths,
    })
}

/// Processes a single discovered file for live updates.
pub fn process_single_file(
    media: &DiscoveredMedia,
    source_root: &Path,
    source_root_id: i64,
    root_as_tag: bool,
    db: Arc<Database>,
) -> Result<()> {
    let scan_gen = {
        let db_guard = &*db;
        // Single file updates reuse max generation + 1.
        // A file created during a scan must never be removed by stale cleanup.
        // By assigning `max + 1`, live updates are guaranteed to have a generation
        // >= any currently running scan's generation. Stale cleanup removes strictly
        // `< scan_gen`, so concurrent live updates are preserved.
        db_guard
            .get_max_scan_generation(source_root_id)
            .unwrap_or(0)
            + 1
    };
    let (_, entry, tags) =
        prepare_file_entry(media, source_root, source_root_id, root_as_tag, scan_gen)?;
    let db_guard = &*db;
    db_guard.upsert_media_batch(&[(entry, tags)], scan_gen)?;
    Ok(())
}

/// Builds a [`ScanErrorEntry`] for a path that failed during the current scan.
fn scan_error(
    source_root_id: i64,
    scan_gen: i64,
    path: String,
    category: &str,
    message: String,
) -> ScanErrorEntry {
    ScanErrorEntry {
        source_root_id,
        scan_generation: scan_gen,
        path,
        category: category.to_string(),
        message,
    }
}

/// Prepares a media entry and derived tags for batch insertion.
/// Returns (path_str, entry, tags).
/// Canonical paths of all source roots, used by the walker to enforce the
/// file-symlink boundary rule (I-2): a symlink whose target resolves outside
/// every root is skipped. Best-effort — an unreadable roots table yields an
/// empty list, which conservatively rejects all symlink targets.
fn source_root_boundaries(db: &Database) -> Vec<PathBuf> {
    db.list_source_roots()
        .unwrap_or_default()
        .into_iter()
        .map(|r| PathBuf::from(r.path))
        .collect()
}

fn prepare_file_entry(
    media: &DiscoveredMedia,
    source_root: &Path,
    source_root_id: i64,
    root_as_tag: bool,
    _scan_gen: i64,
) -> Result<(String, MediaEntry, Vec<TagIdentity>)> {
    let path_str = media
        .path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("non-UTF-8 file path"))?
        .to_owned();

    let filename = media
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_owned();

    let mut tags = derive_tags(&media.path, source_root, source_root_id, root_as_tag);
    // Dedup by identity (relative folder path) to keep derive_tags pure and deterministic.
    let mut seen = HashSet::new();
    tags.retain(|tag| seen.insert(tag.relative_folder_path.clone()));

    // Path relative to the owning source root; falls back to the basename if the
    // file somehow isn't under the root (should not happen for walker output).
    let relative_path = media
        .path
        .strip_prefix(source_root)
        .ok()
        .and_then(|rel| rel.to_str())
        .map(|s| s.to_owned())
        .unwrap_or_else(|| filename.clone());

    // Canonical target path: resolves the file's own path for regular files and
    // the link target for file symlinks (02 §4). If resolution fails (e.g. the
    // file vanished between discovery and processing), fall back to the absolute
    // path so the row still carries a stable identity. Boundary/dedup handling
    // for symlinks (I-2) is intentionally not implemented here.
    let canonical_identity = std::fs::canonicalize(&media.path)
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_owned()))
        .unwrap_or_else(|| path_str.clone());

    let entry = MediaEntry {
        path: path_str.clone(),
        relative_path,
        canonical_identity,
        filename,
        source_root_id,
        media_type: media.media_type,
        size_bytes: media.size_bytes as i64,
        created_at: media.created.map(system_time_to_epoch),
        modified_at: system_time_to_epoch(media.modified),
    };

    Ok((path_str, entry, tags))
}

fn retain_existing_files(batch: &mut Vec<(MediaEntry, Vec<TagIdentity>)>) -> usize {
    let before = batch.len();
    batch.retain(|(entry, _)| {
        std::fs::metadata(&entry.path)
            .map(|metadata| metadata.is_file())
            .unwrap_or(false)
    });
    before - batch.len()
}

/// Derives path-qualified tag identities from the directory components between
/// the source root and the file. Each ancestor folder is its own tag, uniquely
/// keyed by `(source_root_id, relative_folder_path)`, so same-named folders in
/// different subtrees or roots stay distinct (A-2). The root directory itself is
/// excluded unless `root_as_tag` is set (spec section 6, default OFF).
///
/// ```text
/// source root: /home/user/media   (id 7)
/// file path:   /home/user/media/Travel/Japan/2023/photo.jpg
/// tags:        Travel (rel "Travel"), Japan (rel "Travel/Japan"), 2023 (rel "Travel/Japan/2023")
/// ```
fn derive_tags(
    file_path: &Path,
    source_root: &Path,
    source_root_id: i64,
    root_as_tag: bool,
) -> Vec<TagIdentity> {
    let relative = match file_path.strip_prefix(source_root) {
        Ok(rel) => rel,
        Err(_) => return Vec::new(),
    };

    let parent = match relative.parent() {
        Some(p) => p,
        None => return Vec::new(),
    };

    let root_name = source_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    let mut tags: Vec<TagIdentity> = Vec::new();

    // Spec decision: root name is excluded by default. When enabled, the root
    // folder itself is a tag with an empty relative path.
    if root_as_tag {
        tags.push(TagIdentity {
            source_root_id,
            relative_folder_path: String::new(),
            display_name: root_name.to_string(),
            display_path: root_name.to_string(),
        });
    }

    // Accumulate the relative path as we descend so each ancestor folder carries
    // its full lineage identity, not just its basename.
    let mut rel_accum = PathBuf::new();
    for component in parent.components() {
        if let Component::Normal(name) = component {
            let Some(name) = name.to_str() else { continue };
            rel_accum.push(name);
            let relative_folder_path = rel_accum.to_string_lossy().to_string();
            let display_path = if root_name.is_empty() {
                relative_folder_path.clone()
            } else {
                format!("{root_name}/{relative_folder_path}")
            };
            tags.push(TagIdentity {
                source_root_id,
                relative_folder_path,
                display_name: name.to_string(),
                display_path,
            });
        }
    }

    tags
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ── Unit tests for derive_tags ──────────────────────────────────

    /// Extracts (display_name, relative_folder_path) pairs for terse assertions.
    fn tag_pairs(tags: &[TagIdentity]) -> Vec<(&str, &str)> {
        tags.iter()
            .map(|t| (t.display_name.as_str(), t.relative_folder_path.as_str()))
            .collect()
    }

    #[test]
    fn tags_from_nested_path() {
        let root = PathBuf::from("/home/user/media");
        let file = PathBuf::from("/home/user/media/Travel/Japan/2023/photo.jpg");
        let tags = derive_tags(&file, &root, 7, false);
        assert_eq!(
            tag_pairs(&tags),
            vec![
                ("Travel", "Travel"),
                ("Japan", "Travel/Japan"),
                ("2023", "Travel/Japan/2023"),
            ]
        );
        assert!(tags.iter().all(|t| t.source_root_id == 7));
    }

    #[test]
    fn tags_empty_for_file_at_root() {
        let root = PathBuf::from("/media");
        let file = PathBuf::from("/media/photo.jpg");
        assert!(derive_tags(&file, &root, 1, false).is_empty());
    }

    #[test]
    fn tags_single_level() {
        let root = PathBuf::from("/media");
        let file = PathBuf::from("/media/Vacation/photo.jpg");
        assert_eq!(
            tag_pairs(&derive_tags(&file, &root, 1, false)),
            vec![("Vacation", "Vacation")]
        );
    }

    #[test]
    fn tags_empty_for_unrelated_path() {
        let root = PathBuf::from("/media");
        let file = PathBuf::from("/other/photo.jpg");
        assert!(derive_tags(&file, &root, 1, false).is_empty());
    }

    #[test]
    fn tags_with_root_as_tag() {
        let root = PathBuf::from("/media/MyPhotos");
        let file = PathBuf::from("/media/MyPhotos/Vacation/photo.jpg");
        let tags = derive_tags(&file, &root, 3, true);
        assert_eq!(
            tag_pairs(&tags),
            vec![("MyPhotos", ""), ("Vacation", "Vacation")]
        );
        // Root-as-tag carries the root's display name with an empty relative path.
        assert_eq!(tags[0].display_path, "MyPhotos");
    }

    #[test]
    fn retain_existing_files_skips_entry_deleted_before_batch_commit() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("photo.jpg");
        fs::write(&path, b"fake jpg").unwrap();

        let entry = MediaEntry {
            path: path.to_string_lossy().to_string(),
            relative_path: "photo.jpg".into(),
            canonical_identity: path.to_string_lossy().to_string(),
            filename: "photo.jpg".into(),
            source_root_id: 1,
            media_type: crate::events::MediaType::Image,
            size_bytes: 8,
            created_at: Some(1000),
            modified_at: 2000,
        };
        let mut batch = vec![(
            entry,
            vec![TagIdentity {
                source_root_id: 1,
                relative_folder_path: "Photos".into(),
                display_name: "Photos".into(),
                display_path: "Photos".into(),
            }],
        )];

        fs::remove_file(&path).unwrap();

        let skipped = retain_existing_files(&mut batch);

        assert_eq!(skipped, 1);
        assert!(batch.is_empty());
    }

    // ── Integration tests ───────────────────────────────────────────

    #[tokio::test]
    async fn full_scan_indexes_media_and_derives_tags() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().to_path_buf();

        fs::create_dir_all(root.join("Travel/Japan")).unwrap();
        fs::write(root.join("Travel/Japan/photo.jpg"), b"fake jpg").unwrap();
        fs::write(root.join("Travel/beach.png"), b"fake png").unwrap();
        fs::write(root.join("readme.txt"), b"not media").unwrap();

        let db = Arc::new(Database::open_in_memory().unwrap());
        let (ui_tx, _ui_rx) = tokio::sync::mpsc::channel(1);
        let result = run_scan(root, db.clone(), vec![], false, ui_tx)
            .await
            .unwrap();

        assert_eq!(result.files_found, 2);
        assert_eq!(result.files_upserted, 2);
        assert_eq!(result.files_removed, 0);
        assert_eq!(result.failed_paths.len(), 0);

        let db = &*db;
        let tags = db.get_all_tags_with_counts().unwrap();
        let names: Vec<&str> = tags.iter().map(|t| t.display_name.as_str()).collect();
        assert!(names.contains(&"Travel"));
        assert!(names.contains(&"Japan"));
    }

    #[tokio::test]
    async fn same_named_folders_in_different_roots_are_distinct_tags() {
        // Two roots each contain a "2023" folder; identity is per-root, so they
        // must not merge into one tag (A-2).
        let dir_a = TempDir::new().unwrap();
        let dir_b = TempDir::new().unwrap();
        fs::create_dir_all(dir_a.path().join("2023")).unwrap();
        fs::create_dir_all(dir_b.path().join("2023")).unwrap();
        fs::write(dir_a.path().join("2023/a.jpg"), b"a").unwrap();
        fs::write(dir_b.path().join("2023/b.jpg"), b"b").unwrap();

        let db = Arc::new(Database::open_in_memory().unwrap());
        let (ui_tx, _ui_rx) = tokio::sync::mpsc::channel(4);
        run_scan(
            dir_a.path().to_path_buf(),
            db.clone(),
            vec![],
            false,
            ui_tx.clone(),
        )
        .await
        .unwrap();
        run_scan(dir_b.path().to_path_buf(), db.clone(), vec![], false, ui_tx)
            .await
            .unwrap();

        let db = &*db;
        let tags = db.get_all_tags_with_counts().unwrap();
        let twenty_threes: Vec<_> = tags.iter().filter(|t| t.display_name == "2023").collect();
        assert_eq!(
            twenty_threes.len(),
            2,
            "two distinct '2023' tags expected, got {twenty_threes:?}"
        );
        assert_ne!(
            twenty_threes[0].source_root_id,
            twenty_threes[1].source_root_id
        );
    }

    #[tokio::test]
    async fn rescan_detects_removed_files() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().to_path_buf();

        fs::create_dir_all(root.join("Photos")).unwrap();
        fs::write(root.join("Photos/a.jpg"), b"a").unwrap();
        fs::write(root.join("Photos/b.jpg"), b"b").unwrap();

        let db = Arc::new(Database::open_in_memory().unwrap());

        let (ui_tx, _ui_rx) = tokio::sync::mpsc::channel(1);
        let r1 = run_scan(root.clone(), db.clone(), vec![], false, ui_tx.clone())
            .await
            .unwrap();
        assert_eq!(r1.files_found, 2);
        assert_eq!(r1.files_removed, 0);

        // Remove one file from disk.
        fs::remove_file(root.join("Photos/b.jpg")).unwrap();

        let r2 = run_scan(root, db, vec![], false, ui_tx).await.unwrap();
        assert_eq!(r2.files_found, 1);
        assert_eq!(r2.files_removed, 1);
    }

    #[tokio::test]
    async fn scan_respects_galleryignore() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().to_path_buf();

        fs::create_dir_all(root.join("Public")).unwrap();
        fs::create_dir_all(root.join("Private")).unwrap();
        fs::write(root.join("Public/a.jpg"), b"a").unwrap();
        fs::write(root.join("Private/b.jpg"), b"b").unwrap();
        fs::write(root.join(".galleryignore"), "Private/\n").unwrap();

        let db = Arc::new(Database::open_in_memory().unwrap());
        let (ui_tx, _ui_rx) = tokio::sync::mpsc::channel(1);
        let result = run_scan(root, db, vec![], false, ui_tx).await.unwrap();

        assert_eq!(result.files_found, 1);
    }

    #[tokio::test]
    async fn scan_respects_global_ignore_patterns() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().to_path_buf();

        fs::write(root.join("keep.jpg"), b"keep").unwrap();
        fs::write(root.join("drop.tmp"), b"drop").unwrap();

        let db = Arc::new(Database::open_in_memory().unwrap());
        let patterns = vec!["*.tmp".into()];
        let (ui_tx, _ui_rx) = tokio::sync::mpsc::channel(1);
        let result = run_scan(root, db, patterns, false, ui_tx).await.unwrap();

        // *.tmp is not a media extension anyway, but the ignore rule fires before classification.
        assert_eq!(result.files_found, 1);
    }

    #[tokio::test]
    async fn scan_does_not_follow_directory_symlinks() {
        // A directory symlink inside the root must not be traversed (I-1).
        let dir = TempDir::new().unwrap();
        let root = dir.path().to_path_buf();

        fs::write(root.join("real.jpg"), b"real").unwrap();

        // Target directory lives outside the root; a symlink inside points to it.
        let target = TempDir::new().unwrap();
        fs::write(target.path().join("linked.jpg"), b"linked").unwrap();
        std::os::unix::fs::symlink(target.path(), root.join("LinkDir")).unwrap();

        let db = Arc::new(Database::open_in_memory().unwrap());
        let (ui_tx, _ui_rx) = tokio::sync::mpsc::channel(1);
        let result = run_scan(root, db, vec![], false, ui_tx).await.unwrap();

        // Only the real file is discovered; the symlinked directory is skipped.
        assert_eq!(result.files_found, 1);
    }

    // ── A-3 populate + date_added semantics ─────────────────────────

    #[tokio::test]
    async fn scan_populates_relative_path_and_canonical_identity() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().to_path_buf();

        fs::create_dir_all(root.join("Sub")).unwrap();
        fs::write(root.join("Sub/photo.jpg"), b"fake jpg").unwrap();

        let db = Arc::new(Database::open_in_memory().unwrap());
        let (ui_tx, _ui_rx) = tokio::sync::mpsc::channel(1);
        run_scan(root.clone(), db.clone(), vec![], false, ui_tx)
            .await
            .unwrap();

        let reader = db.reader.lock().unwrap();
        let (relative_path, canonical_identity): (Option<String>, Option<String>) = reader
            .query_row(
                "SELECT relative_path, canonical_identity FROM media WHERE filename = ?1",
                ["photo.jpg"],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert_eq!(relative_path.as_deref(), Some("Sub/photo.jpg"));
        let canonical_identity =
            canonical_identity.expect("canonical_identity must be populated on a fresh index");
        assert!(
            canonical_identity.ends_with("photo.jpg"),
            "canonical_identity should resolve to the file, got: {canonical_identity}"
        );
    }

    #[tokio::test]
    async fn date_added_survives_rescan() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().to_path_buf();

        fs::write(root.join("photo.jpg"), b"fake jpg").unwrap();

        let db = Arc::new(Database::open_in_memory().unwrap());
        let (ui_tx, _ui_rx) = tokio::sync::mpsc::channel(4);
        run_scan(root.clone(), db.clone(), vec![], false, ui_tx.clone())
            .await
            .unwrap();

        // Stamp a known sentinel so a reset is observable regardless of the
        // one-second resolution of strftime('%s','now').
        const SENTINEL: i64 = 100_000;
        {
            let writer = db.writer.lock().unwrap();
            let updated = writer
                .execute(
                    "UPDATE media SET date_added = ?1 WHERE filename = ?2",
                    rusqlite::params![SENTINEL, "photo.jpg"],
                )
                .unwrap();
            assert_eq!(updated, 1, "expected exactly one indexed row to stamp");
        }

        // Rescan the same, unchanged file.
        run_scan(root, db.clone(), vec![], false, ui_tx)
            .await
            .unwrap();

        let reader = db.reader.lock().unwrap();
        let date_added: i64 = reader
            .query_row(
                "SELECT date_added FROM media WHERE filename = ?1",
                ["photo.jpg"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            date_added, SENTINEL,
            "date_added must be preserved across a rescan"
        );
    }

    // ── A-4 scan_errors wiring ──────────────────────────────────────

    /// Seeds a decoy media row whose `canonical_identity` equals the real file's,
    /// so the scan's insert trips the unique `canonical_identity` constraint and
    /// the batch fails — a deterministic scan failure for `photo.jpg`.
    fn seed_canonical_collision(db: &Database, root: &std::path::Path, root_id: i64) {
        let canonical = std::fs::canonicalize(root.join("photo.jpg"))
            .unwrap()
            .to_string_lossy()
            .to_string();
        let decoy = MediaEntry {
            path: "/decoy/x.jpg".into(),
            relative_path: "x.jpg".into(),
            canonical_identity: canonical,
            filename: "x.jpg".into(),
            source_root_id: root_id,
            media_type: crate::events::MediaType::Image,
            size_bytes: 1,
            created_at: None,
            modified_at: 1000,
        };
        db.upsert_media_batch(&[(decoy, vec![])], 1).unwrap();
    }

    #[tokio::test]
    async fn scan_error_recorded_on_scan_failure() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().to_path_buf();
        fs::write(root.join("photo.jpg"), b"fake jpg").unwrap();

        let db = Arc::new(Database::open_in_memory().unwrap());
        let root_str = root.to_str().unwrap().to_string();
        let root_id = db.add_source_root(&root_str, &root_str).unwrap();
        seed_canonical_collision(&db, &root, root_id);

        let (ui_tx, _ui_rx) = tokio::sync::mpsc::channel(4);
        let result = run_scan(root.clone(), db.clone(), vec![], false, ui_tx)
            .await
            .unwrap();

        assert!(!result.failed_paths.is_empty());
        let error_paths = db.get_scan_error_paths().unwrap();
        let expected = root.join("photo.jpg").to_string_lossy().to_string();
        assert!(
            error_paths.contains(&expected),
            "expected scan_errors to contain {expected}, got {error_paths:?}"
        );
    }

    #[tokio::test]
    async fn scan_error_cleared_on_next_successful_scan() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().to_path_buf();
        fs::write(root.join("photo.jpg"), b"fake jpg").unwrap();

        let db = Arc::new(Database::open_in_memory().unwrap());
        let root_str = root.to_str().unwrap().to_string();
        let root_id = db.add_source_root(&root_str, &root_str).unwrap();
        seed_canonical_collision(&db, &root, root_id);

        let (ui_tx, _ui_rx) = tokio::sync::mpsc::channel(8);

        // First scan fails on the canonical collision and records the error.
        run_scan(root.clone(), db.clone(), vec![], false, ui_tx.clone())
            .await
            .unwrap();
        assert_eq!(db.count_scan_errors_for_root(root_id).unwrap(), 1);

        // Drop the decoy so the next scan of photo.jpg succeeds, then rescan: the
        // stale error from the older generation must be cleared.
        db.remove_media_by_path("/decoy/x.jpg").unwrap();
        run_scan(root, db.clone(), vec![], false, ui_tx)
            .await
            .unwrap();
        assert_eq!(db.count_scan_errors_for_root(root_id).unwrap(), 0);
    }

    #[tokio::test]
    async fn subtree_scan_stops_producing_when_root_removed_mid_scan() {
        use crate::backend::concurrency::BackendConcurrency;

        let dir = TempDir::new().unwrap();
        let root = dir.path().to_path_buf();
        fs::create_dir_all(root.join("sub")).unwrap();
        for i in 0..5 {
            fs::write(root.join(format!("sub/f{i}.jpg")), b"jpg").unwrap();
        }

        let db = Arc::new(Database::open_in_memory().unwrap());
        let root_str = root.to_str().unwrap();
        let root_id = db.add_source_root(root_str, root_str).unwrap();

        // A subtree scan job captures the root's current generation...
        let coord = BackendConcurrency::new();
        let generation = coord.current_generation(root_id);
        let cancel = coord.cancellation(root_id, generation);

        // ...then the root is removed, bumping the generation and invalidating
        // this in-flight job's token (B-7) before it consumes walker output.
        coord.invalidate_root(root_id);

        let (ui_tx, _ui_rx) = tokio::sync::mpsc::channel(64);
        let result = run_subtree_scan(root.join("sub"), db.clone(), vec![], false, ui_tx, cancel)
            .await
            .unwrap();

        // The stale scan dropped its results rather than producing records.
        assert_eq!(result.files_upserted, 0, "a stale scan must not upsert");
        assert_eq!(
            db.count_media().unwrap(),
            0,
            "removing the root mid-scan stops its walker from producing results"
        );
    }

    #[tokio::test]
    async fn subtree_scan_runs_to_completion_when_not_cancelled() {
        // Control for the cancellation test: with a live token the same scan
        // indexes its files normally.
        let dir = TempDir::new().unwrap();
        let root = dir.path().to_path_buf();
        fs::create_dir_all(root.join("sub")).unwrap();
        for i in 0..5 {
            fs::write(root.join(format!("sub/f{i}.jpg")), b"jpg").unwrap();
        }

        let db = Arc::new(Database::open_in_memory().unwrap());
        let root_str = root.to_str().unwrap();
        db.add_source_root(root_str, root_str).unwrap();

        let (ui_tx, _ui_rx) = tokio::sync::mpsc::channel(64);
        let result = run_subtree_scan(
            root.join("sub"),
            db.clone(),
            vec![],
            false,
            ui_tx,
            Cancellation::never(),
        )
        .await
        .unwrap();

        assert_eq!(result.files_upserted, 5);
        assert_eq!(db.count_media().unwrap(), 5);
    }

    // ── I-6: partial scans must not run the stale-media sweep ────────

    #[tokio::test]
    async fn partial_scan_skips_stale_media_sweep() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new().unwrap();
        let root = dir.path().to_path_buf();
        fs::create_dir_all(root.join("sub1")).unwrap();
        fs::create_dir_all(root.join("sub2")).unwrap();
        fs::write(root.join("sub1/a.jpg"), b"a").unwrap();
        fs::write(root.join("sub2/b.jpg"), b"b").unwrap();

        let db = Arc::new(Database::open_in_memory().unwrap());
        let root_str = root.to_str().unwrap().to_string();
        db.add_source_root(&root_str, &root_str).unwrap();
        let (ui_tx, _ui_rx) = tokio::sync::mpsc::channel(64);

        // A clean first scan indexes both files.
        run_scan(root.clone(), db.clone(), vec![], false, ui_tx.clone())
            .await
            .unwrap();
        assert_eq!(db.count_media().unwrap(), 2);

        // Make sub2 unreadable so the next walk fails to read that subtree.
        let sub2 = root.join("sub2");
        fs::set_permissions(&sub2, std::fs::Permissions::from_mode(0o000)).unwrap();

        let result = run_scan(root.clone(), db.clone(), vec![], false, ui_tx).await;

        // Restore permissions so the TempDir can be cleaned up regardless.
        fs::set_permissions(&sub2, std::fs::Permissions::from_mode(0o755)).unwrap();
        let result = result.unwrap();

        // The partial walk must not sweep: b.jpg was unreachable, not deleted.
        assert_eq!(
            result.files_removed, 0,
            "a partial scan must not delete anything"
        );
        assert_eq!(
            db.count_media().unwrap(),
            2,
            "records under the unreadable subtree are preserved"
        );
    }

    #[tokio::test]
    async fn clean_scan_runs_stale_media_sweep() {
        // No-regression control: a fully readable scan still reconciles genuine
        // deletions via the stale-media sweep.
        let dir = TempDir::new().unwrap();
        let root = dir.path().to_path_buf();
        fs::write(root.join("a.jpg"), b"a").unwrap();
        fs::write(root.join("b.jpg"), b"b").unwrap();

        let db = Arc::new(Database::open_in_memory().unwrap());
        let root_str = root.to_str().unwrap().to_string();
        db.add_source_root(&root_str, &root_str).unwrap();
        let (ui_tx, _ui_rx) = tokio::sync::mpsc::channel(64);

        run_scan(root.clone(), db.clone(), vec![], false, ui_tx.clone())
            .await
            .unwrap();
        assert_eq!(db.count_media().unwrap(), 2);

        // b.jpg is genuinely removed from disk; a clean rescan must sweep it.
        fs::remove_file(root.join("b.jpg")).unwrap();
        let result = run_scan(root.clone(), db.clone(), vec![], false, ui_tx)
            .await
            .unwrap();

        assert_eq!(
            result.files_removed, 1,
            "a clean scan still reconciles genuine deletions"
        );
        assert_eq!(db.count_media().unwrap(), 1);
    }
}
