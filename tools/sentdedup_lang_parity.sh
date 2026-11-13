#!/usr/bin/env bash
# sentdedup_lang_parity.sh <fineweb2_dir> [n] [key:file...] — sentence dedup parity (tools/parity.py --only SentDedup)
# on each language's FineWeb-2 sample plus fuzzed and sentence-sharing docs; one summary block per language, each
# run taking its own light.lock slot. Reports go to $PARITY_OUT (default ~/.cache/datatrove-rs/out/<worktree dir>).
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
DIR=${1:?usage: sentdedup_lang_parity.sh <fineweb2_dir> [n] [key:file...]}
N=${2:-4000}
shift $(($# < 2 ? $# : 2))
OUT=${PARITY_OUT:-$HOME/.cache/datatrove-rs/out/$(basename "$ROOT")}
mkdir -p "$OUT"
# datatrove language key : sample file (tools/fetch_fineweb2.py names); Thai's sentences fall back, lines run in Rust
ALL="deu_Latn:de fra_Latn:fr rus_Cyrl:ru arb_Arab:ar ell_Grek:el tur_Latn:tr cmn_Hani:cmn jpn_Jpan:ja
  jpn_Jpan:fuzz-ja vie_Latn:vi tha_Thai:ThaiTokenizer hin_Deva:IndicNLPTokenizer-hi ben_Beng:IndicNLPTokenizer-bn
  tam_Taml:IndicNLPTokenizer-ta"
status=0
for g in ${*:-$ALL}; do
  key=${g%%:*} file=${g##*:}
  echo "== $key ($file)"
  "$ROOT/tools/dev.sh" parity "$DIR/$file.parquet" --language "$key" --real "$N" --fuzz 500 --script-fuzz 500 \
    --dup-docs $((N / 2)) --only SentDedup --out "$OUT/sentdedup-$file.txt" 2>/dev/null |
    awk '/^SentDedup|docs:|^PASS|^FAIL|^note/' || status=1
done
exit $status
