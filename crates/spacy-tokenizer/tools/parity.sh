#!/usr/bin/env bash
# parity.sh <docs.bin> [backend] [max_examples] [lang|rules.json] — spaCy dump (cached next to docs) vs Rust dump.
# With a rules.json path, its "lang" field names the spaCy language.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
REPO=$(cd "$ROOT/../.." && pwd)
PY=${PY:-$REPO/.venv/bin/python}
DOCS=$1
BACKEND=${2:-split}
SPEC=${4:-en}
LANG_CODE=$SPEC
[[ $SPEC == *.json ]] && LANG_CODE=$("$PY" -c 'import json,sys; print(json.load(open(sys.argv[1]))["lang"])' "$SPEC")
export PYTHONIOENCODING=utf-8
cargo build -q --release -j 2 --manifest-path "$REPO/Cargo.toml" -p spacy-tokenizer-rs --bin bench
TARGET=$(cargo metadata -q --format-version 1 --no-deps --manifest-path "$REPO/Cargo.toml" | "$PY" -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
REF=$DOCS.spacy-$LANG_CODE
[ -f "$REF" ] || "$PY" "$ROOT/tools/spacy_ref.py" dump "$DOCS" "$REF" 0 "$LANG_CODE"
"$TARGET/release/bench" "$DOCS" --lang "$SPEC" --backend "$BACKEND" --dump "$DOCS.rust-$LANG_CODE"
"$PY" "$ROOT/tools/compare.py" "$DOCS" "$REF" "$DOCS.rust-$LANG_CODE" "${3:-20}"
