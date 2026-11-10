#!/usr/bin/env bash
# icount.sh <docs.bin> <stage-substring>... — user-space instructions per doc for each `profile` stage.
# Instruction counts are stable under machine load, unlike wall time; use them to A/B Rust changes.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
docs=${1:?usage: icount.sh <docs.bin> <stage>...}
shift
cargo build -q --release -j 4 --manifest-path "$ROOT/Cargo.toml" -p datatrove-core --bin profile
BIN=$(cargo metadata -q --format-version 1 --manifest-path "$ROOT/Cargo.toml" | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')/release/profile
n=$(tr -cd '\0' < "$docs" | wc -c)
n=$((n + 1))
base=$(perf stat -x, -e instructions:u "$BIN" "$docs" 1 __none__ 2>&1 >/dev/null | cut -d, -f1)
for stage in "$@"; do
  total=$(perf stat -x, -e instructions:u "$BIN" "$docs" 1 "$stage" 2>&1 >/dev/null | cut -d, -f1)
  # 1 rep + 50 warmup docs per matching stage; base subtracts startup and corpus load.
  printf '%-34s %10.0f instr/doc\n' "$stage" "$(echo "($total - $base) / ($n + 50)" | bc -l)"
done
