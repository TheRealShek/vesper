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
    /// Non-fatal errors encountered during scanning.
    pub errors: u64,
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
                .add_source_root(&root_str)
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
        index::scan_source_root(&scan_root, &global_rules, &tx)
    });

    // Process events as they stream in.
    let mut scanned_paths: HashSet<String> = HashSet::new();
    let mut files_upserted: u64 = 0;
    let mut errors: u64 = 0;

    while let Some(event) = rx.recv().await {
        match event {
            ScanEvent::FileFound(media) => {
                match process_file(&db, &media, &root, source_root_id) {
                    Ok(path_str) => {
                        scanned_paths.insert(path_str);
                        files_upserted += 1;
                    }
                    Err(_) => {
                        errors += 1;
                    }
                }
            }
            ScanEvent::Error { .. } => {
                errors += 1;
            }
            ScanEvent::Started { .. }
            | ScanEvent::Completed { .. }
            | ScanEvent::FileRemoved { .. } => {}
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
        errors,
    })
}

/// Processes a single discovered file: upserts into DB and syncs derived tags.
/// Returns the file's path string on success.
fn process_file(
    db: &Arc<Mutex<Database>>,
    media: &DiscoveredMedia,
    source_root: &Path,
    source_root_id: i64,
) -> Result<String> {
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

    let tags = derive_tags(&media.path, source_root);

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

    // Lock, upsert, sync tags, release.
    {
        let db = db
            .lock()
            .map_err(|_| anyhow::anyhow!("database lock poisoned"))?;
        let media_id = db.upsert_media(&entry)?;
        db.sync_tags_for_media(media_id, &tags)?;
    }

    Ok(path_str)
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
fn derive_tags(file_path: &Path, source_root: &Path) -> Vec<String> {
    let relative = match file_path.strip_prefix(source_root) {
        Ok(rel) => rel,
        Err(_) => return Vec::new(),
    };

    let parent = match relative.parent() {
        Some(p) => p,
        None => return Vec::new(),
    };

    parent
        .components()
        .filter_map(|c| {
            if let Component::Normal(name) = c {
                name.to_str().map(String::from)
            } else {
                None
            }
        })
        .collect()
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
        assert_eq!(derive_tags(&file, &root), vec!["Travel", "Japan", "2023"]);
    }

    #[test]
    fn tags_empty_for_file_at_root() {
        let root = PathBuf::from("/media");
        let file = PathBuf::from("/media/photo.jpg");
        assert!(derive_tags(&file, &root).is_empty());
    }

    #[test]
    fn tags_single_level() {
        let root = PathBuf::from("/media");
        let file = PathBuf::from("/media/Vacation/photo.jpg");
        assert_eq!(derive_tags(&file, &root), vec!["Vacation"]);
    }

    #[test]
    fn tags_empty_for_unrelated_path() {
        let root = PathBuf::from("/media");
        let file = PathBuf::from("/other/photo.jpg");
        assert!(derive_tags(&file, &root).is_empty());
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
        let result = run_scan(root, db.clone(), vec![]).await.unwrap();

        assert_eq!(result.files_found, 2);
        assert_eq!(result.files_upserted, 2);
        assert_eq!(result.files_removed, 0);
        assert_eq!(result.errors, 0);

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

        let r1 = run_scan(root.clone(), db.clone(), vec![]).await.unwrap();
        assert_eq!(r1.files_found, 2);
        assert_eq!(r1.files_removed, 0);

        // Remove one file from disk.
        fs::remove_file(root.join("Photos/b.jpg")).unwrap();

        let r2 = run_scan(root, db, vec![]).await.unwrap();
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
        let result = run_scan(root, db, vec![]).await.unwrap();

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
        let result = run_scan(root, db, patterns).await.unwrap();

        // *.tmp is not a media extension anyway, but the ignore rule fires before classification.
        assert_eq!(result.files_found, 1);
    }
}
