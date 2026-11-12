#!/usr/bin/env bash
# lang_parity.sh <lang|rules.json> <docs.bin> [backend] — every tokenizer check for one language, exit 1 on any diff:
#   affix level: prefix/suffix/infix/url_match/token_match on the docs' chunks, rule keys and random strings;
#   token level: spans on the real docs and on fuzz docs built from the language's rules and words.
# Reference dumps are cached next to docs.bin (delete them after changing spaCy).
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
REPO=$(cd "$ROOT/../.." && pwd)
PY=${PY:-$REPO/.venv/bin/python}
SPEC=$1
DOCS=$2
BACKEND=${3:-split}
LANG_CODE=$SPEC
[[ $SPEC == *.json ]] && LANG_CODE=$("$PY" -c 'import json,sys; print(json.load(open(sys.argv[1]))["lang"])' "$SPEC")
export PYTHONIOENCODING=utf-8 PYTHONWARNINGS=ignore
TARGET=$(cargo metadata -q --format-version 1 --no-deps --manifest-path "$REPO/Cargo.toml" | "$PY" -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
cargo build -q --release -j 2 --manifest-path "$REPO/Cargo.toml" -p spacy-tokenizer-rs --bins || exit 2
fail=0

STR=$DOCS.strings-$LANG_CODE
[ -f "$STR" ] || "$PY" "$ROOT/tools/affix_ref.py" corpus "$DOCS" "$STR" 200000 "$LANG_CODE" >/dev/null
[ -f "$STR.ref" ] || "$PY" "$ROOT/tools/affix_ref.py" ref "$STR" "$STR.ref" "$LANG_CODE"
echo "== affixes"
if "$TARGET/release/affixcheck" "$STR" "$STR.rust" "$BACKEND" "$SPEC"; then
  "$PY" "$ROOT/tools/affix_ref.py" diff "$STR" "$STR.ref" "$STR.rust" | head -12
  [ "${PIPESTATUS[0]}" = 0 ] || fail=1
else
  fail=1
fi

FUZZ=$DOCS.fuzz-$LANG_CODE
[ -f "$FUZZ" ] || "$PY" "$ROOT/tools/fuzz_docs.py" "$FUZZ" 3000 "$LANG_CODE" "$DOCS" >/dev/null
for d in "$DOCS" "$FUZZ"; do
  echo "== tokens $(basename "$d")"
  "$ROOT/tools/parity.sh" "$d" "$BACKEND" 8 "$SPEC" | head -40
  [ "${PIPESTATUS[0]}" = 0 ] || fail=1
done
echo "RESULT $LANG_CODE $([ $fail = 0 ] && echo PASS || echo FAIL)"
exit $fail
