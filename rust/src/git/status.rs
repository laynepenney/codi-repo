//! Git status operations

use git2::{Repository, StatusOptions};
use std::path::PathBuf;

use super::cache::STATUS_CACHE;
use super::{get_current_branch, open_repo, path_exists, GitError};
use crate::core::repo::RepoInfo;

/// Repository status information
#[derive(Debug, Clone)]
pub struct RepoStatusInfo {
    /// Current branch name
    pub current_branch: String,
    /// Is the working directory clean
    pub is_clean: bool,
    /// Staged files
    pub staged: Vec<String>,
    /// Modified files (not staged)
    pub modified: Vec<String>,
    /// Untracked files
    pub untracked: Vec<String>,
    /// Commits ahead of remote
    pub ahead: usize,
    /// Commits behind remote
    pub behind: usize,
}

/// Repository status with name
#[derive(Debug, Clone)]
pub struct RepoStatus {
    /// Repository name
    pub name: String,
    /// Current branch
    pub branch: String,
    /// Is clean
    pub clean: bool,
    /// Staged file count
    pub staged: usize,
    /// Modified file count
    pub modified: usize,
    /// Untracked file count
    pub untracked: usize,
    /// Commits ahead
    pub ahead: usize,
    /// Commits behind
    pub behind: usize,
    /// Whether repo exists
    pub exists: bool,
}

/// Get detailed status for a repository
pub fn get_status_info(repo: &Repository) -> Result<RepoStatusInfo, GitError> {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .include_unmodified(false);

    let statuses = repo.statuses(Some(&mut opts))?;

    let mut staged = Vec::new();
    let mut modified = Vec::new();
    let mut untracked = Vec::new();

    for entry in statuses.iter() {
        let path = entry.path().unwrap_or("").to_string();
        let status = entry.status();

        if status.is_index_new()
            || status.is_index_modified()
            || status.is_index_deleted()
            || status.is_index_renamed()
            || status.is_index_typechange()
        {
            staged.push(path.clone());
        }

        if status.is_wt_modified() || status.is_wt_deleted() || status.is_wt_typechange() {
            modified.push(path.clone());
        }

        if status.is_wt_new() {
            untracked.push(path);
        }
    }

    let current_branch = get_current_branch(repo)?;
    let is_clean = staged.is_empty() && modified.is_empty() && untracked.is_empty();

    // Get ahead/behind counts
    let (ahead, behind) = get_ahead_behind(repo).unwrap_or((0, 0));

    Ok(RepoStatusInfo {
        current_branch,
        is_clean,
        staged,
        modified,
        untracked,
        ahead,
        behind,
    })
}

/// Get cached status or compute it
pub fn get_cached_status(repo_path: &PathBuf) -> Result<RepoStatusInfo, GitError> {
    // Check cache first
    if let Some(status) = STATUS_CACHE.get(repo_path) {
        return Ok(status);
    }

    // Compute and cache
    let repo = open_repo(repo_path)?;
    let status = get_status_info(&repo)?;
    STATUS_CACHE.set(repo_path.clone(), status.clone());
    Ok(status)
}

/// Get ahead/behind counts relative to upstream
fn get_ahead_behind(repo: &Repository) -> Option<(usize, usize)> {
    let head = repo.head().ok()?;
    if !head.is_branch() {
        return Some((0, 0));
    }

    let branch_name = head.shorthand()?;
    let local_oid = head.target()?;

    // Try to get upstream
    let branch = repo.find_branch(branch_name, git2::BranchType::Local).ok()?;
    let upstream = branch.upstream().ok()?;
    let upstream_oid = upstream.get().target()?;

    let (ahead, behind) = repo.graph_ahead_behind(local_oid, upstream_oid).ok()?;
    Some((ahead, behind))
}

/// Get repository status
pub fn get_repo_status(repo_info: &RepoInfo) -> RepoStatus {
    if !path_exists(&repo_info.absolute_path) {
        return RepoStatus {
            name: repo_info.name.clone(),
            branch: String::new(),
            clean: true,
            staged: 0,
            modified: 0,
            untracked: 0,
            ahead: 0,
            behind: 0,
            exists: false,
        };
    }

    match get_cached_status(&repo_info.absolute_path) {
        Ok(status) => RepoStatus {
            name: repo_info.name.clone(),
            branch: status.current_branch,
            clean: status.is_clean,
            staged: status.staged.len(),
            modified: status.modified.len(),
            untracked: status.untracked.len(),
            ahead: status.ahead,
            behind: status.behind,
            exists: true,
        },
        Err(_) => RepoStatus {
            name: repo_info.name.clone(),
            branch: "error".to_string(),
            clean: true,
            staged: 0,
            modified: 0,
            untracked: 0,
            ahead: 0,
            behind: 0,
            exists: true,
        },
    }
}

/// Get status for all repositories
pub fn get_all_repo_status(repos: &[RepoInfo]) -> Vec<RepoStatus> {
    repos.iter().map(get_repo_status).collect()
}

/// Get list of changed files (staged, modified, and untracked)
pub fn get_changed_files(repo: &Repository) -> Result<Vec<String>, GitError> {
    let status = get_status_info(repo)?;
    let mut files = status.staged;
    files.extend(status.modified);
    files.extend(status.untracked);
    Ok(files)
}

/// Check if there are uncommitted changes
pub fn has_uncommitted_changes(repo: &Repository) -> Result<bool, GitError> {
    let status = get_status_info(repo)?;
    Ok(!status.is_clean)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_repo() -> (TempDir, Repository) {
        let temp = TempDir::new().unwrap();
        let repo = Repository::init(temp.path()).unwrap();

        // Configure git user for commits
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();

        (temp, repo)
    }

    #[test]
    fn test_clean_repo() {
        let (temp, repo) = setup_test_repo();

        // Create initial commit
        let sig = repo.signature().unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
            .unwrap();

        let status = get_status_info(&repo).unwrap();
        assert!(status.is_clean);
        assert!(status.staged.is_empty());
        assert!(status.modified.is_empty());
        assert!(status.untracked.is_empty());

        drop(temp);
    }

    #[test]
    fn test_untracked_file() {
        let (temp, repo) = setup_test_repo();

        // Create initial commit first (needed for HEAD to exist)
        {
            fs::write(temp.path().join("README.md"), "# Test").unwrap();
            let mut index = repo.index().unwrap();
            index.add_path(std::path::Path::new("README.md")).unwrap();
            index.write().unwrap();
            let sig = repo.signature().unwrap();
            let tree_id = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
                .unwrap();
        }

        // Create an untracked file
        fs::write(temp.path().join("new_file.txt"), "content").unwrap();

        let status = get_status_info(&repo).unwrap();
        assert!(!status.is_clean);
        assert!(status.staged.is_empty());
        assert!(status.modified.is_empty());
        assert_eq!(status.untracked.len(), 1);
        assert!(status.untracked.contains(&"new_file.txt".to_string()));

        drop(temp);
    }

    #[test]
    fn test_staged_file() {
        let (temp, repo) = setup_test_repo();

        // Create initial commit first (needed for HEAD to exist)
        {
            fs::write(temp.path().join("README.md"), "# Test").unwrap();
            let mut index = repo.index().unwrap();
            index.add_path(std::path::Path::new("README.md")).unwrap();
            index.write().unwrap();
            let sig = repo.signature().unwrap();
            let tree_id = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
                .unwrap();
        }

        // Create and stage a file
        fs::write(temp.path().join("staged.txt"), "content").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("staged.txt")).unwrap();
        index.write().unwrap();

        let status = get_status_info(&repo).unwrap();
        assert!(!status.is_clean);
        assert_eq!(status.staged.len(), 1);
        assert!(status.staged.contains(&"staged.txt".to_string()));

        drop(temp);
    }
}
