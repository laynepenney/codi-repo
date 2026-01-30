//! Git remote operations

use git2::{Cred, FetchOptions, PushOptions, RemoteCallbacks, Repository};
use std::env;
use std::path::Path;

use super::cache::invalidate_status_cache;
use super::{get_current_branch, GitError};

/// Get the URL of a remote
pub fn get_remote_url(repo: &Repository, remote: &str) -> Result<Option<String>, GitError> {
    match repo.find_remote(remote) {
        Ok(remote) => Ok(remote.url().map(|s| s.to_string())),
        Err(e) if e.code() == git2::ErrorCode::NotFound => Ok(None),
        Err(e) => Err(GitError::Git(e)),
    }
}

/// Set the URL of a remote (creates if it doesn't exist)
pub fn set_remote_url(repo: &Repository, remote: &str, url: &str) -> Result<(), GitError> {
    if get_remote_url(repo, remote)?.is_none() {
        repo.remote(remote, url)?;
    } else {
        repo.remote_set_url(remote, url)?;
    }
    Ok(())
}

/// Create remote callbacks with SSH authentication
fn create_callbacks<'a>() -> RemoteCallbacks<'a> {
    let mut callbacks = RemoteCallbacks::new();

    callbacks.credentials(|_url, username_from_url, allowed_types| {
        if allowed_types.contains(git2::CredentialType::SSH_KEY) {
            // Try SSH agent first
            if let Ok(cred) = Cred::ssh_key_from_agent(username_from_url.unwrap_or("git")) {
                return Ok(cred);
            }

            // Fall back to default SSH key
            let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
            let ssh_key = Path::new(&home).join(".ssh").join("id_rsa");

            if ssh_key.exists() {
                return Cred::ssh_key(
                    username_from_url.unwrap_or("git"),
                    None,
                    &ssh_key,
                    None,
                );
            }

            // Try ed25519 key
            let ssh_key_ed = Path::new(&home).join(".ssh").join("id_ed25519");
            if ssh_key_ed.exists() {
                return Cred::ssh_key(
                    username_from_url.unwrap_or("git"),
                    None,
                    &ssh_key_ed,
                    None,
                );
            }
        }

        if allowed_types.contains(git2::CredentialType::USER_PASS_PLAINTEXT) {
            // Try to get credentials from environment
            if let (Ok(user), Ok(pass)) = (env::var("GIT_USER"), env::var("GIT_PASSWORD")) {
                return Cred::userpass_plaintext(&user, &pass);
            }
        }

        Cred::default()
    });

    callbacks
}

/// Fetch from remote
pub fn fetch_remote(repo: &Repository, remote: &str) -> Result<(), GitError> {
    let mut remote = repo.find_remote(remote)?;

    let mut fo = FetchOptions::new();
    fo.remote_callbacks(create_callbacks());

    remote.fetch(&[] as &[&str], Some(&mut fo), None)?;
    Ok(())
}

/// Pull latest changes (fetch + merge)
pub fn pull_latest(repo: &Repository, remote: &str) -> Result<(), GitError> {
    // Fetch first
    fetch_remote(repo, remote)?;

    // Get current branch
    let branch_name = get_current_branch(repo)?;
    let remote_ref = format!("{}/{}", remote, branch_name);

    // Find the remote branch
    let remote_branch = match repo.find_branch(&remote_ref, git2::BranchType::Remote) {
        Ok(b) => b,
        Err(_) => return Ok(()), // No remote branch to merge
    };

    let remote_commit = remote_branch.get().peel_to_commit()?;

    // Merge
    let annotated_commit = repo.find_annotated_commit(remote_commit.id())?;

    let (analysis, _) = repo.merge_analysis(&[&annotated_commit])?;

    if analysis.is_up_to_date() {
        return Ok(());
    }

    if analysis.is_fast_forward() {
        // Fast-forward merge
        let refname = format!("refs/heads/{}", branch_name);
        let mut reference = repo.find_reference(&refname)?;
        reference.set_target(remote_commit.id(), "Fast-forward")?;
        repo.set_head(&refname)?;
        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;
    } else if analysis.is_normal() {
        // Normal merge would be needed - for now, error
        return Err(GitError::OperationFailed(
            "Non-fast-forward merge required. Please merge manually.".to_string(),
        ));
    }

    // Invalidate cache
    if let Some(path) = repo.path().parent() {
        invalidate_status_cache(&path.to_path_buf());
    }

    Ok(())
}

/// Push branch to remote
pub fn push_branch(
    repo: &Repository,
    branch_name: &str,
    remote: &str,
    set_upstream: bool,
) -> Result<(), GitError> {
    let mut remote = repo.find_remote(remote)?;

    let mut po = PushOptions::new();
    po.remote_callbacks(create_callbacks());

    let refspec = format!("refs/heads/{}:refs/heads/{}", branch_name, branch_name);
    remote.push(&[&refspec], Some(&mut po))?;

    // Set upstream tracking if requested
    if set_upstream {
        let remote_name = remote.name().map(|s| s.to_string()).unwrap_or_else(|| "origin".to_string());
        let upstream_name = format!("{}/{}", remote_name, branch_name);

        // Need to fetch first to have the remote tracking branch
        drop(remote);
        fetch_remote(repo, &remote_name)?;

        let mut branch = repo.find_branch(branch_name, git2::BranchType::Local)?;
        if let Ok(upstream) = repo.find_branch(&upstream_name, git2::BranchType::Remote) {
            branch.set_upstream(Some(upstream.name()?.unwrap_or(&upstream_name)))?;
        }
    }

    Ok(())
}

/// Force push branch to remote
pub fn force_push_branch(repo: &Repository, branch_name: &str, remote: &str) -> Result<(), GitError> {
    let mut remote = repo.find_remote(remote)?;

    let mut po = PushOptions::new();
    po.remote_callbacks(create_callbacks());

    let refspec = format!("+refs/heads/{}:refs/heads/{}", branch_name, branch_name);
    remote.push(&[&refspec], Some(&mut po))?;

    Ok(())
}

/// Delete a remote branch
pub fn delete_remote_branch(repo: &Repository, branch_name: &str, remote: &str) -> Result<(), GitError> {
    let mut remote = repo.find_remote(remote)?;

    let mut po = PushOptions::new();
    po.remote_callbacks(create_callbacks());

    let refspec = format!(":refs/heads/{}", branch_name);
    remote.push(&[&refspec], Some(&mut po))?;

    Ok(())
}

/// Get upstream tracking branch name
pub fn get_upstream_branch(repo: &Repository, branch_name: Option<&str>) -> Result<Option<String>, GitError> {
    let branch_name = match branch_name {
        Some(name) => name.to_string(),
        None => get_current_branch(repo)?,
    };

    let branch = repo.find_branch(&branch_name, git2::BranchType::Local)?;

    match branch.upstream() {
        Ok(upstream) => Ok(upstream.name()?.map(|s| s.to_string())),
        Err(e) if e.code() == git2::ErrorCode::NotFound => Ok(None),
        Err(e) => Err(GitError::Git(e)),
    }
}

/// Check if upstream branch exists on remote
pub fn upstream_branch_exists(repo: &Repository, remote: &str) -> Result<bool, GitError> {
    let upstream = get_upstream_branch(repo, None)?;
    match upstream {
        Some(name) => {
            // The name is like "origin/branch", extract just branch name
            let branch_name = name.split('/').last().unwrap_or(&name);
            Ok(super::branch::remote_branch_exists(repo, branch_name, remote))
        }
        None => Ok(false),
    }
}

/// Set upstream tracking for the current branch
pub fn set_upstream_branch(repo: &Repository, remote: &str) -> Result<(), GitError> {
    let branch_name = get_current_branch(repo)?;
    let mut branch = repo.find_branch(&branch_name, git2::BranchType::Local)?;

    let upstream_name = format!("{}/{}", remote, branch_name);
    branch.set_upstream(Some(&upstream_name))?;

    Ok(())
}

/// Hard reset to a target
pub fn reset_hard(repo: &Repository, target: &str) -> Result<(), GitError> {
    let obj = repo.revparse_single(target)?;
    let commit = obj.peel_to_commit()?;

    repo.reset(commit.as_object(), git2::ResetType::Hard, None)?;

    // Invalidate cache
    if let Some(path) = repo.path().parent() {
        invalidate_status_cache(&path.to_path_buf());
    }

    Ok(())
}

/// Safe pull that handles deleted upstream branches
pub fn safe_pull_latest(
    repo: &Repository,
    default_branch: &str,
    remote: &str,
) -> Result<SafePullResult, GitError> {
    let current_branch = get_current_branch(repo)?;

    // If on default branch, just pull
    if current_branch == default_branch {
        return match pull_latest(repo, remote) {
            Ok(()) => Ok(SafePullResult {
                pulled: true,
                recovered: false,
                message: None,
            }),
            Err(e) => Ok(SafePullResult {
                pulled: false,
                recovered: false,
                message: Some(e.to_string()),
            }),
        };
    }

    // Check if upstream exists
    let has_upstream = get_upstream_branch(repo, None)?.is_some();
    let upstream_exists = upstream_branch_exists(repo, remote)?;

    if !upstream_exists {
        if !has_upstream {
            // Never pushed - don't auto-switch
            return Ok(SafePullResult {
                pulled: false,
                recovered: false,
                message: Some(format!(
                    "Branch '{}' has no upstream configured. Push with 'gr push -u' first, or checkout '{}' manually.",
                    current_branch, default_branch
                )),
            });
        }

        // Check for local-only commits
        let has_local_commits = super::branch::has_commits_ahead(repo, default_branch)?;
        if has_local_commits {
            return Ok(SafePullResult {
                pulled: false,
                recovered: false,
                message: Some(format!(
                    "Branch '{}' has local commits not in '{}'. Push your changes or merge manually.",
                    current_branch, default_branch
                )),
            });
        }

        // Safe to switch - upstream was deleted and no local work would be lost
        super::branch::checkout_branch(repo, default_branch)?;
        pull_latest(repo, remote)?;

        return Ok(SafePullResult {
            pulled: true,
            recovered: true,
            message: Some(format!(
                "Switched from '{}' to '{}' (upstream branch was deleted)",
                current_branch, default_branch
            )),
        });
    }

    // Normal pull
    match pull_latest(repo, remote) {
        Ok(()) => Ok(SafePullResult {
            pulled: true,
            recovered: false,
            message: None,
        }),
        Err(e) => Ok(SafePullResult {
            pulled: false,
            recovered: false,
            message: Some(e.to_string()),
        }),
    }
}

/// Result of safe_pull_latest
#[derive(Debug, Clone)]
pub struct SafePullResult {
    /// Whether pull succeeded
    pub pulled: bool,
    /// Whether recovery was needed (switched to default branch)
    pub recovered: bool,
    /// Optional message
    pub message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_repo() -> (TempDir, Repository) {
        let temp = TempDir::new().unwrap();
        let repo = Repository::init(temp.path()).unwrap();

        {
            let mut config = repo.config().unwrap();
            config.set_str("user.name", "Test User").unwrap();
            config.set_str("user.email", "test@example.com").unwrap();
        }

        // Create initial commit
        fs::write(temp.path().join("README.md"), "# Test").unwrap();
        {
            let mut index = repo.index().unwrap();
            index.add_path(std::path::Path::new("README.md")).unwrap();
            index.write().unwrap();

            let sig = repo.signature().unwrap();
            let tree_id = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
                .unwrap();
        }

        (temp, repo)
    }

    #[test]
    fn test_get_remote_url() {
        let (temp, repo) = setup_test_repo();

        // No remote yet
        assert!(get_remote_url(&repo, "origin").unwrap().is_none());

        // Add remote
        repo.remote("origin", "https://github.com/test/repo.git")
            .unwrap();
        let url = get_remote_url(&repo, "origin").unwrap();
        assert_eq!(url, Some("https://github.com/test/repo.git".to_string()));

        drop(temp);
    }

    #[test]
    fn test_set_remote_url() {
        let (temp, repo) = setup_test_repo();

        // Create new remote
        set_remote_url(&repo, "origin", "https://github.com/test/repo1.git").unwrap();
        assert_eq!(
            get_remote_url(&repo, "origin").unwrap(),
            Some("https://github.com/test/repo1.git".to_string())
        );

        // Update remote
        set_remote_url(&repo, "origin", "https://github.com/test/repo2.git").unwrap();
        assert_eq!(
            get_remote_url(&repo, "origin").unwrap(),
            Some("https://github.com/test/repo2.git".to_string())
        );

        drop(temp);
    }
}
