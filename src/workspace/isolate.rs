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
/// nicely" (§38W1mjg of the base spec).
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
    Ok(strip_windows_verbatim_prefix(canonical))
}

/// `std::fs::canonicalize` on Windows returns the `\\?\`-prefixed
/// extended-length ("verbatim") form (`\\?\C:\...`, or `\\?\UNC\server\share`
/// for a UNC path) — confirmed against a real run: Claude Code's own
/// sandbox flags this syntax as a "suspicious Windows path pattern"
/// requiring manual approval, which a non-interactive `--permission-mode`
/// run has no one to grant, permanently denying every subsequent write for
/// the rest of that session. Every path ksforge hands to an agent as its
/// working directory — and states in the prompt as "Workspace: ..." (see
/// `application::prompt`) — goes through here first, so this is the one
/// place to strip it rather than re-deriving the fix at every call site.
/// A no-op everywhere else, and for any Windows path that (rarely) doesn't
/// carry the prefix to begin with.
#[cfg(windows)]
pub fn strip_windows_verbatim_prefix(path: PathBuf) -> PathBuf {
    let raw = path.as_os_str().to_string_lossy();
    if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = raw.strip_prefix(r"\\?\") {
        PathBuf::from(rest.to_string())
    } else {
        path
    }
}

#[cfg(not(windows))]
pub fn strip_windows_verbatim_prefix(path: PathBuf) -> PathBuf {
    path
}

#[cfg(all(test, windows))]
mod windows_verbatim_prefix_tests {
    use super::*;

    #[test]
    fn strips_a_plain_disk_prefix() {
        assert_eq!(
            strip_windows_verbatim_prefix(PathBuf::from(r"\\?\C:\chevp\apps\ksforge")),
            PathBuf::from(r"C:\chevp\apps\ksforge")
        );
    }

    #[test]
    fn strips_a_unc_prefix() {
        assert_eq!(
            strip_windows_verbatim_prefix(PathBuf::from(r"\\?\UNC\server\share\dir")),
            PathBuf::from(r"\\server\share\dir")
        );
    }

    #[test]
    fn leaves_an_already_plain_path_untouched() {
        assert_eq!(
            strip_windows_verbatim_prefix(PathBuf::from(r"C:\chevp\apps\ksforge")),
            PathBuf::from(r"C:\chevp\apps\ksforge")
        );
    }
}
