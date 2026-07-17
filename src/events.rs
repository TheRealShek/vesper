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
    // File modification time from the filesystem.
    pub modified: SystemTime,
    // File creation time (may be unavailable on some Linux filesystems).
    // Stored separately because modified is always guaranteed, but created is not.
    pub created: Option<SystemTime>,
}

/// Events emitted by the filesystem scanner during indexing.
#[derive(Debug)]
pub enum ScanEvent {
    /// Scan of a source root has started.
    #[allow(dead_code)]
    Started { root: PathBuf },
    /// A media file was discovered during the scan.
    FileFound(DiscoveredMedia),
    /// A non-fatal error occurred while scanning a specific path.
    #[allow(dead_code)]
    Error { path: PathBuf, message: String },
    /// Scan of a source root completed successfully.
    #[allow(dead_code)]
    Completed { root: PathBuf, total_found: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagMode {
    Any,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    DateModifiedDesc,
    DateModifiedAsc,
    DateAddedDesc,
    DateAddedAsc,
    FilenameAsc,
    FilenameDesc,
    FileSizeDesc,
    FileSizeAsc,
}

#[derive(Debug, Clone)]
pub struct MediaQuery {
    pub tags: Vec<crate::state::TagFilter>,
    pub tag_mode: TagMode,
    pub search: Option<String>,
    pub sort: SortOrder,
}

// Separating AppEvent (frontend -> backend) and UiEvent (backend -> frontend)
// keeps coupling one-way and clarifies event flow direction.
/// Events emitted by a frontend to trigger backend operations.
#[derive(Debug)]
pub enum AppEvent {
    /// Request to add a source root path.
    AddSourceRoot(String),
    /// Request to remove a source root by ID.
    RemoveSourceRoot(i64),
    /// Request to update backend configuration settings
    UpdateSettings(crate::state::BackendState),
    /// Request to rescan all source roots.
    RescanRoots,
    /// Regenerate thumbnails whose source changed or whose prior generation failed.
    RegenerateThumbnails,
    /// Recreate derived index records, preserve roots/settings, and fully rescan.
    RebuildLibraryIndex,
    /// Request to fetch data (tags, media, roots) for a frontend.
    FetchData,
    /// Request to rescan a subtree (e.g., due to .galleryignore changes).
    RescanSubtree(std::path::PathBuf),
    /// A single file was created, modified, or deleted.
    FileChanged(std::path::PathBuf, ChangeKind),
    /// Query media items with filter, sort, and pagination. The `u64` is the
    /// query generation (B-2); it is echoed back in [`UiEvent::QueryResult`]
    /// so a frontend can discard results from a superseded query.
    QueryMedia(MediaQuery, u64),
    /// A virtualized grid cell began or stopped referencing this thumbnail.
    ThumbnailVisibility { media_id: i64, visible: bool },
    /// A visible cell read a thumbnail. Decoding and access accounting happen
    /// off the frontend thread; the decoded pixels return in a typed event.
    ReadThumbnail { media_id: i64, path: String },
    /// Request the persisted scan-error path list (NEW-4). The query runs on
    /// the database worker and returns via
    /// [`UiEvent::ScanErrorPaths`]; frontend code never touches SQLite directly.
    FetchScanErrors,
}

/// Events published by the backend for a future frontend or another consumer.
///
/// This contract is deliberately core-owned: the backend must be runnable
/// without linking a particular UI implementation.
pub enum UiEvent {
    ThumbnailReady(i64, String, Option<i64>),
    ThumbnailDecoded(DecodedThumbnail),
    ThumbnailsEvicted(Vec<i64>),
    ScanCompleted(usize, Vec<String>),
    BackendWarning(String),
    RecoverableCritical(Option<String>),
    SettingsError(String),
    ScanStarted,
    ScanProgress(usize),
    DataFetched {
        tags: Vec<UiTag>,
        media: Vec<UiMediaItem>,
        roots: Vec<UiSourceRoot>,
        has_roots: bool,
        generation: u64,
    },
    MediaChunk {
        generation: u64,
        items: Vec<UiMediaItem>,
    },
    RootsOffline(usize),
    #[allow(dead_code)]
    FatalError(String),
    MediaAdded(UiMediaItem),
    MediaRemoved(String),
    TagsUpdated(Vec<UiTag>),
    QueryResult(Vec<UiMediaItem>, u32, u64),
    QueryChunk {
        generation: u64,
        items: Vec<UiMediaItem>,
    },
    ScanErrorPaths(Vec<String>),
}

/// A decoded RGBA thumbnail prepared off the GTK thread.
#[derive(Debug)]
pub struct DecodedThumbnail {
    pub media_id: i64,
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// The type of filesystem change for a media file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Modified,
    Deleted,
}

/// A tag summary sent to the UI.
#[derive(Debug, Clone)]
pub struct UiSourceRoot {
    pub id: i64,
    pub name: String,
    pub path: String,
    #[allow(dead_code)]
    pub display_path: String,
    pub is_available: bool,
}

/// A tag summary sent to the UI, carrying full path-qualified identity (A-2).
#[derive(Debug, Clone)]
pub struct UiTag {
    pub id: i64,
    pub source_root_id: i64,
    pub relative_folder_path: String,
    pub display_name: String,
    pub display_path: String,
    pub file_count: i64,
}

/// A fully prepared media item ready for the UI layer.
#[derive(Debug, Clone)]
pub struct UiMediaItem {
    pub id: i64,
    pub path: String,
    pub filename: String,
    pub tags: String,
    pub thumbnail_path: String,
    pub duration_secs: i64,
    pub media_type: MediaType,
    pub size_bytes: i64,
    pub created_at: Option<i64>,
    pub date_added: i64,
    pub modified_at: i64,
    // Derived at fetch time because offline state is root-level in the DB, not per-file.
    pub is_offline: bool,
}

pub trait ChannelSendExt<T> {
    fn send_log(&self, msg: T);
    fn send_critical(&self, msg: T);
}

impl<T: Send + 'static> ChannelSendExt<T> for tokio::sync::mpsc::Sender<T> {
    fn send_log(&self, msg: T) {
        // try_send is non-blocking; dropping events on a full channel is safer than deadlocking the UI or watcher threads.
        if let Err(e) = self.try_send(msg) {
            eprintln!("Channel send failed (event dropped): {}", e);
        }
    }

    fn send_critical(&self, msg: T) {
        let tx = self.clone();
        // Spawns a Tokio task to enqueue the event asynchronously.
        // WHY: Many callsites are synchronous GTK UI callbacks that cannot .await or block.
        // Spawning ensures critical events are never dropped on full channels (unlike try_send)
        // while safely offloading backpressure to the Tokio runtime instead of freezing the UI thread.
        tokio::spawn(async move {
            if let Err(e) = tx.send(msg).await {
                eprintln!("Critical channel send failed: {}", e);
            }
        });
    }
}
