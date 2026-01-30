# gitgrip Benchmark Comparison: Rust vs TypeScript

## Test Environment
- **Date:** 2026-01-30
- **System:** Darwin arm64 (Apple Silicon M-series)
- **Rust:** rustc 1.93.0
- **Node:** v22.22.0

## Summary

| Benchmark | TypeScript | Rust (Criterion) | Speedup |
|-----------|-----------|------------------|---------|
| manifest_parse | 0.431ms | 0.023ms | **18.7x faster** |
| state_parse | 0.002ms | 0.001ms | **1.5x faster** |
| url_parse | <0.001ms | 0.0005ms | Similar (sub-ms) |
| manifest_validate | N/A | 0.0003ms | Rust-only |
| git_status | N/A | 0.212ms | Rust-only |

### Key Findings

1. **Manifest Parsing: ~19x faster**
   - TypeScript (yaml pkg): 0.431ms average
   - Rust (serde_yaml): 0.023ms (23.29µs via Criterion)
   - This is the most impactful improvement as manifest parsing happens on every command

2. **State Parsing: ~1.5x faster**
   - TypeScript: 0.002ms average
   - Rust: 0.001ms (1.32µs via Criterion)
   - JSON parsing is already highly optimized in V8, so gains are modest

3. **URL Parsing: Similar performance**
   - Both are sub-millisecond (495ns in Rust)
   - At these scales, measurement noise dominates

4. **Additional Rust benchmarks**
   - **manifest_validate**: 277ns - validates manifest structure
   - **url_parse_azure_https**: 549ns - Azure DevOps URL parsing
   - **git_status**: 212µs - git2 library status check on test repo

## Detailed Results

### TypeScript Results (100 iterations)

```
manifest_parse: avg=0.431ms, min=0.217ms, max=1.110ms, p95=0.833ms
state_parse:    avg=0.002ms, min=0.001ms, max=0.003ms, p95=0.002ms
url_parse:      avg=0.000ms, min=0.000ms, max=0.004ms, p95=0.001ms
```

### Rust Criterion Results (statistical, 100+ samples)

```
manifest_parse:        time: [23.232 µs 23.290 µs 23.350 µs]
state_parse:           time: [1.3114 µs 1.3186 µs 1.3263 µs]
url_parse_github_ssh:  time: [494.31 ns 495.75 ns 497.38 ns]
url_parse_azure_https: time: [547.68 ns 549.17 ns 550.90 ns]
manifest_validate:     time: [276.13 ns 276.91 ns 277.77 ns]
git_status:            time: [210.62 µs 211.94 µs 213.40 µs]
```

### Rust CLI Results (gr bench, 100 iterations)

```
manifest_parse: avg=0.014ms, min=0.010ms, max=0.023ms, p95=0.019ms
state_parse:    avg=0.000ms, min=0.000ms, max=0.003ms, p95=0.001ms
url_parse:      avg=0.001ms, min=0.001ms, max=0.002ms, p95=0.001ms
```

## Notes

- **Criterion** uses statistical analysis with 100+ samples and warmup for accuracy
- **TypeScript** uses Node.js with the `yaml` package for YAML parsing
- **Rust** uses `serde_yaml` for YAML and `serde_json` for JSON parsing
- Speedup = TypeScript time / Rust time (higher is better for Rust)
- Sub-millisecond operations approach timer resolution limits in JavaScript

## Real-World Implications

The **~19x improvement in manifest parsing** translates to noticeable performance gains in:
- `gr sync` - parses manifest on every invocation
- `gr status` - parses manifest for repo list
- `gr branch`, `gr checkout`, etc. - all load the manifest

For a workspace with many repos, these micro-improvements compound into a perceptibly snappier CLI experience. The Rust version also has:
- **No JIT warmup** - first execution is as fast as subsequent ones
- **Lower memory usage** - no V8 heap overhead
- **Faster startup** - no Node.js runtime initialization (~50-100ms saved)

## Running Benchmarks

```bash
# Run full comparison
./rust/run-benchmarks.sh 100

# Run Rust benchmarks only
cd rust && cargo bench

# Run TypeScript benchmarks only
npx tsx rust/bench-compare.ts 100

# Run Rust CLI benchmarks
./rust/target/release/gr bench --iterations 100
```
