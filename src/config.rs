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
