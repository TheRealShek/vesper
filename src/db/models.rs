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
    pub added_at: i64,
    pub is_available: bool,
}

/// A media file as stored in the database.
#[derive(Debug, Clone)]
pub struct MediaRow {
    pub id: i64,
    pub path: String,
    pub filename: String,
    pub source_root_id: i64,
    pub media_type: MediaType,
    pub size_bytes: i64,
    pub created_at: Option<i64>,
    pub modified_at: i64,
    pub thumbnail_path: Option<String>,
    pub duration_secs: Option<i64>,
    pub indexed_at: i64,
}

/// A tag with its associated file count, sorted by count descending.
#[derive(Debug, Clone)]
pub struct TagWithCount {
    pub id: i64,
    pub name: String,
    pub file_count: i64,
}

// ── Input types (write to DB) ───────────────────────────────────────

/// Data needed to insert or update a media entry.
#[derive(Debug, Clone)]
pub struct MediaEntry {
    pub path: String,
    pub filename: String,
    pub source_root_id: i64,
    pub media_type: MediaType,
    pub size_bytes: i64,
    pub created_at: Option<i64>,
    pub modified_at: i64,
    pub indexed_at: i64,
}

// ── Timestamp conversion utilities ──────────────────────────────────

/// Converts a `SystemTime` to Unix epoch seconds.
pub fn system_time_to_epoch(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
