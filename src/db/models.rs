//! Data types for database rows and write inputs.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::events::MediaType;

// ── Row types (read from DB) ────────────────────────────────────────

/// A source root directory as stored in the database.
#[derive(Debug, Clone)]
pub struct SourceRoot {
    pub id: i64,
    pub path: String,
    pub display_path: String,
    pub is_available: bool,
}

/// A media file as stored in the database.
// Row is separate from Entry because reads and writes have different fields (e.g. Row has an ID and DB-generated properties) and lifetimes.
#[derive(Debug, Clone)]
pub struct MediaRow {
    pub id: i64,
    pub path: String,
    pub filename: String,
    pub source_root_id: i64,
    pub media_type: MediaType,
    pub size_bytes: i64,
    // Linux filesystems don't always expose birth time, so created_at is optional while modified_at (mtime) is always guaranteed.
    pub created_at: Option<i64>,
    pub modified_at: i64,
    pub date_added: i64,
    pub thumbnail_path: Option<String>,
    pub duration_secs: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct MediaItem {
    pub id: i64,
    pub path: String,
    pub filename: String,
    pub source_root_id: i64,
    pub media_type: MediaType,
    pub size_bytes: i64,
    pub created_at: Option<i64>,
    pub modified_at: i64,
    pub date_added: i64,
    pub thumbnail_path: Option<String>,
    pub duration_secs: Option<i64>,
}

impl From<MediaRow> for MediaItem {
    fn from(row: MediaRow) -> Self {
        Self {
            id: row.id,
            path: row.path,
            filename: row.filename,
            source_root_id: row.source_root_id,
            media_type: row.media_type,
            size_bytes: row.size_bytes,
            created_at: row.created_at,
            modified_at: row.modified_at,
            date_added: row.date_added,
            thumbnail_path: row.thumbnail_path,
            duration_secs: row.duration_secs,
        }
    }
}

/// Thumbnail cache status for a media row (T-1).
///
/// Lets a caller decide whether to show the current thumbnail, a kept-old
/// (`stale`) thumbnail, or a failure placeholder. Placeholder rendering itself
/// is a U/V concern; this type only makes the status queryable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThumbnailStatus {
    /// Stable cache key addressing the current thumbnail file, if generated.
    pub cache_key: Option<String>,
    /// Path to the current (possibly stale) thumbnail file, if any.
    pub thumbnail_path: Option<String>,
    /// The source file changed since the thumbnail was generated; the old
    /// thumbnail is kept until an explicit regeneration succeeds.
    pub stale: bool,
    /// Last generation-failure reason, if the most recent attempt failed.
    pub failure: Option<String>,
}

/// The source fields needed to (re)generate a thumbnail and compute its stable
/// cache key (T-1).
#[derive(Debug, Clone)]
pub struct ThumbnailSource {
    pub media_id: i64,
    pub path: String,
    /// Canonical identity — the stable addressing basis for the cache key.
    pub canonical_identity: String,
    pub media_type: MediaType,
    pub size_bytes: i64,
    pub modified_at: i64,
}

/// Database manifest row used by thumbnail disk-cache LRU maintenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThumbnailCacheEntry {
    pub media_id: i64,
    pub cache_key: String,
    pub thumbnail_path: String,
    pub last_accessed_at: Option<i64>,
}

/// A folder-derived tag with its associated file count, sorted by count descending.
///
/// Tag identity is `(source_root_id, relative_folder_path)`: two folders with the
/// same basename in different roots or subtrees are distinct tags. `display_name`
/// is the folder basename; `display_path` is the lineage (root name + relative
/// path) used for disambiguation.
#[derive(Debug, Clone)]
pub struct TagWithCount {
    pub id: i64,
    pub source_root_id: i64,
    pub relative_folder_path: String,
    pub display_name: String,
    pub display_path: String,
    pub file_count: i64,
}

// ── Input types (write to DB) ───────────────────────────────────────

/// Identity of a folder-derived tag, written during scanning.
///
/// Uniquely keyed by `(source_root_id, relative_folder_path)`. `relative_folder_path`
/// is empty for the source root itself (root-as-tag). `display_name` is the folder
/// basename; `display_path` is the lineage kept for later disambiguation.
#[derive(Debug, Clone)]
pub struct TagIdentity {
    pub source_root_id: i64,
    pub relative_folder_path: String,
    pub display_name: String,
    pub display_path: String,
}

/// Data needed to insert or update a media entry.
#[derive(Debug, Clone)]
pub struct MediaEntry {
    pub path: String,
    /// Path relative to the owning source root (`source_root_id`). Together with
    /// `source_root_id` this forms the row's per-root identity (02 §4).
    pub relative_path: String,
    /// Canonical target path string: a regular file's own canonical path, or a
    /// file symlink's resolved target. Unique across the library (02 §4); the
    /// symlink-boundary and canonical-dedup reconciliation is deferred to I-2.
    pub canonical_identity: String,
    pub filename: String,
    pub source_root_id: i64,
    pub media_type: MediaType,
    pub size_bytes: i64,
    pub created_at: Option<i64>,
    pub modified_at: i64,
}

/// A scan failure to persist in the `scan_errors` table.
///
/// Identity is `(source_root_id, scan_generation, path)`. `last_seen` is set by
/// the database at write time, so it is not carried here.
#[derive(Debug, Clone)]
pub struct ScanErrorEntry {
    pub source_root_id: i64,
    pub scan_generation: i64,
    pub path: String,
    pub category: String,
    pub message: String,
}

// ── Timestamp conversion utilities ──────────────────────────────────

/// Converts a `SystemTime` to UTC Unix epoch milliseconds, the unit every
/// persisted schema timestamp uses (02 §4).
pub fn system_time_to_epoch_millis(time: SystemTime) -> i64 {
    // Epoch 0 is used as a safe sentinel on error because a missing or corrupt timestamp shouldn't abort an entire indexing run.
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}
