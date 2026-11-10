#!/usr/bin/env bash
# icount.sh <docs.bin> <stage-substring>... — user-space instructions and cycles per doc for each `profile` stage.
# Instructions are stable under machine load; cycles also catch cache-miss effects but vary more. Use both to A/B.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
docs=${1:?usage: icount.sh <docs.bin> <stage>...}
shift
cargo build -q --release -j 4 --manifest-path "$ROOT/Cargo.toml" -p datatrove-core --bin profile
BIN=$(cargo metadata -q --format-version 1 --no-deps --manifest-path "$ROOT/Cargo.toml" | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')/release/profile
n=$(($(tr -cd '\0' < "$docs" | wc -c) + 1))
count() { perf stat -x, -e instructions:u,cycles:u "$BIN" "$docs" 1 "$1" 2>&1 >/dev/null | cut -d, -f1 | paste -sd' '; }
read -r base_i base_c <<< "$(count __none__)"
for stage in "$@"; do
  read -r i c <<< "$(count "$stage")"
  # 1 rep + 50 warmup docs per matching stage; base subtracts startup and corpus load.
  printf '%-34s %9.0f instr/doc %9.0f cycles/doc\n' "$stage" \
    "$(echo "($i - $base_i) / ($n + 50)" | bc -l)" "$(echo "($c - $base_c) / ($n + 50)" | bc -l)"
done
