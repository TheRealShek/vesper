//! Error types for the indexing module.

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during filesystem indexing.
#[derive(Debug, Error)]
pub enum IndexError {
    #[error("failed to read directory '{}'", path.display())]
    #[allow(dead_code)]
    ReadDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to read metadata for '{}'", path.display())]
    #[allow(dead_code)]
    Metadata {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to read symlink target for '{}'", path.display())]
    #[allow(dead_code)]
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
    // Has no fields because a closed channel is always fatal to the walk; no specific path context is needed when the entire scan aborts.
    ChannelSend,

    // Separated from NotFound to provide distinct user-facing messages: NotADirectory implies the path exists but is the wrong type.
    #[error("source root '{}' is not a directory", path.display())]
    NotADirectory { path: PathBuf },

    #[error("source root '{}' does not exist", path.display())]
    #[allow(dead_code)]
    NotFound { path: PathBuf },
}
