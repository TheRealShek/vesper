//! Scan coordinator: wires `index/` walker → `db/` storage.
//!
//! Runs a full scan of a source root, upserting discovered media,
//! deriving tags from folder structure, and cleaning up files that
//! no longer exist on disk.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use anyhow::{Context, Result};

use crate::db::{Database, MediaEntry, system_time_to_epoch};
use crate::events::{DiscoveredMedia, ScanEvent};
use crate::index;

/// Summary of a completed scan operation.
#[derive(Debug)]
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
    db: Arc<Mutex<Database>>,
    global_patterns: Vec<String>,
    root_as_tag: bool,
) -> Result<ScanResult> {
    let global_rules = index::build_global_rules(&global_patterns)
        .context("failed to build global ignore rules")?;

    // Ensure source root exists in DB.
    let root_str = root
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("source root path is not valid UTF-8"))?
        .to_owned();

    let source_root_id = {
        let db = db
            .lock()
            .map_err(|_| anyhow::anyhow!("database lock poisoned"))?;
        match db
            .find_source_root_by_path(&root_str)
            .context("failed to look up source root")?
        {
            Some(sr) => sr.id,
            None => db
                .add_source_root(&root_str, &root_str)
                .context("failed to add source root")?,
        }
    };

    // Snapshot previously indexed paths for removal detection.
    let previously_indexed: HashSet<String> = {
        let db = db
            .lock()
            .map_err(|_| anyhow::anyhow!("database lock poisoned"))?;
        db.get_all_paths_for_root(source_root_id)
            .context("failed to get indexed paths")?
            .into_iter()
            .collect()
    };

    // Channel: walker (blocking) → coordinator (async).
    let (tx, mut rx) = tokio::sync::mpsc::channel::<ScanEvent>(1024);
    let scan_root = root.clone();

    // Spawn the blocking filesystem walker.
    // When this closure returns, `tx` is dropped, closing the channel.
    let walker_handle = tokio::task::spawn_blocking(move || {
        index::scan_source_root(&scan_root, &global_rules, Vec::new(), &tx)
    });

    // Process events as they stream in.
    let mut scanned_paths: HashSet<String> = HashSet::new();
    let mut files_upserted: u64 = 0;
    let mut failed_paths: Vec<String> = Vec::new();
    let mut batch_buffer: Vec<(MediaEntry, Vec<String>)> = Vec::with_capacity(500);

    while let Some(event) = rx.recv().await {
        match event {
            ScanEvent::FileFound(media) => {
                match prepare_file_entry(&media, &root, source_root_id, root_as_tag) {
                    Ok((path_str, entry, tags)) => {
                        scanned_paths.insert(path_str);
                        batch_buffer.push((entry, tags));

                        if batch_buffer.len() >= 500 {
                            let db_guard = db
                                .lock()
                                .map_err(|_| anyhow::anyhow!("database lock poisoned"))?;
                            if let Err(e) = db_guard.upsert_media_batch(&batch_buffer) {
                                eprintln!("batch upsert failed: {e}");
                                failed_paths
                                    .extend(batch_buffer.iter().map(|(m, _)| m.path.clone()));
                            } else {
                                files_upserted += batch_buffer.len() as u64;
                            }
                            batch_buffer.clear();
                        }
                    }
                    Err(_) => {
                        failed_paths.push(media.path.display().to_string());
                    }
                }
            }
            ScanEvent::Error { path, .. } => {
                failed_paths.push(path.display().to_string());
            }
            ScanEvent::Started { .. }
            | ScanEvent::Completed { .. }
            | ScanEvent::FileRemoved { .. } => {}
        }
    }

    if !batch_buffer.is_empty() {
        let db_guard = db
            .lock()
            .map_err(|_| anyhow::anyhow!("database lock poisoned"))?;
        if let Err(e) = db_guard.upsert_media_batch(&batch_buffer) {
            eprintln!("batch upsert failed: {e}");
            failed_paths.extend(batch_buffer.iter().map(|(m, _)| m.path.clone()));
        } else {
            files_upserted += batch_buffer.len() as u64;
        }
    }

    // Channel exhausted — walker finished. Collect its result.
    let walker_result = walker_handle.await.context("walker task panicked")?;
    let files_found = walker_result.map_err(|e| anyhow::anyhow!("walker failed: {e}"))?;

    // Detect removals: paths in DB but absent from this scan.
    let removed_paths: Vec<&String> = previously_indexed
        .iter()
        .filter(|p| !scanned_paths.contains(p.as_str()))
        .collect();
    let files_removed = removed_paths.len() as u64;

    if !removed_paths.is_empty() {
        let db = db
            .lock()
            .map_err(|_| anyhow::anyhow!("database lock poisoned"))?;
        for path in &removed_paths {
            db.remove_media_by_path(path)
                .context("failed to remove stale media entry")?;
        }
        db.cleanup_orphaned_tags()
            .context("failed to clean up orphaned tags")?;
    }

    Ok(ScanResult {
        root,
        files_found,
        files_upserted,
        files_removed,
        failed_paths,
    })
}

/// Runs a scan on a specific subtree, preserving database entries outside of it.
pub async fn run_subtree_scan(
    subtree: PathBuf,
    db: Arc<Mutex<Database>>,
    global_patterns: Vec<String>,
    root_as_tag: bool,
) -> Result<ScanResult> {
    let global_rules = index::build_global_rules(&global_patterns)
        .context("failed to build global ignore rules")?;

    let (source_root_id, source_root_path) = {
        let db = db
            .lock()
            .map_err(|_| anyhow::anyhow!("database lock poisoned"))?;
        let roots = db.list_source_roots().context("failed to list roots")?;
        let root = roots
            .into_iter()
            .filter(|r| subtree.starts_with(&r.path))
            .max_by_key(|r| std::path::Path::new(&r.path).components().count())
            .ok_or_else(|| anyhow::anyhow!("subtree does not belong to any source root"))?;
        (root.id, PathBuf::from(root.path))
    };

    let previously_indexed: HashSet<String> = {
        let db = db
            .lock()
            .map_err(|_| anyhow::anyhow!("database lock poisoned"))?;
        db.get_all_paths_for_root(source_root_id)
            .context("failed to get indexed paths")?
            .into_iter()
            .filter(|p| p.starts_with(subtree.to_str().unwrap_or("")))
            .collect()
    };

    let (tx, mut rx) = tokio::sync::mpsc::channel::<ScanEvent>(1024);
    let scan_subtree = subtree.clone();

    let mut initial_ignore_stack = Vec::new();
    if let Ok(relative) = scan_subtree.strip_prefix(&source_root_path) {
        let mut path_to_check = source_root_path.clone();
        for component in relative.components() {
            if let Ok(Some(rules)) = index::ignore_rules::load_directory_rules(&path_to_check) {
                initial_ignore_stack.push(rules);
            }
            path_to_check.push(component);
        }
    }

    let walker_handle = tokio::task::spawn_blocking(move || {
        index::scan_source_root(&scan_subtree, &global_rules, initial_ignore_stack, &tx)
    });

    let mut scanned_paths: HashSet<String> = HashSet::new();
    let mut files_upserted: u64 = 0;
    let mut failed_paths: Vec<String> = Vec::new();
    let mut batch_buffer: Vec<(MediaEntry, Vec<String>)> = Vec::with_capacity(500);

    while let Some(event) = rx.recv().await {
        match event {
            ScanEvent::FileFound(media) => {
                match prepare_file_entry(&media, &source_root_path, source_root_id, root_as_tag) {
                    Ok((path_str, entry, tags)) => {
                        scanned_paths.insert(path_str);
                        batch_buffer.push((entry, tags));

                        if batch_buffer.len() >= 500 {
                            let db_guard = db
                                .lock()
                                .map_err(|_| anyhow::anyhow!("database lock poisoned"))?;
                            if let Err(e) = db_guard.upsert_media_batch(&batch_buffer) {
                                eprintln!("batch upsert failed: {e}");
                                failed_paths
                                    .extend(batch_buffer.iter().map(|(m, _)| m.path.clone()));
                            } else {
                                files_upserted += batch_buffer.len() as u64;
                            }
                            batch_buffer.clear();
                        }
                    }
                    Err(_) => failed_paths.push(media.path.display().to_string()),
                }
            }
            ScanEvent::Error { path, .. } => {
                failed_paths.push(path.display().to_string());
            }
            ScanEvent::Started { .. }
            | ScanEvent::Completed { .. }
            | ScanEvent::FileRemoved { .. } => {}
        }
    }

    if !batch_buffer.is_empty() {
        let db_guard = db
            .lock()
            .map_err(|_| anyhow::anyhow!("database lock poisoned"))?;
        if let Err(e) = db_guard.upsert_media_batch(&batch_buffer) {
            eprintln!("batch upsert failed: {e}");
            failed_paths.extend(batch_buffer.iter().map(|(m, _)| m.path.clone()));
        } else {
            files_upserted += batch_buffer.len() as u64;
        }
    }

    let walker_result = walker_handle.await.context("walker task panicked")?;
    let files_found = walker_result.map_err(|e| anyhow::anyhow!("walker failed: {e}"))?;

    let removed_paths: Vec<&String> = previously_indexed
        .iter()
        .filter(|p| !scanned_paths.contains(p.as_str()))
        .collect();
    let files_removed = removed_paths.len() as u64;

    if !removed_paths.is_empty() {
        let db = db
            .lock()
            .map_err(|_| anyhow::anyhow!("database lock poisoned"))?;
        for path in &removed_paths {
            db.remove_media_by_path(path)
                .context("failed to remove stale media entry")?;
        }
        db.cleanup_orphaned_tags()
            .context("failed to clean up orphaned tags")?;
    }

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
    db: Arc<Mutex<Database>>,
) -> Result<()> {
    let (_, entry, tags) = prepare_file_entry(media, source_root, source_root_id, root_as_tag)?;
    let db_guard = db
        .lock()
        .map_err(|_| anyhow::anyhow!("database lock poisoned"))?;
    db_guard.upsert_media_batch(&[(entry, tags)])?;
    Ok(())
}

/// Prepares a media entry and derived tags for batch insertion.
/// Returns (path_str, entry, tags).
fn prepare_file_entry(
    media: &DiscoveredMedia,
    source_root: &Path,
    source_root_id: i64,
    root_as_tag: bool,
) -> Result<(String, MediaEntry, Vec<String>)> {
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

    let mut tags = derive_tags(&media.path, source_root, root_as_tag);
    let mut seen = HashSet::new();
    tags.retain(|tag| seen.insert(tag.clone()));

    let entry = MediaEntry {
        path: path_str.clone(),
        filename,
        source_root_id,
        media_type: media.media_type,
        size_bytes: media.size_bytes as i64,
        created_at: media.created.map(system_time_to_epoch),
        modified_at: system_time_to_epoch(media.modified),
        indexed_at: system_time_to_epoch(SystemTime::now()),
    };

    Ok((path_str, entry, tags))
}

/// Derives tag names from the directory components between the source root
/// and the file. The root directory name itself is excluded (spec section 6,
/// default: root-as-tag OFF).
///
/// ```text
/// source root: /home/user/media
/// file path:   /home/user/media/Travel/Japan/2023/photo.jpg
/// tags:        ["Travel", "Japan", "2023"]
/// ```
fn derive_tags(file_path: &Path, source_root: &Path, root_as_tag: bool) -> Vec<String> {
    let relative = match file_path.strip_prefix(source_root) {
        Ok(rel) => rel,
        Err(_) => return Vec::new(),
    };

    let parent = match relative.parent() {
        Some(p) => p,
        None => return Vec::new(),
    };

    let mut tags: Vec<String> = parent
        .components()
        .filter_map(|c| {
            if let Component::Normal(name) = c {
                name.to_str().map(String::from)
            } else {
                None
            }
        })
        .collect();

    if root_as_tag && let Some(name) = source_root.file_name().and_then(|n| n.to_str()) {
        tags.insert(0, name.to_string());
    }

    tags
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ── Unit tests for derive_tags ──────────────────────────────────

    #[test]
    fn tags_from_nested_path() {
        let root = PathBuf::from("/home/user/media");
        let file = PathBuf::from("/home/user/media/Travel/Japan/2023/photo.jpg");
        assert_eq!(
            derive_tags(&file, &root, false),
            vec!["Travel", "Japan", "2023"]
        );
    }

    #[test]
    fn tags_empty_for_file_at_root() {
        let root = PathBuf::from("/media");
        let file = PathBuf::from("/media/photo.jpg");
        assert!(derive_tags(&file, &root, false).is_empty());
    }

    #[test]
    fn tags_single_level() {
        let root = PathBuf::from("/media");
        let file = PathBuf::from("/media/Vacation/photo.jpg");
        assert_eq!(derive_tags(&file, &root, false), vec!["Vacation"]);
    }

    #[test]
    fn tags_empty_for_unrelated_path() {
        let root = PathBuf::from("/media");
        let file = PathBuf::from("/other/photo.jpg");
        assert!(derive_tags(&file, &root, false).is_empty());
    }

    #[test]
    fn tags_with_root_as_tag() {
        let root = PathBuf::from("/media/MyPhotos");
        let file = PathBuf::from("/media/MyPhotos/Vacation/photo.jpg");
        assert_eq!(
            derive_tags(&file, &root, true),
            vec!["MyPhotos", "Vacation"]
        );
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

        let db = Arc::new(Mutex::new(Database::open_in_memory().unwrap()));
        let result = run_scan(root, db.clone(), vec![], false).await.unwrap();

        assert_eq!(result.files_found, 2);
        assert_eq!(result.files_upserted, 2);
        assert_eq!(result.files_removed, 0);
        assert_eq!(result.failed_paths.len(), 0);

        let db = db.lock().unwrap();
        let tags = db.get_all_tags_with_counts().unwrap();
        let names: Vec<&str> = tags.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"Travel"));
        assert!(names.contains(&"Japan"));
    }

    #[tokio::test]
    async fn rescan_detects_removed_files() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().to_path_buf();

        fs::create_dir_all(root.join("Photos")).unwrap();
        fs::write(root.join("Photos/a.jpg"), b"a").unwrap();
        fs::write(root.join("Photos/b.jpg"), b"b").unwrap();

        let db = Arc::new(Mutex::new(Database::open_in_memory().unwrap()));

        let r1 = run_scan(root.clone(), db.clone(), vec![], false)
            .await
            .unwrap();
        assert_eq!(r1.files_found, 2);
        assert_eq!(r1.files_removed, 0);

        // Remove one file from disk.
        fs::remove_file(root.join("Photos/b.jpg")).unwrap();

        let r2 = run_scan(root, db, vec![], false).await.unwrap();
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

        let db = Arc::new(Mutex::new(Database::open_in_memory().unwrap()));
        let result = run_scan(root, db, vec![], false).await.unwrap();

        assert_eq!(result.files_found, 1);
    }

    #[tokio::test]
    async fn scan_respects_global_ignore_patterns() {
        let dir = TempDir::new().unwrap();
        let root = dir.path().to_path_buf();

        fs::write(root.join("keep.jpg"), b"keep").unwrap();
        fs::write(root.join("drop.tmp"), b"drop").unwrap();

        let db = Arc::new(Mutex::new(Database::open_in_memory().unwrap()));
        let patterns = vec!["*.tmp".into()];
        let result = run_scan(root, db, patterns, false).await.unwrap();

        // *.tmp is not a media extension anyway, but the ignore rule fires before classification.
        assert_eq!(result.files_found, 1);
    }
}
