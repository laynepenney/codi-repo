//! Criterion benchmarks for comparing with TypeScript version
//!
//! Run with: cargo bench
//! Results are saved in target/criterion/ for comparison

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use gitgrip::core::manifest::{Manifest, RepoConfig};
use gitgrip::core::repo::RepoInfo;
use gitgrip::core::state::StateFile;
use std::path::PathBuf;

/// Benchmark manifest YAML parsing
fn bench_manifest_parse(c: &mut Criterion) {
    let yaml = r#"
version: 1
manifest:
  url: git@github.com:user/manifest.git
  default_branch: main
repos:
  app:
    url: git@github.com:user/app.git
    path: app
    default_branch: main
    copyfile:
      - src: README.md
        dest: APP_README.md
    linkfile:
      - src: config.yaml
        dest: app-config.yaml
  lib:
    url: git@github.com:user/lib.git
    path: lib
    default_branch: main
  api:
    url: git@github.com:user/api.git
    path: api
    default_branch: main
settings:
  pr_prefix: "[multi-repo]"
  merge_strategy: all-or-nothing
workspace:
  env:
    NODE_ENV: development
  scripts:
    build:
      description: Build all packages
      command: npm run build
    test:
      description: Run tests
      steps:
        - name: lint
          command: npm run lint
        - name: test
          command: npm test
"#;

    c.bench_function("manifest_parse", |b| {
        b.iter(|| Manifest::parse(black_box(yaml)).unwrap())
    });
}

/// Benchmark state JSON parsing
fn bench_state_parse(c: &mut Criterion) {
    let json = r#"{
        "currentManifestPr": 42,
        "branchToPr": {
            "feat/new-feature": 42,
            "feat/another": 43,
            "fix/bug": 44
        },
        "prLinks": {
            "42": [
                {
                    "repoName": "app",
                    "owner": "user",
                    "repo": "app",
                    "number": 123,
                    "url": "https://github.com/user/app/pull/123",
                    "state": "open",
                    "approved": true,
                    "checksPass": true,
                    "mergeable": true
                },
                {
                    "repoName": "lib",
                    "owner": "user",
                    "repo": "lib",
                    "number": 456,
                    "url": "https://github.com/user/lib/pull/456",
                    "state": "open",
                    "approved": false,
                    "checksPass": true,
                    "mergeable": true
                }
            ],
            "43": [],
            "44": []
        }
    }"#;

    c.bench_function("state_parse", |b| {
        b.iter(|| StateFile::parse(black_box(json)).unwrap())
    });
}

/// Benchmark git URL parsing
fn bench_url_parse(c: &mut Criterion) {
    let config = RepoConfig {
        url: "git@github.com:organization/repository-name.git".to_string(),
        path: "packages/repository-name".to_string(),
        default_branch: "main".to_string(),
        copyfile: None,
        linkfile: None,
        platform: None,
    };
    let workspace = PathBuf::from("/home/user/workspace");

    c.bench_function("url_parse_github_ssh", |b| {
        b.iter(|| RepoInfo::from_config("repo", black_box(&config), black_box(&workspace)))
    });
}

/// Benchmark Azure DevOps URL parsing
fn bench_url_parse_azure(c: &mut Criterion) {
    let config = RepoConfig {
        url: "https://dev.azure.com/organization/project/_git/repository".to_string(),
        path: "repository".to_string(),
        default_branch: "main".to_string(),
        copyfile: None,
        linkfile: None,
        platform: None,
    };
    let workspace = PathBuf::from("/home/user/workspace");

    c.bench_function("url_parse_azure_https", |b| {
        b.iter(|| RepoInfo::from_config("repo", black_box(&config), black_box(&workspace)))
    });
}

/// Benchmark manifest validation
fn bench_manifest_validate(c: &mut Criterion) {
    let yaml = r#"
version: 1
repos:
  app:
    url: git@github.com:user/app.git
    path: app
    copyfile:
      - src: file1.txt
        dest: dest1.txt
      - src: file2.txt
        dest: dest2.txt
    linkfile:
      - src: link1
        dest: dest/link1
workspace:
  scripts:
    build:
      steps:
        - name: step1
          command: echo 1
        - name: step2
          command: echo 2
        - name: step3
          command: echo 3
"#;

    // Parse once, then benchmark validation
    let manifest: Manifest = serde_yaml::from_str(yaml).unwrap();

    c.bench_function("manifest_validate", |b| {
        b.iter(|| black_box(&manifest).validate().unwrap())
    });
}

/// Benchmark git status on a repo (requires a real git repo)
fn bench_git_status(c: &mut Criterion) {
    use git2::Repository;
    use gitgrip::git::status::get_status_info;
    use tempfile::TempDir;
    use std::fs;

    // Set up a test repo
    let temp = TempDir::new().unwrap();
    let repo = Repository::init(temp.path()).unwrap();

    // Configure and create initial commit
    {
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Bench User").unwrap();
        config.set_str("user.email", "bench@example.com").unwrap();
    }
    {
        fs::write(temp.path().join("README.md"), "# Benchmark Repo").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("README.md")).unwrap();
        index.write().unwrap();
        let sig = repo.signature().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[]).unwrap();
    }

    // Add some files to make the status more realistic
    for i in 0..10 {
        fs::write(temp.path().join(format!("file{}.txt", i)), format!("Content {}", i)).unwrap();
    }

    c.bench_function("git_status", |b| {
        b.iter(|| get_status_info(black_box(&repo)).unwrap())
    });
}

/// Benchmark git branch listing
fn bench_git_list_branches(c: &mut Criterion) {
    use git2::Repository;
    use tempfile::TempDir;
    use std::fs;

    // Set up a test repo with multiple branches
    let temp = TempDir::new().unwrap();
    let repo = Repository::init(temp.path()).unwrap();

    // Configure and create initial commit
    {
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Bench User").unwrap();
        config.set_str("user.email", "bench@example.com").unwrap();
    }
    {
        fs::write(temp.path().join("README.md"), "# Benchmark Repo").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("README.md")).unwrap();
        index.write().unwrap();
        let sig = repo.signature().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[]).unwrap();
    }

    // Create several branches
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    for i in 0..10 {
        repo.branch(&format!("branch-{}", i), &head, false).unwrap();
    }

    c.bench_function("git_list_branches", |b| {
        b.iter(|| {
            let branches: Vec<_> = repo.branches(Some(git2::BranchType::Local))
                .unwrap()
                .collect();
            black_box(branches.len())
        })
    });
}

/// Benchmark file hashing (useful for change detection)
fn bench_file_hash(c: &mut Criterion) {
    use std::hash::{Hash, Hasher};
    use std::collections::hash_map::DefaultHasher;

    let content = "This is some test content for hashing\n".repeat(100);

    c.bench_function("file_hash_content", |b| {
        b.iter(|| {
            let mut hasher = DefaultHasher::new();
            black_box(&content).hash(&mut hasher);
            hasher.finish()
        })
    });
}

/// Benchmark path operations (common in file linking)
fn bench_path_operations(c: &mut Criterion) {
    use std::path::PathBuf;

    let workspace = PathBuf::from("/home/user/workspace");
    let repo_path = "packages/my-awesome-repo";

    c.bench_function("path_join", |b| {
        b.iter(|| {
            let full = workspace.join(black_box(repo_path));
            black_box(full)
        })
    });

    let full_path = workspace.join(repo_path);
    c.bench_function("path_canonicalize_relative", |b| {
        b.iter(|| {
            // This simulates normalizing relative paths
            let path = black_box(&full_path);
            path.components().collect::<Vec<_>>()
        })
    });
}

/// Benchmark regex URL parsing (for platform detection)
fn bench_url_regex_parse(c: &mut Criterion) {
    use regex::Regex;

    let github_regex = Regex::new(r"github\.com[:/]([^/]+)/([^/\.]+)").unwrap();
    let gitlab_regex = Regex::new(r"gitlab\.com[:/](.+)/([^/\.]+)").unwrap();

    let url = "git@github.com:organization/repository-name.git";

    c.bench_function("url_regex_github", |b| {
        b.iter(|| {
            github_regex.captures(black_box(url))
        })
    });

    let gitlab_url = "git@gitlab.com:group/subgroup/repo.git";
    c.bench_function("url_regex_gitlab", |b| {
        b.iter(|| {
            gitlab_regex.captures(black_box(gitlab_url))
        })
    });
}

criterion_group!(
    benches,
    bench_manifest_parse,
    bench_state_parse,
    bench_url_parse,
    bench_url_parse_azure,
    bench_manifest_validate,
    bench_git_status,
    bench_git_list_branches,
    bench_file_hash,
    bench_path_operations,
    bench_url_regex_parse,
);

criterion_main!(benches);
