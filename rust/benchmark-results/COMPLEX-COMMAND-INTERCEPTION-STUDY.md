# Complex Git Command Interception Study

## Overview

This study analyzes what it would take to intercept additional git commands beyond the basic ones already implemented (status, branch, rev-parse).

## Currently Intercepted (Simple)

| Command | Difficulty | Implementation |
|---------|------------|----------------|
| `git status` | Easy | `repo.statuses(None)` |
| `git branch` | Easy | `repo.branches()` |
| `git rev-parse HEAD` | Easy | `head.target()` |

## Candidates for Interception

### 1. `git log` Variants

#### `git log --oneline -n 10`

**Difficulty**: Medium

**git2 Implementation**:
```rust
fn git_log_oneline(repo: &Repository, count: usize) -> Result<String, Error> {
    let mut revwalk = repo.revwalk()?;
    revwalk.push_head()?;

    let mut output = String::new();
    for (i, oid) in revwalk.enumerate() {
        if i >= count { break; }
        let oid = oid?;
        let commit = repo.find_commit(oid)?;
        let short_id = &oid.to_string()[..7];
        let message = commit.summary().unwrap_or("");
        output.push_str(&format!("{} {}\n", short_id, message));
    }
    Ok(output)
}
```

**Challenges**:
- Many log flags: `--oneline`, `--graph`, `--decorate`, `--all`, `--author`, `--since`, `--until`, `--grep`
- Graph rendering is complex (ASCII art for `--graph`)
- Date formatting varies by locale/config

**Recommendation**: Intercept simple cases (`--oneline -n N`), fall back for complex flags.

---

#### `git log --format="%H %s"`

**Difficulty**: Hard

**Challenges**:
- Format string parsing: `%H`, `%h`, `%s`, `%b`, `%an`, `%ae`, `%ad`, `%cn`, `%ce`, `%cd`, etc.
- 40+ format specifiers
- Conditional formatting: `%C(red)`, `%C(reset)`

**Recommendation**: Don't intercept. Too many edge cases.

---

### 2. `git diff` Variants

#### `git diff` (unstaged changes)

**Difficulty**: Medium-Hard

**git2 Implementation**:
```rust
fn git_diff_unstaged(repo: &Repository) -> Result<String, Error> {
    let head = repo.head()?.peel_to_tree()?;
    let diff = repo.diff_tree_to_workdir(Some(&head), None)?;

    let mut output = String::new();
    diff.print(git2::DiffFormat::Patch, |delta, hunk, line| {
        // Format each line...
        true
    })?;
    Ok(output)
}
```

**Challenges**:
- Many diff options: `--stat`, `--numstat`, `--shortstat`, `--name-only`, `--name-status`
- Color output
- Context lines (`-U3`, `-U10`)
- Word diff (`--word-diff`)
- Binary file handling

**Recommendation**: Intercept `--stat` and `--name-only`, fall back for patch output.

---

#### `git diff --stat`

**Difficulty**: Medium

**git2 Implementation**:
```rust
fn git_diff_stat(repo: &Repository) -> Result<String, Error> {
    let diff = repo.diff_index_to_workdir(None, None)?;
    let stats = diff.stats()?;

    let mut output = String::new();
    // Format stats similar to git output
    for delta in diff.deltas() {
        let path = delta.new_file().path().unwrap_or(Path::new(""));
        // Calculate insertions/deletions...
    }
    output.push_str(&format!(
        " {} files changed, {} insertions(+), {} deletions(-)\n",
        stats.files_changed(),
        stats.insertions(),
        stats.deletions()
    ));
    Ok(output)
}
```

**Recommendation**: Good candidate for interception.

---

### 3. `git show` Variants

#### `git show HEAD`

**Difficulty**: Medium

**git2 Implementation**:
```rust
fn git_show_commit(repo: &Repository, rev: &str) -> Result<String, Error> {
    let obj = repo.revparse_single(rev)?;
    let commit = obj.peel_to_commit()?;

    let mut output = String::new();
    output.push_str(&format!("commit {}\n", commit.id()));
    output.push_str(&format!("Author: {} <{}>\n",
        commit.author().name().unwrap_or(""),
        commit.author().email().unwrap_or("")
    ));
    output.push_str(&format!("Date:   {}\n\n", /* format date */));
    output.push_str(&format!("    {}\n", commit.message().unwrap_or("")));

    // Add diff...
    Ok(output)
}
```

**Challenges**:
- Diff generation (same as `git diff`)
- Various output formats
- Showing tags, blobs, trees (not just commits)

**Recommendation**: Intercept simple `git show HEAD` or `git show <sha>`, fall back for others.

---

### 4. `git stash` Variants

#### `git stash list`

**Difficulty**: Easy

**git2 Implementation**:
```rust
fn git_stash_list(repo: &Repository) -> Result<String, Error> {
    let mut output = String::new();
    repo.stash_foreach(|index, message, oid| {
        output.push_str(&format!("stash@{{{}}}: {}\n", index, message));
        true
    })?;
    Ok(output)
}
```

**Recommendation**: Good candidate - simple and common.

---

#### `git stash show`

**Difficulty**: Medium

Same challenges as `git diff`.

**Recommendation**: Intercept with `--stat` only.

---

### 5. `git remote -v`

**Difficulty**: Easy

**git2 Implementation**:
```rust
fn git_remote_list(repo: &Repository) -> Result<String, Error> {
    let mut output = String::new();
    for name in repo.remotes()?.iter() {
        let name = name.unwrap_or("");
        if let Ok(remote) = repo.find_remote(name) {
            if let Some(url) = remote.url() {
                output.push_str(&format!("{}\t{} (fetch)\n", name, url));
            }
            if let Some(url) = remote.pushurl().or(remote.url()) {
                output.push_str(&format!("{}\t{} (push)\n", name, url));
            }
        }
    }
    Ok(output)
}
```

**Recommendation**: Good candidate - simple and common.

---

### 6. `git tag`

#### `git tag` (list tags)

**Difficulty**: Easy

**git2 Implementation**:
```rust
fn git_tag_list(repo: &Repository) -> Result<String, Error> {
    let mut output = String::new();
    repo.tag_foreach(|oid, name| {
        let name = String::from_utf8_lossy(name);
        let name = name.strip_prefix("refs/tags/").unwrap_or(&name);
        output.push_str(&format!("{}\n", name));
        true
    })?;
    Ok(output)
}
```

**Recommendation**: Good candidate.

---

### 7. Piped Commands

#### `git status | grep modified`

**Difficulty**: Very Hard (Don't intercept)

**Why**:
- Would need to implement shell pipe semantics
- User expects exact git output format
- Regex matching on our output might differ

**Recommendation**: Never intercept piped commands.

---

## Implementation Priority Matrix

| Command | Difficulty | Frequency | Speedup | Priority |
|---------|------------|-----------|---------|----------|
| `git log --oneline -N` | Medium | High | ~50x | **High** |
| `git diff --stat` | Medium | Medium | ~30x | **High** |
| `git diff --name-only` | Easy | Medium | ~30x | **High** |
| `git stash list` | Easy | Medium | ~50x | **Medium** |
| `git remote -v` | Easy | Low | ~50x | **Medium** |
| `git tag` | Easy | Low | ~50x | **Low** |
| `git show HEAD` | Medium | Low | ~30x | **Low** |
| `git log --format=...` | Hard | Low | ~50x | **Skip** |
| `git diff` (patch) | Hard | Medium | ~20x | **Skip** |
| Piped commands | N/A | Medium | N/A | **Never** |

---

## Recommended Phase 2 Additions

### High Priority (Add Next)

```rust
// 1. git log --oneline -N
["git", "log", "--oneline"] => Some(GitCommand::LogOneline { count: 10 }),
["git", "log", "--oneline", "-n", n] => Some(GitCommand::LogOneline { count: n.parse()? }),
["git", "log", "--oneline", n] if n.starts_with('-') => {
    Some(GitCommand::LogOneline { count: n[1..].parse()? })
},

// 2. git diff --stat
["git", "diff", "--stat"] => Some(GitCommand::DiffStat),

// 3. git diff --name-only
["git", "diff", "--name-only"] => Some(GitCommand::DiffNameOnly),
["git", "diff", "--name-status"] => Some(GitCommand::DiffNameStatus),
```

### Medium Priority (Add Later)

```rust
// 4. git stash list
["git", "stash", "list"] => Some(GitCommand::StashList),

// 5. git remote -v
["git", "remote", "-v"] => Some(GitCommand::RemoteList),
```

---

## Estimated Implementation Effort

| Command | Lines of Code | Test Cases | Time Estimate |
|---------|---------------|------------|---------------|
| `git log --oneline` | ~40 | 5 | 2 hours |
| `git diff --stat` | ~60 | 5 | 3 hours |
| `git diff --name-only` | ~30 | 3 | 1 hour |
| `git stash list` | ~20 | 3 | 1 hour |
| `git remote -v` | ~25 | 3 | 1 hour |
| **Total Phase 2** | ~175 | 19 | **8 hours** |

---

## gix Considerations

For commands where gix has better APIs:

| Command | git2 | gix | Use |
|---------|------|-----|-----|
| `git log` | revwalk | `repo.rev_walk()` | Either |
| `git diff` | diff_* | Limited | git2 |
| `git stash` | stash_foreach | Limited | git2 |
| `git tag` | tag_foreach | `repo.references()` | gix faster |
| `git remote` | remotes() | `repo.remote_names()` | Either |

---

---

## Full Git Command Coverage Analysis

### Read-Only Commands (Safe to Intercept)

| Command | git2 Support | gix Support | Difficulty | Recommendation |
|---------|--------------|-------------|------------|----------------|
| `git status` | ✅ Full | ⚠️ Partial | Easy | **Intercept** |
| `git branch` | ✅ Full | ✅ Full | Easy | **Intercept** |
| `git branch -a` | ✅ Full | ✅ Full | Easy | **Intercept** |
| `git branch -r` | ✅ Full | ✅ Full | Easy | **Intercept** |
| `git log` | ✅ Full | ✅ Full | Medium | **Intercept simple** |
| `git log --oneline` | ✅ Full | ✅ Full | Easy | **Intercept** |
| `git log -n N` | ✅ Full | ✅ Full | Easy | **Intercept** |
| `git log --graph` | ❌ Manual | ❌ Manual | Hard | Skip |
| `git diff` | ✅ Full | ⚠️ Partial | Medium | **Intercept --stat** |
| `git diff --staged` | ✅ Full | ⚠️ Partial | Medium | **Intercept --stat** |
| `git diff --name-only` | ✅ Full | ⚠️ Partial | Easy | **Intercept** |
| `git show` | ✅ Full | ✅ Full | Medium | **Intercept simple** |
| `git rev-parse` | ✅ Full | ✅ Full | Easy | **Intercept** |
| `git rev-list` | ✅ Full | ✅ Full | Medium | Intercept |
| `git cat-file` | ✅ Full | ✅ Full | Easy | Intercept |
| `git ls-files` | ✅ Full | ✅ Full | Easy | **Intercept** |
| `git ls-tree` | ✅ Full | ✅ Full | Easy | Intercept |
| `git tag` | ✅ Full | ✅ Full | Easy | **Intercept** |
| `git tag -l` | ✅ Full | ✅ Full | Easy | **Intercept** |
| `git remote` | ✅ Full | ✅ Full | Easy | **Intercept** |
| `git remote -v` | ✅ Full | ✅ Full | Easy | **Intercept** |
| `git stash list` | ✅ Full | ❌ None | Easy | **Intercept** |
| `git config --get` | ✅ Full | ✅ Full | Easy | **Intercept** |
| `git config --list` | ✅ Full | ✅ Full | Easy | Intercept |
| `git describe` | ✅ Full | ⚠️ Partial | Medium | Intercept |
| `git blame` | ✅ Full | ⚠️ Partial | Hard | Skip |
| `git shortlog` | ❌ Manual | ❌ Manual | Hard | Skip |
| `git reflog` | ✅ Full | ✅ Full | Medium | Intercept |
| `git for-each-ref` | ✅ Full | ✅ Full | Medium | Intercept |
| `git count-objects` | ✅ Full | ⚠️ Partial | Easy | Intercept |
| `git fsck` | ❌ None | ❌ None | N/A | Skip |
| `git gc` | ❌ None | ❌ None | N/A | Skip |

### Write Commands (Use CLI for Safety)

| Command | Recommendation | Reason |
|---------|----------------|--------|
| `git add` | CLI | Index manipulation risky |
| `git commit` | CLI | Hooks, GPG signing |
| `git push` | CLI | Auth, SSH, hooks |
| `git pull` | CLI | Merge conflicts, auth |
| `git fetch` | CLI | Auth, progress |
| `git merge` | CLI | Conflict resolution |
| `git rebase` | CLI | Interactive, conflicts |
| `git reset` | CLI | Destructive |
| `git checkout` | CLI | Worktree changes |
| `git switch` | CLI | Worktree changes |
| `git restore` | CLI | Worktree changes |
| `git stash push` | CLI | Index manipulation |
| `git stash pop` | CLI | Merge conflicts |
| `git cherry-pick` | CLI | Conflicts |
| `git revert` | CLI | Conflicts |
| `git tag -a` | CLI | GPG signing |
| `git branch -d` | CLI | Destructive |
| `git branch -D` | CLI | Destructive |
| `git clean` | CLI | Destructive |
| `git rm` | CLI | Index + worktree |
| `git mv` | CLI | Index + worktree |

---

## Detailed Implementation: All Interceptable Commands

### Tier 1: Already Implemented
```rust
["git", "status"]
["git", "status", "--porcelain"]
["git", "status", "-s"]
["git", "branch"]
["git", "branch", "-a"]
["git", "rev-parse", "HEAD"]
["git", "rev-parse", "--abbrev-ref", "HEAD"]
```

### Tier 2: Easy Additions (~1 hour each)

#### `git log --oneline`
```rust
["git", "log", "--oneline"] => GitCommand::LogOneline { count: 10 },
["git", "log", "--oneline", "-n", n] => GitCommand::LogOneline { count: parse(n) },
["git", "log", "--oneline", "-N"] => GitCommand::LogOneline { count: N },

fn execute_log_oneline(repo: &Repository, count: usize) -> String {
    let mut revwalk = repo.revwalk().unwrap();
    revwalk.push_head().unwrap();
    revwalk.take(count)
        .filter_map(|oid| oid.ok())
        .filter_map(|oid| repo.find_commit(oid).ok())
        .map(|c| format!("{} {}", &c.id().to_string()[..7], c.summary().unwrap_or("")))
        .collect::<Vec<_>>()
        .join("\n")
}
```

#### `git ls-files`
```rust
["git", "ls-files"] => GitCommand::LsFiles,

fn execute_ls_files(repo: &Repository) -> String {
    let index = repo.index().unwrap();
    index.iter()
        .filter_map(|e| String::from_utf8(e.path.clone()).ok())
        .collect::<Vec<_>>()
        .join("\n")
}
```

#### `git tag` / `git tag -l`
```rust
["git", "tag"] | ["git", "tag", "-l"] => GitCommand::TagList,

fn execute_tag_list(repo: &Repository) -> String {
    let mut tags = Vec::new();
    repo.tag_foreach(|_, name| {
        if let Ok(name) = std::str::from_utf8(name) {
            tags.push(name.strip_prefix("refs/tags/").unwrap_or(name).to_string());
        }
        true
    }).unwrap();
    tags.sort();
    tags.join("\n")
}
```

#### `git remote -v`
```rust
["git", "remote", "-v"] => GitCommand::RemoteVerbose,

fn execute_remote_verbose(repo: &Repository) -> String {
    let mut output = String::new();
    for name in repo.remotes().unwrap().iter().flatten() {
        if let Ok(remote) = repo.find_remote(name) {
            let url = remote.url().unwrap_or("");
            output.push_str(&format!("{}\t{} (fetch)\n", name, url));
            output.push_str(&format!("{}\t{} (push)\n", name, remote.pushurl().unwrap_or(url)));
        }
    }
    output
}
```

#### `git stash list`
```rust
["git", "stash", "list"] => GitCommand::StashList,

fn execute_stash_list(repo: &Repository) -> String {
    let mut output = String::new();
    repo.stash_foreach(|idx, msg, _| {
        output.push_str(&format!("stash@{{{}}}: {}\n", idx, msg));
        true
    }).unwrap();
    output
}
```

#### `git config --get KEY`
```rust
["git", "config", "--get", key] => GitCommand::ConfigGet { key },

fn execute_config_get(repo: &Repository, key: &str) -> String {
    repo.config().unwrap()
        .get_string(key)
        .unwrap_or_default()
}
```

### Tier 3: Medium Effort (~2-3 hours each)

#### `git diff --stat`
```rust
["git", "diff", "--stat"] => GitCommand::DiffStat { staged: false },
["git", "diff", "--staged", "--stat"] => GitCommand::DiffStat { staged: true },
["git", "diff", "--cached", "--stat"] => GitCommand::DiffStat { staged: true },

fn execute_diff_stat(repo: &Repository, staged: bool) -> String {
    let diff = if staged {
        let head = repo.head().unwrap().peel_to_tree().unwrap();
        repo.diff_tree_to_index(Some(&head), None, None).unwrap()
    } else {
        repo.diff_index_to_workdir(None, None).unwrap()
    };

    let stats = diff.stats().unwrap();
    let mut output = String::new();

    for delta in diff.deltas() {
        let path = delta.new_file().path().unwrap().display();
        // Would need to track per-file stats...
        output.push_str(&format!(" {} | ...\n", path));
    }

    output.push_str(&format!(
        " {} files changed, {} insertions(+), {} deletions(-)\n",
        stats.files_changed(), stats.insertions(), stats.deletions()
    ));
    output
}
```

#### `git diff --name-only`
```rust
["git", "diff", "--name-only"] => GitCommand::DiffNameOnly { staged: false },

fn execute_diff_name_only(repo: &Repository, staged: bool) -> String {
    let diff = if staged {
        let head = repo.head().unwrap().peel_to_tree().unwrap();
        repo.diff_tree_to_index(Some(&head), None, None).unwrap()
    } else {
        repo.diff_index_to_workdir(None, None).unwrap()
    };

    diff.deltas()
        .filter_map(|d| d.new_file().path())
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}
```

#### `git show HEAD`
```rust
["git", "show", "HEAD"] => GitCommand::ShowHead,
["git", "show", rev] => GitCommand::Show { rev },

fn execute_show(repo: &Repository, rev: &str) -> String {
    let obj = repo.revparse_single(rev).unwrap();
    let commit = obj.peel_to_commit().unwrap();

    let mut output = format!(
        "commit {}\nAuthor: {} <{}>\nDate:   {}\n\n    {}\n",
        commit.id(),
        commit.author().name().unwrap_or(""),
        commit.author().email().unwrap_or(""),
        // format time...
        commit.message().unwrap_or("")
    );

    // Optionally add diff...
    output
}
```

#### `git describe`
```rust
["git", "describe"] => GitCommand::Describe { tags: false },
["git", "describe", "--tags"] => GitCommand::Describe { tags: true },

fn execute_describe(repo: &Repository, tags: bool) -> String {
    let opts = git2::DescribeOptions::new();
    // Configure options...
    repo.describe(&opts)
        .and_then(|d| d.format(None))
        .unwrap_or_default()
}
```

### Tier 4: Complex (Skip or Partial)

| Command | Issue | Recommendation |
|---------|-------|----------------|
| `git log --graph` | ASCII graph rendering | Skip |
| `git log --format=...` | 40+ format codes | Skip |
| `git diff` (full patch) | Complex formatting | Skip |
| `git blame` | Line-by-line attribution | Skip |
| `git shortlog` | Author grouping | Skip |

---

## Complete Interception Map

```rust
fn try_parse_git_command(cmd: &str) -> Option<GitCommand> {
    // Don't intercept pipes/redirects
    if cmd.contains('|') || cmd.contains('>') || cmd.contains('<') {
        return None;
    }

    let parts: Vec<&str> = cmd.split_whitespace().collect();

    match parts.as_slice() {
        // === STATUS ===
        ["git", "status"] => Some(GitCommand::Status { porcelain: false }),
        ["git", "status", "--porcelain" | "-s" | "--short"] => Some(GitCommand::Status { porcelain: true }),

        // === BRANCH ===
        ["git", "branch"] => Some(GitCommand::Branch { all: false, remotes: false }),
        ["git", "branch", "-a" | "--all"] => Some(GitCommand::Branch { all: true, remotes: false }),
        ["git", "branch", "-r" | "--remotes"] => Some(GitCommand::Branch { all: false, remotes: true }),

        // === LOG ===
        ["git", "log", "--oneline"] => Some(GitCommand::LogOneline { count: 10 }),
        ["git", "log", "--oneline", "-n", n] => Some(GitCommand::LogOneline { count: n.parse().ok()? }),
        ["git", "log", "-1", "--oneline"] => Some(GitCommand::LogOneline { count: 1 }),

        // === DIFF ===
        ["git", "diff", "--stat"] => Some(GitCommand::DiffStat { staged: false }),
        ["git", "diff", "--cached" | "--staged", "--stat"] => Some(GitCommand::DiffStat { staged: true }),
        ["git", "diff", "--name-only"] => Some(GitCommand::DiffNameOnly { staged: false }),
        ["git", "diff", "--cached" | "--staged", "--name-only"] => Some(GitCommand::DiffNameOnly { staged: true }),
        ["git", "diff", "--name-status"] => Some(GitCommand::DiffNameStatus { staged: false }),

        // === REV-PARSE ===
        ["git", "rev-parse", "HEAD"] => Some(GitCommand::RevParse { rev: "HEAD", abbrev: false }),
        ["git", "rev-parse", "--abbrev-ref", "HEAD"] => Some(GitCommand::RevParse { rev: "HEAD", abbrev: true }),
        ["git", "rev-parse", "--short", "HEAD"] => Some(GitCommand::RevParseShort),

        // === SHOW ===
        ["git", "show", "--stat"] => Some(GitCommand::ShowStat { rev: "HEAD" }),
        ["git", "show", "--stat", rev] => Some(GitCommand::ShowStat { rev }),

        // === LS-FILES ===
        ["git", "ls-files"] => Some(GitCommand::LsFiles),
        ["git", "ls-files", "-m" | "--modified"] => Some(GitCommand::LsFilesModified),

        // === TAG ===
        ["git", "tag"] | ["git", "tag", "-l"] => Some(GitCommand::TagList),

        // === REMOTE ===
        ["git", "remote"] => Some(GitCommand::RemoteList),
        ["git", "remote", "-v"] => Some(GitCommand::RemoteVerbose),

        // === STASH ===
        ["git", "stash", "list"] => Some(GitCommand::StashList),

        // === CONFIG ===
        ["git", "config", "--get", key] => Some(GitCommand::ConfigGet { key: key.to_string() }),

        // === DESCRIBE ===
        ["git", "describe"] => Some(GitCommand::Describe { tags: false }),
        ["git", "describe", "--tags"] => Some(GitCommand::Describe { tags: true }),

        // === REFLOG ===
        ["git", "reflog"] => Some(GitCommand::Reflog { count: 10 }),
        ["git", "reflog", "-n", n] => Some(GitCommand::Reflog { count: n.parse().ok()? }),

        // === COUNT-OBJECTS ===
        ["git", "count-objects"] => Some(GitCommand::CountObjects),

        _ => None,
    }
}
```

---

## Conclusion

**Phase 2 should add**:
1. `git log --oneline [-n N]` - High usage, medium effort
2. `git diff --stat` - Common in scripts
3. `git diff --name-only` - Easy win

**Skip for now**:
- Full patch diff output (too complex)
- Custom log formats (too many specifiers)
- Piped commands (impossible to intercept correctly)

**Expected impact**: Additional 30-50x speedup for these common commands.

---

## Implementation Roadmap

### Phase 1 (Done)
- `git status`
- `git branch`
- `git rev-parse`

### Phase 2 (Next - ~8 hours)
- `git log --oneline`
- `git diff --stat`
- `git diff --name-only`
- `git ls-files`
- `git tag`

### Phase 3 (Later - ~6 hours)
- `git remote -v`
- `git stash list`
- `git config --get`
- `git describe`
- `git show --stat`

### Phase 4 (If Needed)
- `git reflog`
- `git count-objects`
- `git for-each-ref`

### Never Intercept
- Any write command (add, commit, push, etc.)
- Piped commands
- Commands with complex output formatting
