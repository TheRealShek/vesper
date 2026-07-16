/// Centralized configuration constants for Vesper.
pub const DB_NAME: &str = "vesper.db";

/// Single-instance library lock file, kept next to the database.
pub const LOCK_NAME: &str = "vesper.lock";

/// Debounce time for filesystem events in milliseconds.
pub const FS_DEBOUNCE_MS: u64 = 300;

/// Maximum number of media rows delivered in a single hydration chunk (B-2).
/// Hydration streams the library in windows of this size instead of one giant
/// reload, so the first grid appears without waiting for the whole store.
pub const HYDRATION_CHUNK_SIZE: i64 = 500;

/// Maximum on-disk thumbnail cache size (02 §4).
pub const THUMBNAIL_DISK_BUDGET_BYTES: u64 = 5 * 1024 * 1024 * 1024;

/// Maximum decoded thumbnail memory-cache size and entry count (02 §4).
pub const THUMBNAIL_MEMORY_BUDGET_BYTES: usize = 256 * 1024 * 1024;
pub const THUMBNAIL_MEMORY_ENTRY_LIMIT: usize = 512;

/// Minimum interval between persisted access timestamps for one thumbnail.
pub const THUMBNAIL_ACCESS_BATCH_MS: i64 = 10 * 60 * 1000;
