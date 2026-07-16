//! Structured logging setup (B-8).
//!
//! Replaces ad-hoc `eprintln!` with `tracing`, writing to a size-rotated log
//! file in the user state directory (10 MB per file, 3 files kept). Info-level
//! events never carry a full filesystem path — paths are redacted to their final
//! component via [`redact_path`]; unredacted paths are reserved for debug/trace.

use std::path::Path;

use file_rotate::{ContentLimit, FileRotate, compression::Compression, suffix::AppendCount};
use tracing::info;
use tracing_appender::non_blocking::WorkerGuard;

/// Log file base name inside the user state directory.
pub const LOG_FILE_NAME: &str = "vesper.log";
/// Rotate each log file once it reaches 10 MB (B-8).
const MAX_FILE_BYTES: usize = 10 * 1024 * 1024;
/// Keep 3 rotated log files (B-8).
const KEEP_FILES: usize = 3;

/// Initializes the global tracing subscriber, writing size-rotated logs into
/// `state_dir` (10 MB per file, 3 files kept).
///
/// Returns a [`WorkerGuard`] that must be held for the process lifetime so the
/// background writer flushes on shutdown, or `None` if a global subscriber was
/// already installed (e.g. in tests).
#[must_use]
pub fn init(state_dir: &Path) -> Option<WorkerGuard> {
    let log_path = state_dir.join(LOG_FILE_NAME);
    let appender = FileRotate::new(
        log_path,
        AppendCount::new(KEEP_FILES),
        ContentLimit::Bytes(MAX_FILE_BYTES),
        Compression::None,
        #[cfg(unix)]
        None,
    );
    let (writer, guard) = tracing_appender::non_blocking(appender);

    // Default to INFO; RUST_LOG can raise verbosity (e.g. to surface the full
    // paths that are only emitted at debug/trace).
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    let subscriber = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_env_filter(filter)
        .finish();

    if tracing::subscriber::set_global_default(subscriber).is_err() {
        return None;
    }
    Some(guard)
}

/// Renders a path for info-level logs without leaking its full filesystem
/// location (B-8): only the final component is kept. Unredacted paths are
/// reserved for debug/trace.
pub fn redact_path(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// Logs a completed scan (B-8). The root is redacted to its last component so no
/// full absolute path appears at info level.
pub fn scan_completed(root: &Path, files_found: u64, files_upserted: u64, files_removed: u64) {
    info!(
        root = %redact_path(root),
        files_found,
        files_upserted,
        files_removed,
        "scan completed"
    );
}

/// Logs a source root's availability transition (B-8), with the path redacted.
pub fn root_availability_changed(root: &Path, online: bool) {
    info!(
        root = %redact_path(root),
        online,
        "source root availability changed"
    );
}

/// Logs a successfully applied schema migration (B-8).
pub fn migration_applied(version: i64, name: &str) {
    info!(version, name, "schema migration applied");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::fmt::MakeWriter;

    /// Captures formatted log output into a shared buffer for assertions.
    #[derive(Clone)]
    struct BufWriter(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for BufWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for BufWriter {
        type Writer = BufWriter;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    #[test]
    fn info_scan_and_migration_events_omit_full_absolute_paths() {
        let buf = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::fmt()
            .with_writer(BufWriter(buf.clone()))
            .with_ansi(false)
            .finish();

        // Emit the exact info-level events the backend emits, through the same
        // helpers, under a scoped capturing subscriber.
        tracing::subscriber::with_default(subscriber, || {
            let root = Path::new("/home/alice/Private/Photos/Vacation 2023");
            scan_completed(root, 12, 12, 0);
            root_availability_changed(Path::new("/mnt/usb/DCIM"), false);
            migration_applied(4, "add_media_identity");
        });

        let logged = String::from_utf8(buf.lock().unwrap().clone()).unwrap();

        // The final component survives for context...
        assert!(
            logged.contains("Vacation 2023"),
            "basename is kept for context: {logged}"
        );
        assert!(logged.contains("DCIM"), "basename is kept: {logged}");
        assert!(logged.contains("schema migration applied"));

        // ...but no full absolute path (or its parent segments) leaks at info.
        assert!(
            !logged.contains("/home/alice/Private/Photos/Vacation 2023"),
            "no full absolute path: {logged}"
        );
        assert!(!logged.contains("/mnt/usb/DCIM"), "no full path: {logged}");
        assert!(!logged.contains("/home/alice"), "no parent path: {logged}");
        assert!(!logged.contains("/mnt/usb"), "no parent path: {logged}");
        assert!(!logged.contains("Private"), "no parent segment: {logged}");
    }
}
