//! Deterministic, per-language heuristics backing `Capability::change_scope`
//! (`ChangeScope::TestsOnly`) — host-enforced after the ACT phase, not left
//! to the prompt (see docs/03-architecture.md, "The phase loop").
//!
//! Two checks, not one, because not every language keeps tests in
//! dedicated files: most do (Go's `_test.go`, JS/TS's `.test.js`/
//! `.spec.ts`, Python's `test_*.py`, Ruby's `_spec.rb`, Java/C#'s
//! `*Test(s).*`, anything under a `tests`/`test`/`spec`/`__tests__`
//! directory), but Rust's own convention is a `#[cfg(test)] mod tests`
//! colocated in the *same* file as the implementation it tests — ksforge's
//! own codebase does this throughout. A pure path check would reject every
//! legitimate Rust test addition, so a file that isn't a dedicated test
//! path is still allowed if it contains a recognized inline-test marker
//! after the change.
//!
//! This is a heuristic, not a real diff: it cannot prove a mixed file's
//! *only* change was to its test portion, only that the file plausibly
//! contains tests at all. Good enough to catch "the model edited an
//! unrelated file it had no business touching," not a substitute for
//! actual code review.

use std::path::{Path, PathBuf};

const TEST_DIR_NAMES: &[&str] = &["tests", "test", "__tests__", "spec", "specs"];

/// Is this path a dedicated test file, or under a dedicated test
/// directory, by common per-language convention?
pub fn is_test_path(path: &Path) -> bool {
    if path
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .any(|c| TEST_DIR_NAMES.contains(&c.to_lowercase().as_str()))
    {
        return true;
    }
    let Some(name) = path.file_name().and_then(|f| f.to_str()) else {
        return false;
    };
    let name = name.to_lowercase();
    name.starts_with("test_")
        || name.contains(".test.")
        || name.contains(".spec.")
        || name.ends_with("_test.go")
        || name.ends_with("_test.py")
        || name.ends_with("_test.rs")
        || name.ends_with("_tests.rs")
        || name.ends_with("_spec.rb")
        || name.ends_with("test.java")
        || name.ends_with("tests.java")
        || name.ends_with("test.cs")
        || name.ends_with("tests.cs")
        || name.ends_with("test.kt")
        || name.ends_with("test.php")
}

/// Does this file's content already show a recognized inline-test marker
/// for its extension — the colocated-tests escape hatch (Rust, primarily).
/// Checked against the file's content *after* the change: adding an inline
/// test block to a previously test-free file is exactly the legitimate
/// case this exists to allow.
pub fn has_inline_test_marker(path: &Path, content: &str) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs") => content.contains("#[cfg(test)]"),
        Some("py") => content.contains("import unittest") || content.contains("import pytest"),
        _ => false,
    }
}

/// Every `changed_files` entry (relative to `working_dir`) that is neither
/// a dedicated test path nor a file with a recognized inline-test marker.
/// Empty means the change stayed within the tests-only scope.
pub fn violations(changed_files: &[PathBuf], working_dir: &Path) -> Vec<PathBuf> {
    changed_files
        .iter()
        .filter(|path| {
            if is_test_path(path) {
                return false;
            }
            let content = std::fs::read_to_string(working_dir.join(path)).unwrap_or_default();
            !has_inline_test_marker(path, &content)
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_dedicated_test_paths() {
        assert!(is_test_path(Path::new("tests/foo.rs")));
        assert!(is_test_path(Path::new("src/foo_test.go")));
        assert!(is_test_path(Path::new("src/foo.test.js")));
        assert!(is_test_path(Path::new("src/foo.spec.ts")));
        assert!(is_test_path(Path::new("test_foo.py")));
        assert!(is_test_path(Path::new("foo_spec.rb")));
        assert!(is_test_path(Path::new("FooTest.java")));
        assert!(is_test_path(Path::new("__tests__/foo.js")));
    }

    #[test]
    fn rejects_ordinary_source_paths() {
        assert!(!is_test_path(Path::new("src/main.rs")));
        assert!(!is_test_path(Path::new("src/lib.py")));
        assert!(!is_test_path(Path::new("src/app.js")));
    }

    #[test]
    fn inline_marker_allows_colocated_rust_tests() {
        assert!(has_inline_test_marker(
            Path::new("src/lib.rs"),
            "fn foo() {}\n\n#[cfg(test)]\nmod tests {}\n"
        ));
        assert!(!has_inline_test_marker(
            Path::new("src/lib.rs"),
            "fn foo() {}\n"
        ));
    }

    #[test]
    fn violations_flags_a_plain_source_file_with_no_test_marker() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}\n").unwrap();
        let changed = vec![PathBuf::from("main.rs")];
        assert_eq!(violations(&changed, dir.path()), changed);
    }

    #[test]
    fn violations_allows_a_dedicated_test_path() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("tests")).unwrap();
        std::fs::write(
            dir.path().join("tests/it.rs"),
            "#[test]\nfn it_works() {}\n",
        )
        .unwrap();
        let changed = vec![PathBuf::from("tests/it.rs")];
        assert!(violations(&changed, dir.path()).is_empty());
    }

    #[test]
    fn violations_allows_a_colocated_rust_test_addition() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("lib.rs"),
            "pub fn add(a: i32, b: i32) -> i32 { a + b }\n\n#[cfg(test)]\nmod tests {\n    \
             use super::*;\n    #[test]\n    fn adds() { assert_eq!(add(1, 1), 2); }\n}\n",
        )
        .unwrap();
        let changed = vec![PathBuf::from("lib.rs")];
        assert!(violations(&changed, dir.path()).is_empty());
    }

    #[test]
    fn violations_flags_a_non_test_path_that_no_longer_exists() {
        // e.g. a file the agent created then deleted again within the same
        // turn — content can't be inspected, so the safe default is to
        // reject rather than silently pass an unverifiable change.
        let dir = tempfile::tempdir().unwrap();
        let changed = vec![PathBuf::from("gone.rs")];
        assert_eq!(violations(&changed, dir.path()), changed);
    }
}
