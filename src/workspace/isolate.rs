use std::path::{Path, PathBuf};

use tempfile::TempDir;
use walkdir::WalkDir;

use crate::domain::{KsforgeError, Result};

const IGNORED_DIRS: &[&str] = &[
    ".git",
    ".ksforge",
    "target",
    "node_modules",
    "dist",
    "build",
];

/// A dry-run workspace: a full copy of the real workspace in a temp
/// directory that Claude Code is pointed at instead. Dropping this value
/// deletes the copy. Never claim `--dry-run` is safe without this — the
/// real workspace must be structurally unreachable, not just "we asked
/// nicely" (section 25 of the base spec).
pub struct IsolatedWorkspace {
    dir: TempDir,
}

impl IsolatedWorkspace {
    pub fn path(&self) -> &Path {
        self.dir.path()
    }
}

pub fn prepare(root: &Path) -> Result<IsolatedWorkspace> {
    let dir = tempfile::tempdir()
        .map_err(|e| KsforgeError::Workspace(format!("failed to create temp workspace: {e}")))?;

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
        let relative = entry.path().strip_prefix(root).unwrap_or(entry.path());
        let target = dir.path().join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| {
                KsforgeError::Workspace(format!("failed to create {}: {e}", target.display()))
            })?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            std::fs::copy(entry.path(), &target).map_err(|e| {
                KsforgeError::Workspace(format!(
                    "failed to copy {} into isolated workspace: {e}",
                    entry.path().display()
                ))
            })?;
        }
    }

    Ok(IsolatedWorkspace { dir })
}

/// Resolve `workspace_root`, rejecting anything that is not an existing
/// directory. Kept deliberately small: ksforge only ever needs a single
/// trusted root, unlike a model-controlled multi-path materialization
/// scheme (which this architecture has no equivalent of, since Claude Code
/// itself edits files directly rather than returning a change-set for
/// ksforge to apply).
pub fn resolve_root(path: &Path) -> Result<PathBuf> {
    let canonical = std::fs::canonicalize(path).map_err(|e| {
        KsforgeError::Workspace(format!(
            "workspace {} is not accessible: {e}",
            path.display()
        ))
    })?;
    if !canonical.is_dir() {
        return Err(KsforgeError::Workspace(format!(
            "workspace {} is not a directory",
            canonical.display()
        )));
    }
    Ok(canonical)
}
