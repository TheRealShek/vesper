//! Filesystem indexing: directory walking, ignore rules, and media classification.
//!
//! This module has zero GTK imports. It communicates with the rest of the
//! application exclusively through typed events sent via channels.

mod error;
pub mod ignore_rules;
mod media;
mod walker;

pub use ignore_rules::build_global_rules;
pub use walker::scan_source_root;
