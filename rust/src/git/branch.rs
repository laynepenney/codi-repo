//! Git branch operations

use git2::{BranchType, Repository};

use super::{get_current_branch, GitError};

/// Create a new local branch and check it out
pub fn create_and_checkout_branch(repo: &Repository, branch_name: &str) -> Result<(), GitError> {
    let head = repo.head()?;
    let commit = head.peel_to_commit()?;

    // Create branch
    repo.branch(branch_name, &commit, false)?;

    // Checkout the new branch
    let refname = format!("refs/heads/{}", branch_name);
    let obj = repo.revparse_single(&refname)?;
    repo.checkout_tree(&obj, None)?;
    repo.set_head(&refname)?;

    Ok(())
}

/// Checkout an existing branch
pub fn checkout_branch(repo: &Repository, branch_name: &str) -> Result<(), GitError> {
    let refname = format!("refs/heads/{}", branch_name);

    // Check if branch exists
    if repo.find_reference(&refname).is_err() {
        return Err(GitError::BranchNotFound(branch_name.to_string()));
    }

    let obj = repo.revparse_single(&refname)?;
    repo.checkout_tree(&obj, None)?;
    repo.set_head(&refname)?;

    Ok(())
}

/// Check if a local branch exists
pub fn branch_exists(repo: &Repository, branch_name: &str) -> bool {
    repo.find_branch(branch_name, BranchType::Local).is_ok()
}

/// Check if a remote branch exists
pub fn remote_branch_exists(repo: &Repository, branch_name: &str, remote: &str) -> bool {
    let remote_ref = format!("{}/{}", remote, branch_name);
    repo.find_branch(&remote_ref, BranchType::Remote).is_ok()
}

/// Delete a local branch
pub fn delete_local_branch(repo: &Repository, branch_name: &str, force: bool) -> Result<(), GitError> {
    let mut branch = repo.find_branch(branch_name, BranchType::Local)?;

    // Check if it's the current branch
    let current = get_current_branch(repo)?;
    if current == branch_name {
        return Err(GitError::OperationFailed(
            "Cannot delete the currently checked out branch".to_string(),
        ));
    }

    if force {
        branch.delete()?;
    } else {
        // Check if branch is merged before deleting
        let head_commit = repo.head()?.peel_to_commit()?;
        let branch_commit = branch.get().peel_to_commit()?;

        let merge_base = repo.merge_base(head_commit.id(), branch_commit.id())?;
        if merge_base != branch_commit.id() {
            return Err(GitError::OperationFailed(format!(
                "Branch '{}' is not fully merged. Use force to delete anyway.",
                branch_name
            )));
        }

        branch.delete()?;
    }

    Ok(())
}

/// Check if a branch has been merged into another branch
pub fn is_branch_merged(
    repo: &Repository,
    branch_name: &str,
    target_branch: &str,
) -> Result<bool, GitError> {
    let branch = repo.find_branch(branch_name, BranchType::Local)?;
    let target = repo.find_branch(target_branch, BranchType::Local)?;

    let branch_commit = branch.get().peel_to_commit()?;
    let target_commit = target.get().peel_to_commit()?;

    let merge_base = repo.merge_base(target_commit.id(), branch_commit.id())?;

    // If merge base equals branch commit, branch is fully merged into target
    Ok(merge_base == branch_commit.id())
}

/// Get list of local branches
pub fn list_local_branches(repo: &Repository) -> Result<Vec<String>, GitError> {
    let branches = repo.branches(Some(BranchType::Local))?;
    let mut names = Vec::new();

    for branch in branches {
        let (branch, _) = branch?;
        if let Some(name) = branch.name()? {
            names.push(name.to_string());
        }
    }

    Ok(names)
}

/// Get list of remote branches
pub fn list_remote_branches(repo: &Repository, remote: &str) -> Result<Vec<String>, GitError> {
    let branches = repo.branches(Some(BranchType::Remote))?;
    let mut names = Vec::new();
    let prefix = format!("{}/", remote);

    for branch in branches {
        let (branch, _) = branch?;
        if let Some(name) = branch.name()? {
            if name.starts_with(&prefix) {
                names.push(name[prefix.len()..].to_string());
            }
        }
    }

    Ok(names)
}

/// Get commits between current branch and base branch
pub fn get_commits_between(
    repo: &Repository,
    base_branch: &str,
    head_branch: Option<&str>,
) -> Result<Vec<String>, GitError> {
    let head_name = match head_branch {
        Some(name) => name.to_string(),
        None => get_current_branch(repo)?,
    };

    let base_ref = format!("refs/heads/{}", base_branch);
    let head_ref = format!("refs/heads/{}", head_name);

    let base_oid = repo.revparse_single(&base_ref)?.id();
    let head_oid = repo.revparse_single(&head_ref)?.id();

    let mut revwalk = repo.revwalk()?;
    revwalk.push(head_oid)?;
    revwalk.hide(base_oid)?;

    let mut commits = Vec::new();
    for oid in revwalk {
        commits.push(oid?.to_string());
    }

    Ok(commits)
}

/// Check if branch has commits not in base
pub fn has_commits_ahead(repo: &Repository, base_branch: &str) -> Result<bool, GitError> {
    let commits = get_commits_between(repo, base_branch, None)?;
    Ok(!commits.is_empty())
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
    fn test_create_and_checkout_branch() {
        let (temp, repo) = setup_test_repo();

        create_and_checkout_branch(&repo, "feature").unwrap();

        let current = get_current_branch(&repo).unwrap();
        assert_eq!(current, "feature");

        drop(temp);
    }

    #[test]
    fn test_branch_exists() {
        let (temp, repo) = setup_test_repo();

        assert!(!branch_exists(&repo, "feature"));

        create_and_checkout_branch(&repo, "feature").unwrap();
        assert!(branch_exists(&repo, "feature"));

        drop(temp);
    }

    #[test]
    fn test_checkout_branch() {
        let (temp, repo) = setup_test_repo();

        // Create a feature branch
        create_and_checkout_branch(&repo, "feature").unwrap();

        // Go back to main/master
        let default = if branch_exists(&repo, "main") {
            "main"
        } else {
            "master"
        };
        checkout_branch(&repo, default).unwrap();

        let current = get_current_branch(&repo).unwrap();
        assert_eq!(current, default);

        drop(temp);
    }

    #[test]
    fn test_list_local_branches() {
        let (temp, repo) = setup_test_repo();

        create_and_checkout_branch(&repo, "feature1").unwrap();
        create_and_checkout_branch(&repo, "feature2").unwrap();

        let branches = list_local_branches(&repo).unwrap();
        assert!(branches.contains(&"feature1".to_string()));
        assert!(branches.contains(&"feature2".to_string()));

        drop(temp);
    }
}
