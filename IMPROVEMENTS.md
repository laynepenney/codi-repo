# codi-repo Improvement Ideas

This file captures friction points, feature ideas, and bugs discovered while using `cr`.
Items here should be reviewed before creating GitHub issues.

---

## Pending Review

#### `cr forall` command (like AOSP `repo forall`)
- **Problem**: Some git operations (rebase, cherry-pick, stash, etc.) don't have dedicated `cr` commands yet, forcing users to run raw `git` in each repo manually
- **Observation**: AOSP's `repo` tool solves this with `repo forall -c "git command"` which runs a shell command in every repo directory
- **Proposal**: Add `cr forall -c "command"` that runs an arbitrary command in each repo (and optionally the manifest)
- **Example usage**:
  ```bash
  cr forall -c "git rebase origin/main"       # Rebase all repos
  cr forall -c "git stash"                     # Stash all repos
  cr forall -c "pnpm install"                  # Install deps in all repos
  cr forall --repo tooling -c "git log -5"     # Run in specific repo only
  cr forall --include-manifest -c "git rebase origin/main"  # Include manifest
  ```
- **Priority**: Medium - eliminates the last reason to use raw `git` commands

---

## Approved (Ready for Issues)

_Items moved here after user approval. Create GitHub issues and remove from this list._

---

## Completed

_Items that have been implemented. Keep for historical reference._

### Manifest repo managed by cr (Issue #9)
- **Added in**: PR #12
- **Description**: Manifest repo (`.codi-repo/manifests/`) is now automatically included in all `cr` commands when it has changes. `cr status` shows manifest in a separate section. `cr branch --include-manifest` explicitly includes manifest. `cr pr create/status/merge` handle manifest PRs.

### `cr sync` manifest recovery (Issue #4)
- **Added in**: PR #10
- **Description**: `cr sync` now automatically recovers when manifest's upstream branch was deleted after PR merge

### `cr commit` command (Issue #5)
- **Added in**: PR #10
- **Description**: Commit staged changes across all repos with `cr commit -m "message"`

### `cr push` command (Issue #6)
- **Added in**: PR #10
- **Description**: Push current branch across all repos with `cr push`

### `cr bench` command
- **Added in**: PR #1
- **Description**: Benchmark workspace operations with `cr bench`

### `--timing` flag
- **Added in**: PR #1
- **Description**: Global `--timing` flag shows operation timing breakdown

### `cr add` command (Issue #7)
- **Added in**: PR #11
- **Description**: Stage changes across all repos with `cr add .` or `cr add <files>`

### `cr diff` command (Issue #8)
- **Added in**: PR #11
- **Description**: Show diff across all repos with `cr diff`, supports `--staged`, `--stat`, `--name-only`

### `cr branch --repo` flag (Issue #2)
- **Added in**: PR #11
- **Description**: Create branches in specific repos only with `cr branch feat/x --repo tooling`
