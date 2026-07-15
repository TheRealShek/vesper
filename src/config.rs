/// Centralized configuration constants for Vesper.
pub const DB_NAME: &str = "vesper.db";

/// Single-instance library lock file, kept next to the database.
pub const LOCK_NAME: &str = "vesper.lock";

/// Debounce time for filesystem events in milliseconds.
pub const FS_DEBOUNCE_MS: u64 = 300;
