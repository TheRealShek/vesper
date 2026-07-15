//! Single-instance library lock (B-1).
//!
//! Vesper must never run two write-capable instances against the same library
//! state (01 §4 / 02 §5). Before opening the database for write access we take
//! an exclusive advisory lock (`flock(LOCK_EX | LOCK_NB)`) on a lock file next
//! to the database. This is an OS-level lock, not a PID file: the kernel
//! releases it automatically when the process exits or crashes, so a stale
//! lock can never block a subsequent launch.

use std::fs::OpenOptions;
use std::io;
use std::os::unix::io::AsRawFd;
use std::path::Path;

/// Holds the exclusive library lock for the lifetime of the process.
///
/// The lock is released when this value is dropped, when the underlying file
/// descriptor is closed, or when the process exits. Keep it alive for as long
/// as the database is open for write access.
pub struct LibraryLock {
    // The lock is bound to this open file description; it must stay open.
    _file: std::fs::File,
}

impl LibraryLock {
    /// Attempts to acquire the exclusive library lock at `path`.
    ///
    /// Returns `Ok(Some(lock))` when the lock was acquired, `Ok(None)` when
    /// another instance already holds it, and `Err` when the lock file could
    /// not be opened or the lock call failed for any other reason.
    pub fn acquire(path: &Path) -> io::Result<Option<Self>> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;

        // SAFETY: `file` owns the fd for the duration of this call.
        let ret = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if ret == 0 {
            return Ok(Some(Self { _file: file }));
        }

        let err = io::Error::last_os_error();
        // The lock is held by another instance; not a hard error.
        if matches!(err.raw_os_error(), Some(code) if code == libc::EWOULDBLOCK) {
            return Ok(None);
        }
        Err(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_acquire_reports_contention() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vesper.lock");

        let first = LibraryLock::acquire(&path).unwrap();
        assert!(first.is_some(), "first acquire should succeed");

        let second = LibraryLock::acquire(&path).unwrap();
        assert!(second.is_none(), "second acquire should report contention");
    }

    #[test]
    fn lock_is_released_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vesper.lock");

        let first = LibraryLock::acquire(&path).unwrap();
        assert!(first.is_some());
        drop(first);

        let again = LibraryLock::acquire(&path).unwrap();
        assert!(again.is_some(), "lock should be reacquirable after drop");
    }
}
