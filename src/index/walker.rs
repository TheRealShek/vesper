//! Recursive filesystem walker with `.galleryignore` support and symlink handling.
//!
//! The walker scans a source root directory, discovering media files while
//! respecting ignore rules. Directory symlinks are not followed in v1 (spec
//! section 4). Designed to run inside `tokio::task::spawn_blocking`.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use ignore::gitignore::Gitignore;
use tokio::sync::mpsc;

use crate::events::{DiscoveredMedia, ScanEvent};

use super::error::IndexError;
use super::ignore_rules;
use super::media;

/// Outcome of a completed walk (I-6).
///
/// `partial` is set when any directory could not be read during the walk (a
/// subtree was hidden, e.g. a permissions error or the root going offline
/// mid-scan). A partial walk must not drive the stale-media deletion sweep,
/// since undiscovered files were merely unreachable, not deleted.
pub struct WalkSummary {
    pub files_found: u64,
    pub partial: bool,
}

/// Scans a source root directory recursively, emitting events for discovered media.
///
/// This function performs blocking I/O and **must** be called from
/// `tokio::task::spawn_blocking`. Events are sent through the provided channel.
///
/// Individual file/directory errors are emitted as `ScanEvent::Error` and do
/// not halt the scan, but a directory read error marks the walk **partial** in
/// the returned [`WalkSummary`] (I-6). Only fatal errors (invalid root, closed
/// channel) return `Err`.
pub fn scan_source_root(
    root: &Path,
    global_rules: &Gitignore,
    initial_ignore_stack: Vec<Gitignore>,
    source_roots: &[PathBuf],
    sender: &mpsc::Sender<ScanEvent>,
) -> Result<WalkSummary, IndexError> {
    let root = root.to_path_buf();

    if !root.is_dir() {
        return Err(IndexError::NotADirectory { path: root });
    }

    send_event(sender, ScanEvent::Started { root: root.clone() })?;

    let mut total_found: u64 = 0;
    // Set if any directory read fails mid-walk, marking the walk partial (I-6).
    let mut had_read_error = false;
    let mut ignore_stack = initial_ignore_stack;
    let mut visited_paths = HashSet::new();
    visited_paths.insert(root.clone());
    // Canonical identities already emitted in this walk (I-2). A file symlink
    // and its target — or two symlinks to one target — resolve to the same
    // canonical path; we index the first and skip the rest so a symlink and its
    // target never both get a row (02 §4 unique canonical_identity, §5 "skip the
    // duplicate path").
    let mut seen_canonical = HashSet::new();

    let mut ctx = WalkContext {
        _root: &root,
        global_rules,
        source_roots,
        sender,
        total_found: &mut total_found,
        had_read_error: &mut had_read_error,
        visited_paths: &mut visited_paths,
        seen_canonical: &mut seen_canonical,
        ignore_stack: &mut ignore_stack,
    };
    walk_directory(&mut ctx, &root)?;

    send_event(sender, ScanEvent::Completed { root, total_found })?;

    Ok(WalkSummary {
        files_found: total_found,
        partial: had_read_error,
    })
}

struct WalkContext<'a> {
    _root: &'a Path,
    global_rules: &'a Gitignore,
    /// Canonical paths of every source root, used to enforce the file-symlink
    /// boundary rule (I-2): a symlink target outside all roots is skipped.
    source_roots: &'a [PathBuf],
    sender: &'a mpsc::Sender<ScanEvent>,
    total_found: &'a mut u64,
    /// Set when a directory could not be read, marking the walk partial (I-6).
    had_read_error: &'a mut bool,
    visited_paths: &'a mut HashSet<PathBuf>,
    /// Canonical identities already emitted, for symlink/target dedup (I-2).
    seen_canonical: &'a mut HashSet<PathBuf>,
    ignore_stack: &'a mut Vec<Gitignore>,
}

/// Recursively walks a single directory level.
fn walk_directory(ctx: &mut WalkContext<'_>, dir: &Path) -> Result<(), IndexError> {
    // Load .galleryignore for this directory if present.
    let local_rules = match ignore_rules::load_directory_rules(dir) {
        Ok(rules) => rules,
        Err(e) => {
            send_event(
                ctx.sender,
                ScanEvent::Error {
                    path: dir.to_path_buf(),
                    message: format!("Failed to parse .galleryignore: {e}"),
                },
            )?;
            None
        }
    };

    let pushed_local_rules = local_rules.is_some();
    if let Some(rules) = local_rules {
        ctx.ignore_stack.push(rules);
    }

    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(source) => {
            if dir == ctx._root {
                return Err(IndexError::ReadDir {
                    path: dir.to_path_buf(),
                    source,
                });
            }
            // A subtree became unreadable: this walk is now partial, so the
            // caller must skip the stale-media sweep (I-6). Undiscovered files
            // here were unreachable, not deleted.
            *ctx.had_read_error = true;
            send_event(
                ctx.sender,
                ScanEvent::Error {
                    path: dir.to_path_buf(),
                    message: format!("Failed to read directory: {source}"),
                },
            )?;
            if pushed_local_rules {
                ctx.ignore_stack.pop();
            }
            return Ok(());
        }
    };

    for entry_result in entries {
        let entry = match entry_result {
            Ok(e) => e,
            Err(source) => {
                // A failure while iterating an open ReadDir leaves an unknown
                // remainder of this directory unvisited: the walk is partial
                // (I-6), so the caller must skip the stale-media sweep.
                *ctx.had_read_error = true;
                send_event(
                    ctx.sender,
                    ScanEvent::Error {
                        path: dir.to_path_buf(),
                        message: format!("Failed to read directory entry: {source}"),
                    },
                )?;
                continue;
            }
        };

        let path = entry.path();

        // .galleryignore files are never shown in the media grid (spec section 5).
        // Skipped early to avoid unnecessary regex evaluation on the ignore file itself.
        if entry.file_name() == ".galleryignore" {
            continue;
        }

        // file_type() reads dirent data without an extra stat() syscall, speeding up symlink detection.
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(source) => {
                // The entry was discovered but could not be classified; if it
                // is a directory its whole subtree goes unvisited, so the walk
                // is partial (I-6).
                *ctx.had_read_error = true;
                send_event(
                    ctx.sender,
                    ScanEvent::Error {
                        path: path.clone(),
                        message: format!("Failed to read file type: {source}"),
                    },
                )?;
                continue;
            }
        };

        let is_symlink = file_type.is_symlink();

        // For symlinks, resolve to the actual target metadata.
        // Broken or circular symlinks are silently skipped (spec section 4).
        let resolved_metadata = if is_symlink {
            match fs::metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            }
        } else {
            match entry.metadata() {
                Ok(m) => m,
                Err(source) => {
                    send_event(
                        ctx.sender,
                        ScanEvent::Error {
                            path: path.clone(),
                            message: format!("Failed to read metadata: {source}"),
                        },
                    )?;
                    continue;
                }
            }
        };

        let is_dir = resolved_metadata.is_dir();

        if is_dir {
            // Directory symlinks are not followed in v1 (spec section 4 / 01 §4).
            if is_symlink {
                continue;
            }

            if ignore_rules::is_ignored(&path, true, ctx.ignore_stack, ctx.global_rules) {
                continue;
            }

            // canonicalize resolves all symlinks, ensuring cycle detection works even if paths look syntactically different.
            let canonical_path = match path.canonicalize() {
                Ok(p) => p,
                Err(_) => continue,
            };
            if !ctx.visited_paths.insert(canonical_path) {
                continue;
            }

            walk_directory(ctx, &path)?;
        } else if resolved_metadata.is_file() {
            // Hardcoded scanner-level filter (B-3): in-progress downloads and
            // editor backups never produce a record or an error, independent of
            // the user's ignore rules and ahead of classification.
            if media::is_temp_file(&path) {
                continue;
            }

            if ignore_rules::is_ignored(&path, false, ctx.ignore_stack, ctx.global_rules) {
                continue;
            }

            let Some(media_type) = media::classify(&path) else {
                continue;
            };

            // Canonical identity for boundary + duplicate checks (I-2). A regular
            // file under a canonical root is already canonical; a file symlink
            // resolves to its target, which may lie elsewhere.
            let canonical = if is_symlink {
                match path.canonicalize() {
                    Ok(target) => {
                        // File symlinks may only be indexed when the target lands
                        // inside some source-root boundary (02 §1); a target
                        // outside all roots is skipped.
                        if !ctx.source_roots.iter().any(|r| target.starts_with(r)) {
                            continue;
                        }
                        target
                    }
                    // Broken/circular symlink: nothing to index.
                    Err(_) => continue,
                }
            } else {
                path.clone()
            };

            // Skip the duplicate path when a symlink and its target — or two
            // symlinks to one target — share a canonical identity (I-2, 02 §5):
            // index the first seen, skip the rest so only one row results.
            if !ctx.seen_canonical.insert(canonical) {
                continue;
            }

            let discovered = DiscoveredMedia {
                path,
                media_type,
                size_bytes: resolved_metadata.len(),
                modified: resolved_metadata
                    .modified()
                    .unwrap_or(SystemTime::UNIX_EPOCH),
                created: resolved_metadata.created().ok(),
            };

            send_event(ctx.sender, ScanEvent::FileFound(discovered))?;
            *ctx.total_found += 1;
        }
    }

    if pushed_local_rules {
        ctx.ignore_stack.pop();
    }

    Ok(())
}

/// Sends a scan event, converting channel-closed errors to `IndexError`.
fn send_event(sender: &mpsc::Sender<ScanEvent>, event: ScanEvent) -> Result<(), IndexError> {
    sender
        .blocking_send(event)
        .map_err(|_| IndexError::ChannelSend)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use tempfile::TempDir;

    /// Runs the walker over `root` with the given source-root boundaries and
    /// returns the discovered file paths. The walker uses `blocking_send`, so it
    /// runs on the blocking pool while the test drains the channel.
    async fn discover(root: PathBuf, source_roots: Vec<PathBuf>) -> Vec<PathBuf> {
        let (tx, mut rx) = mpsc::channel(1024);
        let handle = tokio::task::spawn_blocking(move || {
            let rules = crate::index::build_global_rules(&[]).unwrap();
            scan_source_root(&root, &rules, Vec::new(), &source_roots, &tx)
        });
        let mut found = Vec::new();
        while let Some(event) = rx.recv().await {
            if let ScanEvent::FileFound(media) = event {
                found.push(media.path);
            }
        }
        handle.await.unwrap().unwrap();
        found
    }

    #[tokio::test]
    async fn file_symlink_pointing_outside_all_roots_is_rejected() {
        let root_dir = TempDir::new().unwrap();
        let outside_dir = TempDir::new().unwrap();
        let root = root_dir.path().canonicalize().unwrap();
        let outside = outside_dir.path().canonicalize().unwrap();

        // A real media file inside the root, plus a symlink whose target lives
        // outside every source root.
        std::fs::write(root.join("real.jpg"), b"jpg").unwrap();
        std::fs::write(outside.join("target.jpg"), b"jpg").unwrap();
        symlink(outside.join("target.jpg"), root.join("outside_link.jpg")).unwrap();

        let found = discover(root.clone(), vec![root.clone()]).await;

        assert_eq!(
            found.len(),
            1,
            "only the in-root file is indexed: {found:?}"
        );
        assert!(found[0].ends_with("real.jpg"));
        assert!(
            !found.iter().any(|p| p.ends_with("outside_link.jpg")),
            "a symlink resolving outside all roots must be skipped"
        );
    }

    #[tokio::test]
    async fn file_symlink_and_its_target_yield_a_single_row() {
        let root_dir = TempDir::new().unwrap();
        let root = root_dir.path().canonicalize().unwrap();

        // A real file and a symlink to it, both inside the root.
        std::fs::write(root.join("photo.jpg"), b"jpg").unwrap();
        symlink(root.join("photo.jpg"), root.join("alias.jpg")).unwrap();

        let found = discover(root.clone(), vec![root.clone()]).await;

        assert_eq!(
            found.len(),
            1,
            "a symlink and its target must not both be indexed: {found:?}"
        );
        // Whichever path won, it resolves to the shared canonical target.
        assert_eq!(found[0].canonicalize().unwrap(), root.join("photo.jpg"));
    }
}
