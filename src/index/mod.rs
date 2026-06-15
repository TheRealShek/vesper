//! Filesystem indexing: directory walking, ignore rules, and media classification.
//!
//! This module has zero GTK imports. It communicates with the rest of the
//! application exclusively through typed events sent via channels.

mod error;
// ignore_rules and media are public because scan.rs reuses their classifiers directly,
// while walker is private because its internals are purely an implementation detail.
pub mod ignore_rules;
pub mod media;
mod walker;

// Re-exported at the module level to flatten the API so scan.rs doesn't need to know the internal file structure.
pub use ignore_rules::build_global_rules;
pub use walker::scan_source_root;
