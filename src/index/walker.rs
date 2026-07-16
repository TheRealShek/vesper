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

/// Scans a source root directory recursively, emitting events for discovered media.
///
/// This function performs blocking I/O and **must** be called from
/// `tokio::task::spawn_blocking`. Events are sent through the provided channel.
///
/// Individual file/directory errors are emitted as `ScanEvent::Error` and do
/// not halt the scan. Only fatal errors (invalid root, closed channel) return `Err`.
pub fn scan_source_root(
    root: &Path,
    global_rules: &Gitignore,
    initial_ignore_stack: Vec<Gitignore>,
    sender: &mpsc::Sender<ScanEvent>,
) -> Result<u64, IndexError> {
    let root = root.to_path_buf();

    if !root.is_dir() {
        return Err(IndexError::NotADirectory { path: root });
    }

    send_event(sender, ScanEvent::Started { root: root.clone() })?;

    let mut total_found: u64 = 0;
    let mut ignore_stack = initial_ignore_stack;
    let mut visited_paths = HashSet::new();
    visited_paths.insert(root.clone());

    let mut ctx = WalkContext {
        _root: &root,
        global_rules,
        sender,
        total_found: &mut total_found,
        visited_paths: &mut visited_paths,
        ignore_stack: &mut ignore_stack,
    };
    walk_directory(&mut ctx, &root)?;

    send_event(sender, ScanEvent::Completed { root, total_found })?;

    Ok(total_found)
}

struct WalkContext<'a> {
    _root: &'a Path,
    global_rules: &'a Gitignore,
    sender: &'a mpsc::Sender<ScanEvent>,
    total_found: &'a mut u64,
    visited_paths: &'a mut HashSet<PathBuf>,
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

            if let Some(media_type) = media::classify(&path) {
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
