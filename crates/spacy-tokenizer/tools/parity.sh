#!/usr/bin/env bash
# parity.sh <docs.bin> [backend] [max_examples] — spaCy dump (cached next to docs) vs Rust dump.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
REPO=$(cd "$ROOT/../.." && pwd)
PY=${PY:-$REPO/.venv/bin/python}
DOCS=$1
BACKEND=${2:-split}
export PYTHONIOENCODING=utf-8
cargo build -q --release -j 4 --manifest-path "$REPO/Cargo.toml" -p spacy-tokenizer-rs --bin bench
TARGET=$(cargo metadata -q --format-version 1 --no-deps --manifest-path "$REPO/Cargo.toml" | "$PY" -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
[ -f "$DOCS.spacy" ] || "$PY" "$ROOT/tools/spacy_ref.py" dump "$DOCS" "$DOCS.spacy"
"$TARGET/release/bench" "$DOCS" --backend "$BACKEND" --dump "$DOCS.rust"
"$PY" "$ROOT/tools/compare.py" "$DOCS" "$DOCS.spacy" "$DOCS.rust" "${3:-20}"
