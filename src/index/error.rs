//! Error types for the indexing module.

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during filesystem indexing.
#[derive(Debug, Error)]
pub enum IndexError {
    #[error("failed to read directory '{}'", path.display())]
    ReadDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to read metadata for '{}'", path.display())]
    Metadata {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to read symlink target for '{}'", path.display())]
    Symlink {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to build ignore rules from '{}'", path.display())]
    IgnoreParse {
        path: PathBuf,
        #[source]
        source: ignore::Error,
    },

    #[error("scan event channel closed")]
    ChannelSend,

    #[error("source root '{}' is not a directory", path.display())]
    NotADirectory { path: PathBuf },

    #[error("source root '{}' does not exist", path.display())]
    NotFound { path: PathBuf },
}
