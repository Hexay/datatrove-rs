#!/usr/bin/env bash
# parity.sh <docs.bin> [backend] [max_examples] — spaCy dump (cached next to docs) vs Rust dump.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
TARGET=$ROOT/../../target
PY=${PY:-python}
DOCS=$1
BACKEND=${2:-split}
export PYTHONIOENCODING=utf-8
[ -f "$DOCS.spacy" ] || "$PY" $ROOT/tools/spacy_ref.py dump "$DOCS" "$DOCS.spacy"
"$TARGET/release/bench.exe" "$DOCS" --backend "$BACKEND" --dump "$DOCS.rust"
"$PY" $ROOT/tools/compare.py "$DOCS" "$DOCS.spacy" "$DOCS.rust" "${3:-20}"
