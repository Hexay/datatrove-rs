#!/usr/bin/env bash
# wordtok_parity.sh <fineweb2_dir> [n] [fuzz] [group...] — wordtok_parity.py over the FineWeb-2 samples of each
# ported tokenizer (default: all), one summary block per language, each taking its own light.lock slot.
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
. "$ROOT/tools/slot.sh"
DIR=${1:?usage: wordtok_parity.sh <fineweb2_dir> [n] [fuzz] [group...]}
N=${2:-4000}
FUZZ=${3:-1000}
shift $(($# < 3 ? $# : 3))
# datatrove language key : sample file (tools/fetch_fineweb2.py names)
ALL="vie_Latn:vi tha_Thai:ThaiTokenizer"
for c in hi ur bn ta te mr ml kn gu pa ne or si as sa kK; do
  key=$("$ROOT/.venv/bin/python" -c "
from functools import partial
from datatrove.utils.word_tokenizers import load_tokenizer_assignments as a
print(next(k for k, f in a().items() if isinstance(f, partial) and f.args == ('IndicNLPTokenizer', '$c') and '_' in k))")
  ALL="$ALL $key:IndicNLPTokenizer-$c"
done
SETS=${*:-$ALL}
status=0
for g in $SETS; do
  in_light_slot "$ROOT/.venv/bin/python" -W ignore "$ROOT/tools/wordtok_parity.py" "${g%%:*}" "$DIR/${g##*:}.bin" \
    --n "$N" --fuzz "$FUZZ" 2>&1 || status=1
done
exit $status
