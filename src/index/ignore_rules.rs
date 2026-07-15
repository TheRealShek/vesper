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
    // "/" is used as root so global rules like "*.tmp" match at any filesystem depth.
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

    // Collect ALL matching rules tagged with their scope/depth.
    // depth 0 = global rules
    // depth 1 = outermost local
    // depth N = innermost local
    let mut matches = Vec::new();

    // Global rules get depth 0 (lowest priority) so local .galleryignore files can override them.
    match global_rules.matched(path, is_dir) {
        Match::Ignore(_) => matches.push((0, true)),
        Match::Whitelist(_) => matches.push((0, false)),
        Match::None => {}
    }

    for (i, rules) in dir_rules_stack.iter().enumerate() {
        let depth = i + 1;
        match rules.matched(path, is_dir) {
            Match::Ignore(_) => matches.push((depth, true)),
            Match::Whitelist(_) => matches.push((depth, false)),
            Match::None => {}
        }
    }

    if matches.is_empty() {
        return false;
    }

    // The most specific match wins: innermost local beats outer, local beats global.
    // This maps exactly to picking the match with the highest depth score.
    // Within the same scope, last pattern listed wins is already handled by `Gitignore::matched`.
    // Depth scoring is required because a first-match approach would incorrectly prioritize outer rules if evaluated top-down.
    let winning_match = matches.into_iter().max_by_key(|(depth, _)| *depth).unwrap();
    winning_match.1
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_ignore_rules_precedence() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let global = build_global_rules(&[
            "*.tmp".to_string(),
            "global_ignore.txt".to_string(),
            "!global_unignore.txt".to_string(),
        ])
        .unwrap();

        // Root local rules
        fs::write(
            root.join(".galleryignore"),
            "
# This is a comment
*.log
!/unignored.log
!/child/unignored_by_parent.log
dir_pruned/
",
        )
        .unwrap();
        let root_rules = load_directory_rules(root).unwrap().unwrap();

        // Child local rules
        let child = root.join("child");
        fs::create_dir(&child).unwrap();
        fs::write(
            child.join(".galleryignore"),
            "!*.log\n*.tmp\n\n# another comment",
        )
        .unwrap();
        let child_rules = load_directory_rules(&child).unwrap().unwrap();

        let stack_root = vec![root_rules];
        let stack_child = vec![stack_root[0].clone(), child_rules];

        // Test: Nested .galleryignore where child negates a parent ignore rule
        // Parent ignores *.log. Child negates !*.log.
        assert!(!is_ignored(
            &child.join("test.log"),
            false,
            &stack_child,
            &global
        ));

        // Test: Global rule ignored by a local negation
        fs::write(child.join(".galleryignore"), "!*.tmp\n").unwrap();
        let child_rules2 = load_directory_rules(&child).unwrap().unwrap();
        let stack_child2 = vec![stack_root[0].clone(), child_rules2];
        assert!(!is_ignored(
            &child.join("test.tmp"),
            false,
            &stack_child2,
            &global
        ));

        // Test: Directory pruning
        assert!(is_ignored(
            &root.join("dir_pruned"),
            true,
            &stack_root,
            &global
        ));

        // Test: Same pattern in both global and local (local wins)
        let global2 = build_global_rules(&["*.txt".to_string()]).unwrap();
        fs::write(root.join(".galleryignore"), "!*.txt\n").unwrap();
        let root_rules2 = load_directory_rules(root).unwrap().unwrap();
        assert!(!is_ignored(
            &root.join("test.txt"),
            false,
            &[root_rules2],
            &global2
        ));

        // Test: Blank lines and comments ignored correctly (handled by ignore crate, proven by successful parses above)
    }
}
