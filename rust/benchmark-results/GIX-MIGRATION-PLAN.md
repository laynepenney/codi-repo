# gix Migration Plan: Should gitoxide Be Default?

## The Question

Based on benchmarks showing gix is 40% faster for multi-repo operations, should we make gix the default instead of git2?

## Benchmark Summary

| Operation (5 repos) | git2 | gix | Winner |
|---------------------|------|-----|--------|
| `gr status` | 1.78 ms | **1.28 ms** | gix 40% faster |
| `forall git status` | 1.64 ms | **1.14 ms** | gix 40% faster |
| Single repo status | **151 µs** | 307 µs | git2 2x faster |
| Branch listing | 241 µs | **29 ns** | gix 8,300x faster |

## Who Uses gitgrip?

**Target users**: Teams managing multiple related repositories
- Microservices architectures (5-20 repos)
- Monorepo alternatives (3-10 repos)
- Related projects (frontend/backend/shared-lib)
- Platform teams (core + plugins)

**If you have 1-2 repos, you don't need gitgrip.** The tool exists specifically for multi-repo coordination. Therefore:

> **Most gitgrip users will have 3+ repos** - exactly where gix excels.

## gix Maturity Assessment

| Aspect | Status | Notes |
|--------|--------|-------|
| Repo open | ✅ Stable | Works well |
| Branch operations | ✅ Stable | Extremely fast |
| HEAD/ref resolution | ✅ Stable | Works well |
| Status API | ⚠️ Maturing | Basic support, improving |
| Clone | ✅ Stable | Works |
| Fetch/Pull | ⚠️ Maturing | May need CLI fallback |
| Push | ⚠️ Maturing | May need CLI fallback |
| Merge/Rebase | ❌ Limited | Use CLI |

**Key insight**: gitgrip's hot path is `gr status` (runs constantly), which primarily needs:
- Repo open ✅
- Get current branch ✅
- Check for changes ⚠️ (workable)

The slower operations (clone, push, pull) are infrequent and can fall back to CLI without impacting UX.

## Recommendation: Hybrid Approach

### Phase 1: gix for Read Operations (Now)

Make gix the default for **read-only operations**:
- `open_repo()` - gix
- `get_current_branch()` - gix
- `list_branches()` - gix
- `has_changes()` - gix (or CLI fallback)

Keep git2/CLI for **write operations**:
- `clone_repo()` - CLI
- `push_branch()` - CLI
- `pull_latest()` - CLI
- `checkout_branch()` - git2 or CLI
- `create_branch()` - git2 or CLI

### Phase 2: Full gix (When Status API Matures)

Once gix's status/index API stabilizes:
- Move `has_changes()` to pure gix
- Evaluate gix for checkout/branch create
- Remove git2 dependency entirely (pure Rust!)

### Implementation

```rust
// Cargo.toml - make gix default
[features]
default = ["gitoxide"]
git2-backend = ["git2"]  # Keep for fallback
gitoxide = ["gix"]

// In code - use gix for reads, CLI for writes
pub fn get_current_branch(repo_path: &Path) -> Result<String, GitError> {
    #[cfg(feature = "gitoxide")]
    {
        let repo = gix::open(repo_path)?;
        // Fast path with gix
    }
    #[cfg(not(feature = "gitoxide"))]
    {
        // Fallback to git2
    }
}

pub fn push_branch(...) -> Result<(), GitError> {
    // Always use CLI for reliability
    Command::new("git").args(["push", ...])
}
```

## Migration Path

### v0.5.x (Current)
- git2 default
- gix optional (`--features gitoxide`)
- Benchmarks prove gix advantage

### v0.6.0 (Next)
- gix default for read operations
- git2 available as `--features git2-backend`
- CLI fallback for immature gix APIs

### v0.7.0 (Future)
- Evaluate full gix adoption
- Remove git2 if gix status API is ready
- Pure Rust binary (no C dependencies!)

## Benefits of gix Default

1. **40% faster** for typical gitgrip workloads
2. **Pure Rust** - easier cross-compilation, no libgit2 build issues
3. **Async-ready** - gix supports async, enabling future parallelization
4. **Active development** - gitoxide is actively improved

## Risks

1. **gix API churn** - May need code updates as gix evolves
2. **Edge cases** - Less battle-tested than git2
3. **Status API gaps** - May need CLI fallback for some status checks

## Decision

**Yes, gix should become the default** for gitgrip because:

1. gitgrip users have 3+ repos (by definition)
2. gix is 40% faster for multi-repo operations
3. The hot path (`gr status`) uses operations gix handles well
4. Write operations can use CLI (already battle-tested)
5. Pure Rust distribution is a significant win

**Timeline**: Target gix-default in v0.6.0 after validating the hybrid approach works reliably.
