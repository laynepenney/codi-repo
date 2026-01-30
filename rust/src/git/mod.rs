//! Git operations wrapper
//!
//! Provides a unified interface for git operations using git2.

pub mod branch;
pub mod cache;
pub mod remote;
pub mod status;

pub use branch::*;
pub use cache::{invalidate_status_cache, GitStatusCache, STATUS_CACHE};
pub use remote::*;
pub use status::*;

use git2::Repository;
use std::path::Path;
use thiserror::Error;

/// Errors that can occur during git operations
#[derive(Error, Debug)]
pub enum GitError {
    #[error("Git error: {0}")]
    Git(#[from] git2::Error),

    #[error("Repository not found: {0}")]
    NotFound(String),

    #[error("Not a git repository: {0}")]
    NotARepo(String),

    #[error("Branch not found: {0}")]
    BranchNotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Operation failed: {0}")]
    OperationFailed(String),
}

/// Open a git repository at the given path
pub fn open_repo<P: AsRef<Path>>(path: P) -> Result<Repository, GitError> {
    Repository::open(path.as_ref()).map_err(|e| {
        if e.code() == git2::ErrorCode::NotFound {
            GitError::NotARepo(path.as_ref().display().to_string())
        } else {
            GitError::Git(e)
        }
    })
}

/// Check if a path is a git repository
pub fn is_git_repo<P: AsRef<Path>>(path: P) -> bool {
    Repository::open(path.as_ref()).is_ok()
}

/// Check if a path exists
pub fn path_exists<P: AsRef<Path>>(path: P) -> bool {
    path.as_ref().exists()
}

/// Clone a repository
pub fn clone_repo(url: &str, path: &Path, branch: Option<&str>) -> Result<Repository, GitError> {
    let mut builder = git2::build::RepoBuilder::new();

    if let Some(branch_name) = branch {
        builder.branch(branch_name);
    }

    builder.clone(url, path).map_err(GitError::Git)
}

/// Get the current branch name
pub fn get_current_branch(repo: &Repository) -> Result<String, GitError> {
    let head = repo.head()?;

    if head.is_branch() {
        if let Some(name) = head.shorthand() {
            return Ok(name.to_string());
        }
    }

    // Detached HEAD - return short hash
    if let Some(target) = head.target() {
        return Ok(format!("(HEAD detached at {})", &target.to_string()[..7]));
    }

    Ok("HEAD".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_is_git_repo() {
        let temp = TempDir::new().unwrap();
        assert!(!is_git_repo(temp.path()));

        // Initialize a git repo
        Repository::init(temp.path()).unwrap();
        assert!(is_git_repo(temp.path()));
    }

    #[test]
    fn test_path_exists() {
        let temp = TempDir::new().unwrap();
        assert!(path_exists(temp.path()));
        assert!(!path_exists(temp.path().join("nonexistent")));
    }

    #[test]
    fn test_open_repo() {
        let temp = TempDir::new().unwrap();

        // Should fail for non-repo
        assert!(open_repo(temp.path()).is_err());

        // Should succeed after init
        Repository::init(temp.path()).unwrap();
        assert!(open_repo(temp.path()).is_ok());
    }
}
