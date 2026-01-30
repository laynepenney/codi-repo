//! Forall command implementation
//!
//! Runs a command in each repository.

use crate::cli::output::Output;
use crate::core::manifest::Manifest;
use crate::core::repo::RepoInfo;
use crate::git::path_exists;
use std::path::PathBuf;
use std::process::Command;

/// Run the forall command
pub fn run_forall(
    workspace_root: &PathBuf,
    manifest: &Manifest,
    command: &str,
    parallel: bool,
    changed_only: bool,
) -> anyhow::Result<()> {
    let repos: Vec<RepoInfo> = manifest
        .repos
        .iter()
        .filter_map(|(name, config)| RepoInfo::from_config(name, config, workspace_root))
        .collect();

    if parallel {
        run_parallel(&repos, command, changed_only)?;
    } else {
        run_sequential(&repos, command, changed_only)?;
    }

    Ok(())
}

fn run_sequential(repos: &[RepoInfo], command: &str, changed_only: bool) -> anyhow::Result<()> {
    let mut success_count = 0;
    let mut error_count = 0;
    let mut skip_count = 0;

    for repo in repos {
        if !path_exists(&repo.absolute_path) {
            Output::warning(&format!("{}: not cloned, skipping", repo.name));
            skip_count += 1;
            continue;
        }

        // Check if repo has changes (if changed_only flag is set)
        if changed_only && !has_changes(&repo.absolute_path)? {
            skip_count += 1;
            continue;
        }

        Output::header(&format!("{}:", repo.name));

        let output = Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(&repo.absolute_path)
            .env("REPO_NAME", &repo.name)
            .env("REPO_PATH", &repo.absolute_path)
            .env("REPO_URL", &repo.url)
            .env("REPO_BRANCH", &repo.default_branch)
            .output()?;

        if output.status.success() {
            print!("{}", String::from_utf8_lossy(&output.stdout));
            if !output.stderr.is_empty() {
                eprint!("{}", String::from_utf8_lossy(&output.stderr));
            }
            success_count += 1;
        } else {
            print!("{}", String::from_utf8_lossy(&output.stdout));
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
            Output::error(&format!("Command failed with exit code: {:?}", output.status.code()));
            error_count += 1;
        }
        println!();
    }

    // Summary
    if error_count == 0 {
        Output::success(&format!(
            "Command completed in {} repo(s){}",
            success_count,
            if skip_count > 0 { format!(", {} skipped", skip_count) } else { String::new() }
        ));
    } else {
        Output::warning(&format!(
            "{} succeeded, {} failed, {} skipped",
            success_count, error_count, skip_count
        ));
    }

    Ok(())
}

fn run_parallel(repos: &[RepoInfo], command: &str, changed_only: bool) -> anyhow::Result<()> {
    use std::sync::{Arc, Mutex};
    use std::thread;

    let results = Arc::new(Mutex::new(Vec::new()));
    let mut handles = vec![];

    for repo in repos {
        if !path_exists(&repo.absolute_path) {
            continue;
        }

        if changed_only && !has_changes(&repo.absolute_path).unwrap_or(false) {
            continue;
        }

        let repo_name = repo.name.clone();
        let repo_path = repo.absolute_path.clone();
        let repo_url = repo.url.clone();
        let repo_branch = repo.default_branch.clone();
        let cmd = command.to_string();
        let results = Arc::clone(&results);

        let handle = thread::spawn(move || {
            let output = Command::new("sh")
                .arg("-c")
                .arg(&cmd)
                .current_dir(&repo_path)
                .env("REPO_NAME", &repo_name)
                .env("REPO_PATH", &repo_path)
                .env("REPO_URL", &repo_url)
                .env("REPO_BRANCH", &repo_branch)
                .output();

            let mut results = results.lock().unwrap();
            results.push((repo_name, output));
        });

        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // Print results
    let results = results.lock().unwrap();
    let mut success_count = 0;
    let mut error_count = 0;

    for (repo_name, output) in results.iter() {
        Output::header(&format!("{}:", repo_name));
        match output {
            Ok(output) => {
                print!("{}", String::from_utf8_lossy(&output.stdout));
                if !output.stderr.is_empty() {
                    eprint!("{}", String::from_utf8_lossy(&output.stderr));
                }
                if output.status.success() {
                    success_count += 1;
                } else {
                    error_count += 1;
                }
            }
            Err(e) => {
                Output::error(&format!("Failed to run command: {}", e));
                error_count += 1;
            }
        }
        println!();
    }

    if error_count == 0 {
        Output::success(&format!("Command completed in {} repo(s)", success_count));
    } else {
        Output::warning(&format!("{} succeeded, {} failed", success_count, error_count));
    }

    Ok(())
}

/// Check if a repository has uncommitted changes
fn has_changes(repo_path: &PathBuf) -> anyhow::Result<bool> {
    match crate::git::open_repo(repo_path) {
        Ok(repo) => {
            let statuses = repo.statuses(None)?;
            Ok(!statuses.is_empty())
        }
        Err(_) => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Repository;
    use tempfile::TempDir;

    fn setup_test_repo(temp: &TempDir) -> PathBuf {
        let repo_path = temp.path().join("repo");
        std::fs::create_dir_all(&repo_path).unwrap();
        let repo = Repository::init(&repo_path).unwrap();

        // Configure git
        {
            let mut config = repo.config().unwrap();
            config.set_str("user.name", "Test User").unwrap();
            config.set_str("user.email", "test@example.com").unwrap();
        }

        // Create initial commit
        {
            std::fs::write(repo_path.join("README.md"), "# Test").unwrap();
            let mut index = repo.index().unwrap();
            index.add_path(std::path::Path::new("README.md")).unwrap();
            index.write().unwrap();
            let sig = repo.signature().unwrap();
            let tree_id = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[]).unwrap();
        }

        repo_path
    }

    #[test]
    fn test_has_changes_clean_repo() {
        let temp = TempDir::new().unwrap();
        let repo_path = setup_test_repo(&temp);

        let result = has_changes(&repo_path);
        assert!(result.is_ok());
        assert!(!result.unwrap()); // Clean repo has no changes
    }

    #[test]
    fn test_has_changes_with_modifications() {
        let temp = TempDir::new().unwrap();
        let repo_path = setup_test_repo(&temp);

        // Modify a tracked file
        std::fs::write(repo_path.join("README.md"), "# Modified").unwrap();

        let result = has_changes(&repo_path);
        assert!(result.is_ok());
        assert!(result.unwrap()); // Has modifications
    }

    #[test]
    fn test_has_changes_with_untracked_file() {
        let temp = TempDir::new().unwrap();
        let repo_path = setup_test_repo(&temp);

        // Add untracked file
        std::fs::write(repo_path.join("new-file.txt"), "content").unwrap();

        let result = has_changes(&repo_path);
        assert!(result.is_ok());
        assert!(result.unwrap()); // Has untracked file
    }

    #[test]
    fn test_has_changes_nonexistent_repo() {
        let path = PathBuf::from("/nonexistent/path");
        let result = has_changes(&path);
        assert!(result.is_ok());
        assert!(!result.unwrap()); // Returns false for non-repo
    }
}
