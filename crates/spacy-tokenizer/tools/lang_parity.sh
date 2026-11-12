#!/usr/bin/env bash
# lang_parity.sh <lang|rules.json> <docs.bin> [backend] — every tokenizer check for one language, exit 1 on any diff:
#   affix level: prefix/suffix/infix/url_match/token_match on the docs' chunks, rule keys and random strings;
#   token level: spans on the real docs and on fuzz docs built from the language's rules and words.
# Reference dumps are cached next to docs.bin and redone when spaCy's version changes; Rust dumps go to $OUT
# (common.sh). Builds the bins once with PROFILE (default iter).
set -uo pipefail
. "$(dirname "$0")/common.sh"
SPEC=$1
DOCS=$2
BACKEND=${3:-split}
LANG_CODE=$SPEC
[[ $SPEC == *.json ]] && LANG_CODE=$("$PY" -c 'import json,sys; print(json.load(open(sys.argv[1]))["lang"])' "$SPEC")
export PYTHONWARNINGS=ignore
build_bins || exit 2
fail=0

STR=$DOCS.strings-$LANG_CODE
RUST=$OUT/$(basename "$STR").rust
cached "$STR" "$PY" "$ROOT/tools/affix_ref.py" corpus "$DOCS" {} 200000 "$LANG_CODE" >/dev/null
cached "$STR.ref" "$PY" "$ROOT/tools/affix_ref.py" ref "$STR" {} "$LANG_CODE"
echo "== affixes"
if "$BIN/affixcheck" "$STR" "$RUST" "$BACKEND" "$SPEC"; then
  "$PY" "$ROOT/tools/affix_ref.py" diff "$STR" "$STR.ref" "$RUST" | head -12
  [ "${PIPESTATUS[0]}" = 0 ] || fail=1
else
  fail=1
fi

FUZZ=$DOCS.fuzz-$LANG_CODE
cached "$FUZZ" "$PY" "$ROOT/tools/fuzz_docs.py" {} 3000 "$LANG_CODE" "$DOCS" >/dev/null
for d in "$DOCS" "$FUZZ"; do
  echo "== tokens $(basename "$d")"
  "$ROOT/tools/parity.sh" "$d" "$BACKEND" 8 "$SPEC" | head -40
  [ "${PIPESTATUS[0]}" = 0 ] || fail=1
done
echo "RESULT $LANG_CODE $([ $fail = 0 ] && echo PASS || echo FAIL)"
exit $fail
