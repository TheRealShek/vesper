//! Typed events and DTOs for cross-boundary communication.
//!
//! These types flow between `index/`, `db/`, and `ui/` via channels.
//! No module-specific imports allowed here.

use std::path::PathBuf;
use std::time::SystemTime;

/// Classification of a media file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaType {
    Image,
    Video,
}

impl MediaType {
    /// Returns the string representation used for database storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
        }
    }

    /// Parses a media type from its database string representation.
    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "image" => Some(Self::Image),
            "video" => Some(Self::Video),
            _ => None,
        }
    }
}

/// Metadata about a discovered media file.
#[derive(Debug, Clone)]
pub struct DiscoveredMedia {
    /// Absolute path to the media file.
    pub path: PathBuf,
    /// Whether this is an image or video.
    pub media_type: MediaType,
    /// File size in bytes.
    pub size_bytes: u64,
    /// File modification time from the filesystem.
    pub modified: SystemTime,
    /// File creation time (may be unavailable on some Linux filesystems).
    pub created: Option<SystemTime>,
}

/// Events emitted by the filesystem scanner during indexing.
#[derive(Debug)]
pub enum ScanEvent {
    /// Scan of a source root has started.
    Started { root: PathBuf },
    /// A media file was discovered during the scan.
    FileFound(DiscoveredMedia),
    /// A previously indexed file no longer exists on disk.
    FileRemoved { path: PathBuf },
    /// A non-fatal error occurred while scanning a specific path.
    Error { path: PathBuf, message: String },
    /// Scan of a source root completed successfully.
    Completed { root: PathBuf, total_found: u64 },
}

/// Events emitted by the UI to trigger backend operations.
#[derive(Debug)]
pub enum AppEvent {
    /// Request to add a source root path.
    AddSourceRoot(String),
    /// Request to remove a source root by ID.
    RemoveSourceRoot(i64),
    /// Request to rescan all source roots.
    RescanRoots,
    /// Request to fetch data (tags, media, roots) for the UI.
    FetchData,
}
