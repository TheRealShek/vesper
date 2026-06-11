//! `.galleryignore` evaluation using gitignore-compatible syntax.
//!
//! Supports both global ignore rules (from application settings) and
//! per-directory `.galleryignore` files. Precedence per spec section 5:
//! 1. Per-directory rules evaluated innermost-first.
//! 2. Global rules evaluated last.
//! 3. Negation (`!`) at any level can un-ignore.
//! 4. Most specific matching rule wins.

use std::path::Path;

use ignore::gitignore::{Gitignore, GitignoreBuilder};

use super::error::IndexError;

/// Builds a `Gitignore` matcher from a list of global pattern strings.
///
/// Global rules apply to all source roots. The matcher uses root `/`
/// so basename patterns (e.g. `*.tmp`) match at any depth.
pub fn build_global_rules(patterns: &[String]) -> Result<Gitignore, IndexError> {
    let mut builder = GitignoreBuilder::new("/");
    for pattern in patterns {
        builder
            .add_line(None, pattern)
            .map_err(|source| IndexError::IgnoreParse {
                path: "global rules".into(),
                source,
            })?;
    }
    builder.build().map_err(|source| IndexError::IgnoreParse {
        path: "global rules".into(),
        source,
    })
}

/// Loads a `.galleryignore` file from the given directory, if one exists.
///
/// Returns `Ok(None)` if no `.galleryignore` file is present.
pub fn load_directory_rules(dir: &Path) -> Result<Option<Gitignore>, IndexError> {
    let ignore_path = dir.join(".galleryignore");
    if !ignore_path.exists() {
        return Ok(None);
    }

    let mut builder = GitignoreBuilder::new(dir);
    if let Some(err) = builder.add(&ignore_path) {
        return Err(IndexError::IgnoreParse {
            path: ignore_path,
            source: err,
        });
    }
    let rules = builder.build().map_err(|source| IndexError::IgnoreParse {
        path: ignore_path,
        source,
    })?;
    Ok(Some(rules))
}

/// Checks whether a path should be ignored based on stacked ignore rules.
///
/// Evaluates per-directory rules from innermost (last in stack) to outermost,
/// then global rules. The first definitive match wins.
pub fn is_ignored(
    path: &Path,
    is_dir: bool,
    dir_rules_stack: &[Gitignore],
    global_rules: &Gitignore,
) -> bool {
    use ignore::Match;

    // Per-directory rules: innermost first.
    for rules in dir_rules_stack.iter().rev() {
        match rules.matched(path, is_dir) {
            Match::Ignore(_) => return true,
            Match::Whitelist(_) => return false,
            Match::None => continue,
        }
    }

    // Global rules last.
    matches!(global_rules.matched(path, is_dir), Match::Ignore(_))
}

/// Default global ignore patterns pre-populated on first launch (spec section 5).
pub const DEFAULT_GLOBAL_PATTERNS: &[&str] = &[
    ".git/",
    "node_modules/",
    ".Trash/",
    ".cache/",
    "*.tmp",
    "*.part",
    ".DS_Store",
    "Thumbs.db",
];
