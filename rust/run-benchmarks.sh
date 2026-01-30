#!/bin/bash
# Unified benchmark comparison script for Rust vs TypeScript
# This script runs both benchmark suites and generates a comparison report

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUST_DIR="$SCRIPT_DIR"
TS_DIR="$(dirname "$SCRIPT_DIR")"
OUTPUT_DIR="$RUST_DIR/benchmark-results"
ITERATIONS=${1:-100}

echo "=============================================="
echo "  gitgrip Benchmark Comparison"
echo "  Rust vs TypeScript"
echo "=============================================="
echo ""
echo "Iterations: $ITERATIONS"
echo "Output directory: $OUTPUT_DIR"
echo ""

mkdir -p "$OUTPUT_DIR"

# Run TypeScript benchmarks
echo "=============================================="
echo "  Running TypeScript Benchmarks..."
echo "=============================================="
cd "$TS_DIR"
npx tsx "$RUST_DIR/bench-compare.ts" "$ITERATIONS" 2>&1 | tee "$OUTPUT_DIR/typescript-results.txt"

echo ""
echo "=============================================="
echo "  Running Rust Benchmarks..."
echo "=============================================="
cd "$RUST_DIR"

# Run Rust benchmarks with cargo bench
# Note: Criterion uses its own iteration logic, so iterations param is for warmup
cargo bench --quiet 2>&1 | tee "$OUTPUT_DIR/rust-results.txt"

# Also run the built-in gr bench command for comparison
echo ""
echo "=============================================="
echo "  Running Rust CLI Benchmarks (gr bench)..."
echo "=============================================="
./target/release/gr bench --iterations "$ITERATIONS" 2>&1 | tee "$OUTPUT_DIR/rust-cli-results.txt"

echo ""
echo "=============================================="
echo "  Generating Comparison Report..."
echo "=============================================="

# Generate comparison report
cat > "$OUTPUT_DIR/COMPARISON-REPORT.md" << 'EOF'
# gitgrip Benchmark Comparison: Rust vs TypeScript

## Test Environment
EOF

echo "- **Date:** $(date)" >> "$OUTPUT_DIR/COMPARISON-REPORT.md"
echo "- **System:** $(uname -s) $(uname -m)" >> "$OUTPUT_DIR/COMPARISON-REPORT.md"
echo "- **Rust:** $(rustc --version)" >> "$OUTPUT_DIR/COMPARISON-REPORT.md"
echo "- **Node:** $(node --version)" >> "$OUTPUT_DIR/COMPARISON-REPORT.md"
echo "- **Iterations:** $ITERATIONS" >> "$OUTPUT_DIR/COMPARISON-REPORT.md"

cat >> "$OUTPUT_DIR/COMPARISON-REPORT.md" << 'EOF'

## Summary

| Benchmark | TypeScript | Rust | Speedup |
|-----------|------------|------|---------|
EOF

# Parse TypeScript results
TS_MANIFEST_AVG=$(grep "manifest_parse:" "$OUTPUT_DIR/typescript-results.txt" | grep -oE "avg=[0-9.]+" | grep -oE "[0-9.]+")
TS_STATE_AVG=$(grep "state_parse:" "$OUTPUT_DIR/typescript-results.txt" | grep -oE "avg=[0-9.]+" | grep -oE "[0-9.]+")
TS_URL_AVG=$(grep "url_parse:" "$OUTPUT_DIR/typescript-results.txt" | grep -oE "avg=[0-9.]+" | grep -oE "[0-9.]+")

# Parse Rust Criterion results (convert ns to ms for comparison)
# Criterion output format: benchmark_name    time:   [xxx ns xxx ns xxx ns]
RUST_MANIFEST_NS=$(grep "manifest_parse" "$OUTPUT_DIR/rust-results.txt" | head -1 | grep -oE "\[[0-9.]+ [a-z]+ [0-9.]+ [a-z]+ [0-9.]+ [a-z]+\]" | grep -oE "^[0-9.]+" | head -1 || echo "0")
RUST_STATE_NS=$(grep "state_parse" "$OUTPUT_DIR/rust-results.txt" | head -1 | grep -oE "\[[0-9.]+ [a-z]+ [0-9.]+ [a-z]+ [0-9.]+ [a-z]+\]" | grep -oE "^[0-9.]+" | head -1 || echo "0")
RUST_URL_NS=$(grep "url_parse_github_ssh" "$OUTPUT_DIR/rust-results.txt" | head -1 | grep -oE "\[[0-9.]+ [a-z]+ [0-9.]+ [a-z]+ [0-9.]+ [a-z]+\]" | grep -oE "^[0-9.]+" | head -1 || echo "0")

# Fallback: Parse from CLI results if Criterion didn't output properly
if [ -z "$RUST_MANIFEST_NS" ] || [ "$RUST_MANIFEST_NS" = "0" ]; then
    RUST_MANIFEST_MS=$(grep "manifest_parse:" "$OUTPUT_DIR/rust-cli-results.txt" 2>/dev/null | grep -oE "avg=[0-9.]+" | grep -oE "[0-9.]+" || echo "0")
else
    RUST_MANIFEST_MS=$(echo "scale=6; $RUST_MANIFEST_NS / 1000000" | bc 2>/dev/null || echo "0")
fi

if [ -z "$RUST_STATE_NS" ] || [ "$RUST_STATE_NS" = "0" ]; then
    RUST_STATE_MS=$(grep "state_parse:" "$OUTPUT_DIR/rust-cli-results.txt" 2>/dev/null | grep -oE "avg=[0-9.]+" | grep -oE "[0-9.]+" || echo "0")
else
    RUST_STATE_MS=$(echo "scale=6; $RUST_STATE_NS / 1000000" | bc 2>/dev/null || echo "0")
fi

if [ -z "$RUST_URL_NS" ] || [ "$RUST_URL_NS" = "0" ]; then
    RUST_URL_MS=$(grep "url_parse:" "$OUTPUT_DIR/rust-cli-results.txt" 2>/dev/null | grep -oE "avg=[0-9.]+" | grep -oE "[0-9.]+" || echo "0")
else
    RUST_URL_MS=$(echo "scale=6; $RUST_URL_NS / 1000000" | bc 2>/dev/null || echo "0")
fi

# Calculate speedups (avoid division by zero)
calc_speedup() {
    ts=$1
    rust=$2
    if [ -n "$ts" ] && [ -n "$rust" ] && [ "$rust" != "0" ]; then
        echo "scale=1; $ts / $rust" | bc 2>/dev/null || echo "N/A"
    else
        echo "N/A"
    fi
}

SPEEDUP_MANIFEST=$(calc_speedup "$TS_MANIFEST_AVG" "$RUST_MANIFEST_MS")
SPEEDUP_STATE=$(calc_speedup "$TS_STATE_AVG" "$RUST_STATE_MS")
SPEEDUP_URL=$(calc_speedup "$TS_URL_AVG" "$RUST_URL_MS")

echo "| manifest_parse | ${TS_MANIFEST_AVG:-N/A}ms | ${RUST_MANIFEST_MS:-N/A}ms | ${SPEEDUP_MANIFEST}x |" >> "$OUTPUT_DIR/COMPARISON-REPORT.md"
echo "| state_parse | ${TS_STATE_AVG:-N/A}ms | ${RUST_STATE_MS:-N/A}ms | ${SPEEDUP_STATE}x |" >> "$OUTPUT_DIR/COMPARISON-REPORT.md"
echo "| url_parse | ${TS_URL_AVG:-N/A}ms | ${RUST_URL_MS:-N/A}ms | ${SPEEDUP_URL}x |" >> "$OUTPUT_DIR/COMPARISON-REPORT.md"

cat >> "$OUTPUT_DIR/COMPARISON-REPORT.md" << 'EOF'

## Detailed Results

### TypeScript Results
EOF
cat "$OUTPUT_DIR/typescript-results.txt" >> "$OUTPUT_DIR/COMPARISON-REPORT.md"

cat >> "$OUTPUT_DIR/COMPARISON-REPORT.md" << 'EOF'

### Rust Criterion Results
EOF
cat "$OUTPUT_DIR/rust-results.txt" >> "$OUTPUT_DIR/COMPARISON-REPORT.md"

cat >> "$OUTPUT_DIR/COMPARISON-REPORT.md" << 'EOF'

### Rust CLI Results (gr bench)
EOF
cat "$OUTPUT_DIR/rust-cli-results.txt" >> "$OUTPUT_DIR/COMPARISON-REPORT.md"

cat >> "$OUTPUT_DIR/COMPARISON-REPORT.md" << 'EOF'

## Notes

- **TypeScript** uses Node.js with the `yaml` package for YAML parsing
- **Rust** uses `serde_yaml` for YAML and `serde_json` for JSON parsing
- Speedup = TypeScript time / Rust time (higher is better for Rust)
- Rust benchmarks use Criterion for statistical rigor
- Rust CLI benchmarks (`gr bench`) use simple timing for comparison
EOF

echo ""
echo "=============================================="
echo "  Benchmark Complete!"
echo "=============================================="
echo ""
echo "Results saved to:"
echo "  - $OUTPUT_DIR/COMPARISON-REPORT.md"
echo "  - $OUTPUT_DIR/typescript-results.txt"
echo "  - $OUTPUT_DIR/rust-results.txt"
echo "  - $OUTPUT_DIR/rust-cli-results.txt"
echo ""
cat "$OUTPUT_DIR/COMPARISON-REPORT.md"
