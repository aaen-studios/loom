//! Workspace helpers: which folder a chat is rooted at, and the git branch
//! that folder is on.

use std::path::{Path, PathBuf};

/// Walks up from `start` to find the repository root (a folder with `.git`).
pub fn repo_root(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(candidate) = current {
        if candidate.join(".git").exists() {
            return Some(candidate.to_path_buf());
        }
        current = candidate.parent();
    }
    None
}

/// Current branch name, or a short commit id for detached HEAD.
pub fn git_branch(workdir: &Path) -> Option<String> {
    let root = repo_root(workdir)?;
    let head = std::fs::read_to_string(root.join(".git/HEAD")).ok()?;
    let head = head.trim();

    if let Some(reference) = head.strip_prefix("ref: ") {
        let branch = reference.rsplit('/').next().unwrap_or(reference);
        return Some(branch.to_string());
    }

    // Detached HEAD: show the abbreviated commit.
    if head.len() >= 7 {
        return Some(head[..7].to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_repo_root_from_a_subfolder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        let nested = root.join("src/components");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();

        assert_eq!(repo_root(&nested).as_deref(), Some(root.as_path()));
        assert_eq!(git_branch(&nested).as_deref(), Some("main"));
    }

    #[test]
    fn detached_head_reports_a_short_sha() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::write(
            dir.path().join(".git/HEAD"),
            "0123456789abcdef0123456789abcdef01234567\n",
        )
        .unwrap();

        assert_eq!(git_branch(dir.path()).as_deref(), Some("0123456"));
    }

    #[test]
    fn non_repos_are_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(git_branch(dir.path()).is_none());
    }
}
