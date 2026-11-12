#!/usr/bin/env bash
# parity.sh <docs.bin> [backend] [max_examples] [lang|rules.json] — spaCy dump (cached next to docs) vs Rust dump.
# With a rules.json path, its "lang" field names the spaCy language. Env: see common.sh.
set -euo pipefail
. "$(dirname "$0")/common.sh"
DOCS=$1
BACKEND=${2:-split}
SPEC=${4:-en}
LANG_CODE=$SPEC
[[ $SPEC == *.json ]] && LANG_CODE=$("$PY" -c 'import json,sys; print(json.load(open(sys.argv[1]))["lang"])' "$SPEC")
build_bins
in_slot "$0" "$@"
REF=$DOCS.spacy-$LANG_CODE
RUST=$OUT/$(basename "$DOCS").rust-$LANG_CODE
cached "$REF" "$PY" "$ROOT/tools/spacy_ref.py" dump "$DOCS" {} 0 "$LANG_CODE"
"$BIN/bench" "$DOCS" --lang "$SPEC" --backend "$BACKEND" --dump "$RUST"
"$PY" "$ROOT/tools/compare.py" "$DOCS" "$REF" "$RUST" "${3:-20}"
