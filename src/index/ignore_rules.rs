//! `.galleryignore` evaluation using gitignore-compatible syntax.
//!
//! Supports both global ignore rules (from application settings) and
//! per-directory `.galleryignore` files. Precedence per spec 02 §2 (I-5):
//! global rules are evaluated first, then `.galleryignore` files from the source
//! root down to the leaf, as **one combined ordered list** with unified
//! last-matching-rule-wins evaluation — not as separate matcher objects each
//! deciding independently. Negation (`!`) at any level can un-ignore, and the
//! last matching rule across the whole list decides.

use std::path::Path;

use ignore::gitignore::{Gitignore, GitignoreBuilder};

use super::error::IndexError;

/// A single invalid ignore-rule line, collected during validation (I-5).
///
/// Invalid lines never partially apply — a rule set with any invalid line is
/// rejected as a whole — and each error carries enough context (source + 1-based
/// line number) for the Settings dialog to surface it, which consumes these
/// via [`validate_global_patterns`] on Apply (`src/ui/settings.rs`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoreValidationError {
    /// Where the invalid line came from: a `.galleryignore` path, or the
    /// `"global rules"` sentinel for settings-provided patterns.
    pub source: String,
    /// 1-based line number within `source`, when it can be attributed to a line.
    pub line: Option<usize>,
    /// Human-readable reason the line is invalid.
    pub message: String,
}

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

/// Checks whether a path should be ignored under one combined ordered ignore
/// list (I-5, 02 §2).
///
/// The effective list for a directory is: global rules first, then the stacked
/// per-directory `.galleryignore` matchers from the source root down to the leaf
/// (`dir_rules_stack` is already in root-to-leaf order). Evaluation is unified
/// last-matching-rule-wins: the whole list is scanned in order and the **last**
/// matcher that yields a definitive Ignore/Whitelist decision wins. Within a
/// single matcher, `Gitignore::matched` already applies last-match-wins over its
/// own lines, so scanning the ordered list this way is equivalent to running
/// last-match-wins over every rule in the combined list — a global rule and a
/// local negation are reconciled by one list, not by two objects deciding
/// independently.
pub fn is_ignored(
    path: &Path,
    is_dir: bool,
    dir_rules_stack: &[Gitignore],
    global_rules: &Gitignore,
) -> bool {
    use ignore::Match;

    // One combined ordered list: global first, then locals root-to-leaf.
    let effective = std::iter::once(global_rules).chain(dir_rules_stack.iter());

    // Default is "not ignored"; each definitive match overwrites the running
    // decision, so the last match in list order is the one that stands.
    let mut ignored = false;
    for rules in effective {
        match rules.matched(path, is_dir) {
            Match::Ignore(_) => ignored = true,
            Match::Whitelist(_) => ignored = false,
            Match::None => {}
        }
    }
    ignored
}

/// Validates global ignore patterns line by line (I-5).
///
/// Returns the built matcher when every pattern is valid, or **all** invalid
/// lines (with 1-based line numbers) otherwise. On any error the matcher is not
/// built, so invalid patterns never partially apply. The Settings dialog calls
/// this on Apply to identify invalid lines before persisting.
pub fn validate_global_patterns(
    patterns: &[String],
) -> Result<Gitignore, Vec<IgnoreValidationError>> {
    let mut builder = GitignoreBuilder::new("/");
    let mut errors = Vec::new();
    for (index, pattern) in patterns.iter().enumerate() {
        if let Err(source) = builder.add_line(None, pattern) {
            errors.push(IgnoreValidationError {
                source: "global rules".into(),
                line: Some(index + 1),
                message: source.to_string(),
            });
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    builder.build().map_err(|source| {
        vec![IgnoreValidationError {
            source: "global rules".into(),
            line: None,
            message: source.to_string(),
        }]
    })
}

// I-5: a separate directory-rule validator was removed — the walker's
// [`load_directory_rules`] is the canonical `.galleryignore` handler; a parse
// error there rejects the whole file (no partial apply) and is surfaced as a
// scan error.

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

    #[test]
    fn global_rule_and_local_negation_combine_under_one_last_match_list() {
        // Global ignores every .tmp; a local .galleryignore un-ignores exactly
        // one of them. Under one combined last-match-wins list (I-5), the local
        // negation — later in the list than the global rule — wins for that file,
        // while other .tmp files stay ignored. Two objects deciding independently
        // ("most specific scope wins") could not express this per-file override.
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let global = build_global_rules(&["*.tmp".to_string()]).unwrap();
        fs::write(root.join(".galleryignore"), "!important.tmp\n").unwrap();
        let local = load_directory_rules(root).unwrap().unwrap();
        let stack = vec![local];

        // A generic .tmp: only the global rule matches → ignored.
        assert!(
            is_ignored(&root.join("scratch.tmp"), false, &stack, &global),
            "global *.tmp still ignores files with no local override"
        );

        // The specifically un-ignored file: the local negation is the last
        // matching rule in the combined list → NOT ignored.
        assert!(
            !is_ignored(&root.join("important.tmp"), false, &stack, &global),
            "a local negation overrides an earlier global ignore in the combined list"
        );
    }

    #[test]
    fn invalid_ignore_pattern_does_not_partially_apply_and_is_collected() {
        // A rule set with one invalid line among valid ones must be rejected as a
        // whole (no partial application) and report the offending line (I-5).
        let patterns = vec![
            "*.jpg".to_string(),     // line 1 — valid
            "[z-a].tmp".to_string(), // line 2 — invalid (reversed character range)
            "*.png".to_string(),     // line 3 — valid
        ];

        let result = validate_global_patterns(&patterns);
        let errors = result.expect_err("an invalid pattern must fail validation");

        // The invalid line is identified with a 1-based line number and source,
        // in a form the Settings dialog (U-5) can surface.
        assert_eq!(errors.len(), 1, "exactly the invalid line is reported");
        assert_eq!(errors[0].line, Some(2), "reports the offending line number");
        assert_eq!(errors[0].source, "global rules");
        assert!(!errors[0].message.is_empty(), "carries a reason");

        // And nothing partially applies: no matcher is returned when invalid.
        assert!(
            validate_global_patterns(&patterns).is_err(),
            "an invalid set never yields a partial matcher"
        );
    }

    #[test]
    fn valid_global_patterns_build_successfully() {
        // Control: a fully-valid set validates and builds a usable matcher.
        let matcher = validate_global_patterns(&["*.tmp".to_string(), "!keep.tmp".to_string()])
            .expect("valid patterns build a matcher");
        assert!(matches!(
            matcher.matched("scratch.tmp", false),
            ignore::Match::Ignore(_)
        ));
    }
}
