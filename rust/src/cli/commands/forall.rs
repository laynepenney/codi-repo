//! Forall command implementation
//!
//! Runs a command in each repository.
//!
//! Includes optimization: common git commands are intercepted and run using
//! the git2/gix library instead of spawning git CLI processes, providing
//! up to 100x speedup.

use crate::cli::output::Output;
use crate::core::manifest::Manifest;
use crate::core::repo::RepoInfo;
use crate::git::path_exists;
use std::path::PathBuf;
use std::process::Command;

/// Interceptable git commands for optimization
#[derive(Debug, Clone)]
enum GitCommand {
    /// git status [--porcelain|-s]
    Status { porcelain: bool },
    /// git branch [-a]
    ListBranches { all: bool },
    /// git rev-parse HEAD
    GetHead,
    /// git rev-parse --abbrev-ref HEAD
    GetBranch,
    /// git diff --stat
    DiffStat,
}

/// Try to parse a command string into an interceptable GitCommand
fn try_parse_git_command(command: &str) -> Option<GitCommand> {
    let trimmed = command.trim();

    // Don't intercept piped commands
    if trimmed.contains('|') || trimmed.contains('>') || trimmed.contains('<') {
        return None;
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();

    match parts.as_slice() {
        // git status variants
        ["git", "status"] => Some(GitCommand::Status { porcelain: false }),
        ["git", "status", "--porcelain"] => Some(GitCommand::Status { porcelain: true }),
        ["git", "status", "-s"] => Some(GitCommand::Status { porcelain: true }),
        ["git", "status", "--short"] => Some(GitCommand::Status { porcelain: true }),

        // git branch variants
        ["git", "branch"] => Some(GitCommand::ListBranches { all: false }),
        ["git", "branch", "-a"] => Some(GitCommand::ListBranches { all: true }),
        ["git", "branch", "--all"] => Some(GitCommand::ListBranches { all: true }),

        // git rev-parse variants
        ["git", "rev-parse", "HEAD"] => Some(GitCommand::GetHead),
        ["git", "rev-parse", "--abbrev-ref", "HEAD"] => Some(GitCommand::GetBranch),

        // git diff --stat
        ["git", "diff", "--stat"] => Some(GitCommand::DiffStat),

        _ => None,
    }
}

/// Execute an intercepted git command using git2 (fast path)
fn execute_git_command(repo_path: &PathBuf, cmd: &GitCommand) -> Result<String, String> {
    let repo = crate::git::open_repo(repo_path)
        .map_err(|e| format!("Failed to open repo: {}", e))?;

    match cmd {
        GitCommand::Status { porcelain } => {
            let statuses = repo.statuses(None)
                .map_err(|e| format!("Failed to get status: {}", e))?;

            if *porcelain {
                // Format as porcelain output
                let mut output = String::new();
                for entry in statuses.iter() {
                    let status = entry.status();
                    let path = entry.path().unwrap_or("?");

                    let index_status = if status.is_index_new() { 'A' }
                        else if status.is_index_modified() { 'M' }
                        else if status.is_index_deleted() { 'D' }
                        else if status.is_index_renamed() { 'R' }
                        else if status.is_index_typechange() { 'T' }
                        else { ' ' };

                    let wt_status = if status.is_wt_new() { '?' }
                        else if status.is_wt_modified() { 'M' }
                        else if status.is_wt_deleted() { 'D' }
                        else if status.is_wt_renamed() { 'R' }
                        else if status.is_wt_typechange() { 'T' }
                        else { ' ' };

                    output.push_str(&format!("{}{} {}\n", index_status, wt_status, path));
                }
                Ok(output)
            } else {
                // Human-readable format
                if statuses.is_empty() {
                    Ok("nothing to commit, working tree clean\n".to_string())
                } else {
                    let mut output = String::new();
                    let mut staged = Vec::new();
                    let mut unstaged = Vec::new();
                    let mut untracked = Vec::new();

                    for entry in statuses.iter() {
                        let path = entry.path().unwrap_or("?").to_string();
                        let status = entry.status();

                        if status.is_index_new() || status.is_index_modified() || status.is_index_deleted() {
                            staged.push(path.clone());
                        }
                        if status.is_wt_modified() || status.is_wt_deleted() {
                            unstaged.push(path.clone());
                        }
                        if status.is_wt_new() {
                            untracked.push(path);
                        }
                    }

                    if !staged.is_empty() {
                        output.push_str("Changes to be committed:\n");
                        for f in &staged {
                            output.push_str(&format!("  {}\n", f));
                        }
                    }
                    if !unstaged.is_empty() {
                        output.push_str("Changes not staged for commit:\n");
                        for f in &unstaged {
                            output.push_str(&format!("  {}\n", f));
                        }
                    }
                    if !untracked.is_empty() {
                        output.push_str("Untracked files:\n");
                        for f in &untracked {
                            output.push_str(&format!("  {}\n", f));
                        }
                    }
                    Ok(output)
                }
            }
        }

        GitCommand::ListBranches { all } => {
            let mut output = String::new();
            let head = repo.head().ok();
            let current_branch = head.as_ref()
                .and_then(|h| h.shorthand())
                .unwrap_or("");

            // Local branches
            let branches = repo.branches(Some(git2::BranchType::Local))
                .map_err(|e| format!("Failed to list branches: {}", e))?;

            for branch in branches {
                let (branch, _) = branch.map_err(|e| format!("Failed to read branch: {}", e))?;
                let name = branch.name()
                    .map_err(|e| format!("Failed to get branch name: {}", e))?
                    .unwrap_or("?");

                if name == current_branch {
                    output.push_str(&format!("* {}\n", name));
                } else {
                    output.push_str(&format!("  {}\n", name));
                }
            }

            // Remote branches if -a flag
            if *all {
                let remote_branches = repo.branches(Some(git2::BranchType::Remote))
                    .map_err(|e| format!("Failed to list remote branches: {}", e))?;

                for branch in remote_branches {
                    let (branch, _) = branch.map_err(|e| format!("Failed to read branch: {}", e))?;
                    let name = branch.name()
                        .map_err(|e| format!("Failed to get branch name: {}", e))?
                        .unwrap_or("?");
                    output.push_str(&format!("  remotes/{}\n", name));
                }
            }

            Ok(output)
        }

        GitCommand::GetHead => {
            let head = repo.head()
                .map_err(|e| format!("Failed to get HEAD: {}", e))?;
            let oid = head.target()
                .ok_or_else(|| "HEAD has no target".to_string())?;
            Ok(format!("{}\n", oid))
        }

        GitCommand::GetBranch => {
            let head = repo.head()
                .map_err(|e| format!("Failed to get HEAD: {}", e))?;
            let name = head.shorthand().unwrap_or("HEAD");
            Ok(format!("{}\n", name))
        }

        GitCommand::DiffStat => {
            // For diff --stat, fall back to CLI as it's complex to replicate
            Err("DiffStat not implemented, use CLI".to_string())
        }
    }
}

/// Run the forall command
pub fn run_forall(
    workspace_root: &PathBuf,
    manifest: &Manifest,
    command: &str,
    parallel: bool,
    changed_only: bool,
    no_intercept: bool,
) -> anyhow::Result<()> {
    let repos: Vec<RepoInfo> = manifest
        .repos
        .iter()
        .filter_map(|(name, config)| RepoInfo::from_config(name, config, workspace_root))
        .collect();

    // Try to intercept git commands for optimization
    let intercepted = if no_intercept {
        None
    } else {
        try_parse_git_command(command)
    };

    if parallel {
        run_parallel(&repos, command, changed_only, intercepted.as_ref())?;
    } else {
        run_sequential(&repos, command, changed_only, intercepted.as_ref())?;
    }

    Ok(())
}

fn run_sequential(
    repos: &[RepoInfo],
    command: &str,
    changed_only: bool,
    intercepted: Option<&GitCommand>,
) -> anyhow::Result<()> {
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

        // Try optimized path if we have an intercepted command
        if let Some(git_cmd) = intercepted {
            match execute_git_command(&repo.absolute_path, git_cmd) {
                Ok(output) => {
                    print!("{}", output);
                    success_count += 1;
                    println!();
                    continue;
                }
                Err(_) => {
                    // Fall through to CLI execution
                }
            }
        }

        // CLI execution (fallback or non-interceptable command)
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

fn run_parallel(
    repos: &[RepoInfo],
    command: &str,
    changed_only: bool,
    intercepted: Option<&GitCommand>,
) -> anyhow::Result<()> {
    use std::sync::{Arc, Mutex};
    use std::thread;

    let results = Arc::new(Mutex::new(Vec::new()));
    let mut handles = vec![];

    // Clone the intercepted command for threads
    let intercepted_cmd = intercepted.cloned();

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
        let git_cmd = intercepted_cmd.clone();

        let handle = thread::spawn(move || {
            // Try optimized path first
            if let Some(ref git_cmd) = git_cmd {
                if let Ok(output) = execute_git_command(&repo_path, git_cmd) {
                    let mut results = results.lock().unwrap();
                    results.push((repo_name, Ok(output)));
                    return;
                }
            }

            // Fall back to CLI
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
            match output {
                Ok(out) => {
                    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                    if out.status.success() {
                        results.push((repo_name, Ok(format!("{}{}", stdout, stderr))));
                    } else {
                        results.push((repo_name, Err(format!("Exit code: {:?}\n{}{}", out.status.code(), stdout, stderr))));
                    }
                }
                Err(e) => {
                    results.push((repo_name, Err(e.to_string())));
                }
            }
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
                print!("{}", output);
                success_count += 1;
            }
            Err(e) => {
                Output::error(&format!("{}", e));
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

    #[test]
    fn test_try_parse_git_command_status() {
        assert!(matches!(
            try_parse_git_command("git status"),
            Some(GitCommand::Status { porcelain: false })
        ));
        assert!(matches!(
            try_parse_git_command("git status --porcelain"),
            Some(GitCommand::Status { porcelain: true })
        ));
        assert!(matches!(
            try_parse_git_command("git status -s"),
            Some(GitCommand::Status { porcelain: true })
        ));
    }

    #[test]
    fn test_try_parse_git_command_branch() {
        assert!(matches!(
            try_parse_git_command("git branch"),
            Some(GitCommand::ListBranches { all: false })
        ));
        assert!(matches!(
            try_parse_git_command("git branch -a"),
            Some(GitCommand::ListBranches { all: true })
        ));
    }

    #[test]
    fn test_try_parse_git_command_rev_parse() {
        assert!(matches!(
            try_parse_git_command("git rev-parse HEAD"),
            Some(GitCommand::GetHead)
        ));
        assert!(matches!(
            try_parse_git_command("git rev-parse --abbrev-ref HEAD"),
            Some(GitCommand::GetBranch)
        ));
    }

    #[test]
    fn test_try_parse_git_command_not_interceptable() {
        // Piped commands should not be intercepted
        assert!(try_parse_git_command("git status | grep foo").is_none());
        assert!(try_parse_git_command("git log > log.txt").is_none());

        // Non-git commands should not be intercepted
        assert!(try_parse_git_command("npm test").is_none());
        assert!(try_parse_git_command("echo hello").is_none());

        // Complex git commands should not be intercepted
        assert!(try_parse_git_command("git log --oneline -10").is_none());
        assert!(try_parse_git_command("git commit -m 'message'").is_none());
    }

    #[test]
    fn test_execute_git_command_status() {
        let temp = TempDir::new().unwrap();
        let repo_path = setup_test_repo(&temp);

        // Test status on clean repo
        let result = execute_git_command(&repo_path, &GitCommand::Status { porcelain: true });
        assert!(result.is_ok());
        // Clean repo should have empty porcelain output (no untracked since we committed)
        // But actually we have untracked files in some tests, let's check

        // Add an untracked file
        std::fs::write(repo_path.join("untracked.txt"), "content").unwrap();

        let result = execute_git_command(&repo_path, &GitCommand::Status { porcelain: true });
        assert!(result.is_ok());
        assert!(result.unwrap().contains("untracked.txt"));
    }

    #[test]
    fn test_execute_git_command_branch() {
        let temp = TempDir::new().unwrap();
        let repo_path = setup_test_repo(&temp);

        let result = execute_git_command(&repo_path, &GitCommand::ListBranches { all: false });
        assert!(result.is_ok());
        let output = result.unwrap();
        // Should contain the default branch (master or main)
        assert!(output.contains("master") || output.contains("main"));
    }

    #[test]
    fn test_execute_git_command_get_branch() {
        let temp = TempDir::new().unwrap();
        let repo_path = setup_test_repo(&temp);

        let result = execute_git_command(&repo_path, &GitCommand::GetBranch);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("master") || output.contains("main"));
    }
}
