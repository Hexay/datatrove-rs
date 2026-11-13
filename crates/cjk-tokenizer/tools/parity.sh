#!/usr/bin/env bash
# parity.sh <zh|ja> <docs.bin> [max_examples] — datatrove's SpaCyTokenizer doc tokens vs cjkdump, exit 1 on any diff.
# The reference is cached next to docs.bin (<ref>.version holds the jieba/SudachiPy/dictionary versions it came
# from); the Rust dump goes to $SPACY_PARITY_OUT as for the spaCy tokenizer scripts. One light.lock slot per run.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
REPO=$(cd "$ROOT/../.." && pwd)
PY=${PY:-$REPO/.venv/bin/python}
PROFILE=${PROFILE:-iter}
LANG_CODE=$1
DOCS=$2
OUT=${SPACY_PARITY_OUT:-${XDG_CACHE_HOME:-$HOME/.cache}/datatrove-rs/out/$(basename "$REPO")}
mkdir -p "$OUT"
export PYTHONIOENCODING=utf-8 PYTHONWARNINGS=ignore
. "$REPO/tools/slot.sh"

if [ -z "${LIGHT_SLOT_HELD:-}" ]; then
  [ "$PROFILE" = iter ] && unset CARGO_INCREMENTAL  # see tools/dev.sh
  cargo build -q --profile "$PROFILE" -j "${CARGO_BUILD_JOBS:-2}" --manifest-path "$REPO/Cargo.toml" \
    -p cjk-tokenizer-rs --bins || exit 2
  in_light_slot bash "$0" "$@"
  exit
fi

TARGET=$(cargo metadata -q --format-version 1 --no-deps --manifest-path "$REPO/Cargo.toml" \
  | "$PY" -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
mapfile -t P < <("$PY" "$ROOT/tools/cjk_ref.py" paths)
VERSION=$("$PY" -c 'from importlib.metadata import version as v; print(v("jieba"), v("sudachipy"), v("sudachidict-core"))')
REF=$DOCS.ref-$LANG_CODE
if [ ! -f "$REF" ] || [ "$(cat "$REF.version" 2>/dev/null)" != "$VERSION" ]; then
  "$PY" "$ROOT/tools/cjk_ref.py" dump "$DOCS" "$REF.tmp.$$" "$LANG_CODE" && mv "$REF.tmp.$$" "$REF"
  echo "$VERSION" > "$REF.version"
fi
RUST=$OUT/$(basename "$DOCS").rust-$LANG_CODE
case $LANG_CODE in
  zh)
    HMM=$OUT/jieba-hmm.json
    "$PY" "$ROOT/tools/cjk_ref.py" hmm "$HMM"
    "$TARGET/$PROFILE/cjkdump" "$DOCS" "$RUST" zh "${P[0]}" "$HMM" ;;
  ja) "$TARGET/$PROFILE/cjkdump" "$DOCS" "$RUST" ja "${P[1]}" "${P[2]}" "${P[3]}" ;;
  *) echo "language must be zh or ja" >&2; exit 2 ;;
esac
"$PY" "$REPO/crates/spacy-tokenizer/tools/compare.py" "$DOCS" "$REF" "$RUST" "${3:-20}"
