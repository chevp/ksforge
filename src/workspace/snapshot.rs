use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::domain::{KsforgeError, Result};

/// Directories never worth hashing: VCS metadata, the crate's own state,
/// and common build/dependency output that dwarfs the actual source.
const IGNORED_DIRS: &[&str] = &[
    ".git",
    ".ksforge",
    "target",
    "node_modules",
    "dist",
    "build",
];

/// Content hash of every file under `root`, keyed by path relative to
/// `root`. Used to compute which files an agent turn actually touched
/// without depending on Git (§HFNMflB/§3kuclkU) and without holding every
/// file's bytes in memory at once.
pub fn hash_tree(root: &Path) -> Result<BTreeMap<PathBuf, String>> {
    let mut snapshot = BTreeMap::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| {
            e.file_name()
                .to_str()
                .map(|name| !IGNORED_DIRS.contains(&name))
                .unwrap_or(true)
        })
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let bytes = std::fs::read(entry.path()).map_err(|e| {
            KsforgeError::Workspace(format!("failed to read {}: {e}", entry.path().display()))
        })?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let hash: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let relative = entry
            .path()
            .strip_prefix(root)
            .unwrap_or(entry.path())
            .to_path_buf();
        snapshot.insert(relative, hash);
    }
    Ok(snapshot)
}

/// Files present/changed in `after` but not identical in `before` — added
/// or modified. Deleted files (present in `before`, absent in `after`) are
/// included too, since "changed" for reporting purposes means any of the
/// three.
pub fn changed_files(
    before: &BTreeMap<PathBuf, String>,
    after: &BTreeMap<PathBuf, String>,
) -> Vec<PathBuf> {
    let mut changed: Vec<PathBuf> = Vec::new();
    for (path, hash) in after {
        if before.get(path) != Some(hash) {
            changed.push(path.clone());
        }
    }
    for path in before.keys() {
        if !after.contains_key(path) {
            changed.push(path.clone());
        }
    }
    changed.sort();
    changed.dedup();
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn detects_added_modified_deleted() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("keep.txt"), "same").unwrap();
        fs::write(dir.path().join("edit.txt"), "before").unwrap();
        fs::write(dir.path().join("remove.txt"), "gone soon").unwrap();
        let before = hash_tree(dir.path()).unwrap();

        fs::write(dir.path().join("edit.txt"), "after").unwrap();
        fs::remove_file(dir.path().join("remove.txt")).unwrap();
        fs::write(dir.path().join("new.txt"), "brand new").unwrap();
        let after = hash_tree(dir.path()).unwrap();

        let mut changed = changed_files(&before, &after);
        changed.sort();
        assert_eq!(
            changed,
            vec![
                PathBuf::from("edit.txt"),
                PathBuf::from("new.txt"),
                PathBuf::from("remove.txt"),
            ]
        );
    }
}
