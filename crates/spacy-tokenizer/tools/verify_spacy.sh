#!/usr/bin/env bash
# verify_spacy.sh <spacy version> [python] [reference rules dir] — re-export every language's rules with that spaCy
# release in a throwaway venv and compare them byte for byte, apart from the "spacy_version" stamp, with data/rules
# (or another export, e.g. one made by spaCy 3.8.18 on the same Python when the release has no wheel for the Python
# data/rules was made with). Exit 0 iff every file matches. Also prints hashes of the spaCy sources the Rust ports
# reproduce. Venvs and exports go under $SPACY_VERIFY_DIR (default /srv/agents/cache/spacy-verify, else ~/.cache).
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
VERSION=${1:?usage: verify_spacy.sh <spacy version> [python] [reference rules dir]}
PYVER=${2:-3.13}
REF=${3:-$ROOT/data/rules}
base=/srv/agents/cache/spacy-verify
[ -d "${base%/*}" ] || base=$HOME/.cache/datatrove-rs/spacy-verify
DIR=${SPACY_VERIFY_DIR:-$base}/spacy-$VERSION-py$PYVER
VENV=$DIR/venv
if [ ! -x "$VENV/bin/python" ]; then
  # click: spaCy < 3.8.3 imports it, but the latest typer no longer pulls it in
  uv venv -q -p "$PYVER" "$VENV" && uv pip install -q -p "$VENV/bin/python" "spacy==$VERSION" click || exit 2
fi
mkdir -p "$DIR/rules"
langs=$(cd "$ROOT/data/rules" && ls -- *.json | command sed 's/\.json$//')
# shellcheck disable=SC2086
"$VENV/bin/python" "$ROOT/tools/export_rules.py" $langs --out-dir "$DIR/rules" >/dev/null || exit 2
"$VENV/bin/python" - "$REF" "$DIR/rules" <<'EOF'
import hashlib, pathlib, re, sys, spacy
root = pathlib.Path(spacy.__file__).parent
for f in ("tokenizer.pyx", "pipeline/sentencizer.pyx", "util.py", "lang/vi/__init__.py", "lang/zh/__init__.py"):
    print(f"sha256 {f} {hashlib.sha256((root / f).read_bytes()).hexdigest()[:16]}")
ref, new = map(pathlib.Path, sys.argv[1:])
unstamp = lambda b: re.sub(rb'^\{"spacy_version": "[^"]*"', b"{", b, count=1)
bad = [p.name for p in sorted(ref.glob("*.json")) if unstamp(p.read_bytes()) != unstamp((new / p.name).read_bytes())]
for name in bad:
    print(f"differs: {name}")
print(f"RESULT spacy {spacy.__version__} py{sys.version.split()[0]}: {len(bad)} of {len(list(ref.glob('*.json')))} files differ from {ref}")
sys.exit(1 if bad else 0)
EOF
