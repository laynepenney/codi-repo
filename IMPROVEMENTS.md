# codi-repo Improvement Ideas

This file captures friction points, feature ideas, and bugs discovered while using `cr`.
Items here should be reviewed before creating GitHub issues.

---

## Pending Review

#### `cr pr create` should work when repos are on different branches
- **Problem**: `cr pr create` fails with "Repositories are on different branches" even when only a subset of repos have changes. This forces fallback to raw `gh` commands.
- **Example**: When only `tooling` and `manifest` have changes but `public/private/strategy` are on `main`, `cr pr create` refuses to run.
- **Proposal**: Only check branch consistency for repos that actually have commits ahead. Repos on `main` with no changes shouldn't block PR creation for repos that do have changes.

---

## Approved (Ready for Issues)

_No items approved._

---

## Completed

_Items that have been implemented. Keep for historical reference._

### `cr forall` command (Issue #15)
- **Added in**: PR #17
- **Description**: Run arbitrary commands in each repository with `cr forall -c "command"`. Supports `--repo`, `--include-manifest`, and `--continue-on-error` flags.

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
