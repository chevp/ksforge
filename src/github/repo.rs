//! Attributes changed files to the git repo that actually owns them — the
//! piece that makes ksforge's git plumbing work when `--workspace` is not
//! itself a repo root: a plain folder holding several independent repos, or
//! a repo containing submodules. `application`/`workspace` stay git-free
//! (§HFNMflB/§3kuclkU), so this lives here, the one place that already
//! knows about Git.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use super::process::run_git;

/// Directories never worth descending into while looking for repos: VCS
/// metadata, ksforge's own state, and common build/dependency output —
/// same list `workspace::snapshot`/`workspace::isolate` skip for the same
/// reason, duplicated rather than shared (each is a small, local const;
/// see those modules' own copies).
const IGNORED_DIRS: &[&str] = &[
    ".git",
    ".ksforge",
    "target",
    "node_modules",
    "dist",
    "build",
];

/// Safety net against an accidentally huge `--workspace` (e.g. a drive
/// root): stop discovering after this many directories visited, rather
/// than walking forever.
const DISCOVER_VISIT_CAP: usize = 100_000;

/// Nearest ancestor of `start_dir` (inclusive) that looks like a git repo
/// root — a `.git` entry, directory or file (a submodule's `.git` is a
/// file: `gitdir: ../.git/modules/...`). Never searches above `boundary`,
/// so ksforge never reaches for a repo outside the workspace it was given
/// even if one happens to exist further up the filesystem.
fn nearest_repo_root(start_dir: &Path, boundary: &Path) -> Option<PathBuf> {
    let mut current = start_dir;
    loop {
        if current.join(".git").exists() {
            return Some(current.to_path_buf());
        }
        if current == boundary {
            return None;
        }
        current = current.parent()?;
    }
}

/// Groups `changed_files` (each relative to `workspace_root`, as
/// `ExecutionResult::changed_files` always is) by the repo that owns them,
/// returning `(repo root, files relative to that repo root)` pairs sorted
/// by repo-root depth **descending** — deepest first.
///
/// That ordering is what makes submodules and nested/sibling repos come out
/// correct without special-casing them: committing the innermost repo
/// first means its working tree is clean by the time an enclosing repo's
/// `git add <submodule-path>` runs, so the enclosing commit picks up the
/// updated gitlink the normal way.
///
/// A changed file with no `.git` between it and `workspace_root` (a plain
/// non-git workspace folder, or a file outside any discoverable repo) is
/// returned separately in the second element — there is structurally
/// nothing to commit it to.
pub(super) fn group_by_repo(
    workspace_root: &Path,
    changed_files: &[PathBuf],
) -> (Vec<(PathBuf, Vec<PathBuf>)>, Vec<PathBuf>) {
    let mut groups: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
    let mut unversioned = Vec::new();

    for file in changed_files {
        let absolute = workspace_root.join(file);
        let start_dir = absolute.parent().unwrap_or(workspace_root);
        match nearest_repo_root(start_dir, workspace_root) {
            Some(repo) => {
                let relative = absolute.strip_prefix(&repo).unwrap_or(&absolute);
                groups.entry(repo).or_default().push(relative.to_path_buf());
            }
            None => unversioned.push(file.clone()),
        }
    }

    let mut grouped: Vec<(PathBuf, Vec<PathBuf>)> = groups.into_iter().collect();
    grouped.sort_by_key(|(repo, _)| std::cmp::Reverse(repo.components().count()));

    (grouped, unversioned)
}

/// Downward walk from `root` looking for git repos — the counterpart to
/// [`nearest_repo_root`]'s upward search, used by `ksforge status` (no
/// execution id) to build a workspace-wide overview rather than attribute
/// already-known changed files. A directory containing `.git` is a repo
/// leaf: reported, then pruned (`IntoIter::skip_current_dir`) so a
/// submodule or vendored repo inside it is not also reported as a separate
/// top-level entry. `root` itself counts, so a single-repo `--workspace`
/// (today's common case) yields exactly one entry with no wasted walk.
///
/// The second return value is `true` if [`DISCOVER_VISIT_CAP`] was hit —
/// the scan stopped early rather than running unbounded over an
/// unexpectedly huge, non-repo `--workspace` (e.g. a drive root).
pub fn discover_repos(root: &Path) -> (Vec<PathBuf>, bool) {
    let mut repos = Vec::new();
    let mut visited = 0usize;
    let mut capped = false;

    let mut it = WalkDir::new(root).into_iter();
    while let Some(entry) = it.next() {
        visited += 1;
        if visited > DISCOVER_VISIT_CAP {
            capped = true;
            break;
        }
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_dir() {
            continue;
        }
        let name = entry.file_name().to_str().unwrap_or("");
        if entry.depth() > 0 && IGNORED_DIRS.contains(&name) {
            it.skip_current_dir();
            continue;
        }
        if entry.path().join(".git").exists() {
            repos.push(entry.path().to_path_buf());
            it.skip_current_dir();
        }
    }

    (repos, capped)
}

/// A repo's branch and working-tree cleanliness — just enough for
/// `ksforge status`'s workspace overview, not a full `git status` report.
pub struct GitState {
    pub branch: String,
    pub dirty: bool,
}

/// Never fails: a repo this can't read git state for (permissions, a
/// broken `.git`, a detached `HEAD`) still gets a row in the overview
/// rather than aborting the whole scan.
pub async fn git_state(repo: &Path) -> GitState {
    let branch = match run_git(repo, &["symbolic-ref", "--quiet", "--short", "HEAD"]).await {
        Ok(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => "detached".to_string(),
    };
    let dirty = run_git(repo, &["status", "--porcelain=v1"])
        .await
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    GitState { branch, dirty }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_repo_at_workspace_root_yields_one_group() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join("README.md"), "hi").unwrap();

        let (groups, unversioned) = group_by_repo(dir.path(), &[PathBuf::from("README.md")]);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0, dir.path());
        assert_eq!(groups[0].1, vec![PathBuf::from("README.md")]);
        assert!(unversioned.is_empty());
    }

    #[test]
    fn files_with_no_git_anywhere_are_unversioned() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("notes.md"), "hi").unwrap();

        let (groups, unversioned) = group_by_repo(dir.path(), &[PathBuf::from("notes.md")]);

        assert!(groups.is_empty());
        assert_eq!(unversioned, vec![PathBuf::from("notes.md")]);
    }

    #[test]
    fn sibling_repos_under_a_non_git_root_group_separately() {
        let root = tempfile::tempdir().unwrap();
        for repo in ["one", "two"] {
            std::fs::create_dir_all(root.path().join(repo).join(".git")).unwrap();
            std::fs::write(root.path().join(repo).join("f.txt"), repo).unwrap();
        }

        let (groups, unversioned) = group_by_repo(
            root.path(),
            &[PathBuf::from("one/f.txt"), PathBuf::from("two/f.txt")],
        );

        assert_eq!(groups.len(), 2);
        assert!(unversioned.is_empty());
        let roots: Vec<_> = groups.iter().map(|(r, _)| r.clone()).collect();
        assert!(roots.contains(&root.path().join("one")));
        assert!(roots.contains(&root.path().join("two")));
    }

    #[test]
    fn a_nested_repo_groups_deeper_than_its_parent_and_sorts_first() {
        let outer = tempfile::tempdir().unwrap();
        std::fs::create_dir(outer.path().join(".git")).unwrap();
        std::fs::write(outer.path().join("outer.txt"), "outer").unwrap();
        let inner = outer.path().join("vendor").join("lib");
        std::fs::create_dir_all(inner.join(".git")).unwrap();
        std::fs::write(inner.join("inner.txt"), "inner").unwrap();

        let (groups, unversioned) = group_by_repo(
            outer.path(),
            &[
                PathBuf::from("outer.txt"),
                PathBuf::from("vendor/lib/inner.txt"),
            ],
        );

        assert!(unversioned.is_empty());
        assert_eq!(groups.len(), 2);
        // Deepest repo root first.
        assert_eq!(groups[0].0, inner);
        assert_eq!(groups[0].1, vec![PathBuf::from("inner.txt")]);
        assert_eq!(groups[1].0, outer.path());
        assert_eq!(groups[1].1, vec![PathBuf::from("outer.txt")]);
    }

    #[test]
    fn never_searches_above_workspace_root() {
        let root = tempfile::tempdir().unwrap();
        // A `.git` directly above `root` (in its parent) must not be found —
        // `root` itself has no `.git`, so this file should be unversioned.
        std::fs::write(root.path().join("f.txt"), "hi").unwrap();

        let (groups, unversioned) = group_by_repo(root.path(), &[PathBuf::from("f.txt")]);

        assert!(groups.is_empty());
        assert_eq!(unversioned, vec![PathBuf::from("f.txt")]);
    }

    #[test]
    fn discover_repos_finds_the_root_itself_when_it_is_a_repo() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();

        let (repos, capped) = discover_repos(dir.path());

        assert_eq!(repos, vec![dir.path().to_path_buf()]);
        assert!(!capped);
    }

    #[test]
    fn discover_repos_finds_sibling_repos_under_a_non_git_root() {
        let root = tempfile::tempdir().unwrap();
        for repo in ["one", "two"] {
            std::fs::create_dir_all(root.path().join(repo).join(".git")).unwrap();
        }
        std::fs::create_dir(root.path().join("not-a-repo")).unwrap();

        let (mut repos, capped) = discover_repos(root.path());
        repos.sort();

        assert_eq!(
            repos,
            vec![root.path().join("one"), root.path().join("two")]
        );
        assert!(!capped);
    }

    #[test]
    fn discover_repos_does_not_descend_into_a_found_repo() {
        let root = tempfile::tempdir().unwrap();
        let outer = root.path().join("outer");
        std::fs::create_dir_all(outer.join(".git")).unwrap();
        // A nested repo (e.g. a submodule) inside `outer` must not be
        // reported separately — `outer` itself is already the leaf.
        std::fs::create_dir_all(outer.join("vendor/lib").join(".git")).unwrap();

        let (repos, _) = discover_repos(root.path());

        assert_eq!(repos, vec![outer]);
    }

    #[test]
    fn discover_repos_skips_ignored_directories() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("node_modules/some-pkg").join(".git")).unwrap();

        let (repos, _) = discover_repos(root.path());

        assert!(repos.is_empty());
    }

    #[tokio::test]
    async fn git_state_reports_clean_and_dirty() {
        let dir = tempfile::tempdir().unwrap();
        run_git(dir.path(), &["init", "-q", "-b", "main"])
            .await
            .unwrap();
        std::fs::write(dir.path().join("f.txt"), "hi").unwrap();
        run_git(dir.path(), &["add", "f.txt"]).await.unwrap();
        run_git(
            dir.path(),
            &["-c", "user.email=t@t.test", "-c", "user.name=t"],
        )
        .await
        .ok();
        run_git(
            dir.path(),
            &[
                "-c",
                "user.email=t@t.test",
                "-c",
                "user.name=t",
                "commit",
                "-q",
                "-m",
                "initial",
            ],
        )
        .await
        .unwrap();

        let clean = git_state(dir.path()).await;
        assert_eq!(clean.branch, "main");
        assert!(!clean.dirty);

        std::fs::write(dir.path().join("f.txt"), "changed").unwrap();
        let dirty = git_state(dir.path()).await;
        assert!(dirty.dirty);
    }
}
