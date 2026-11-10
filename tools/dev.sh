#!/usr/bin/env bash
# dev.sh build|test|parity|bench [args] — run from anywhere; uses the repo's .venv.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
export PATH="$HOME/.cargo/bin:$PATH" PYTHONIOENCODING=utf-8 PYTHONUNBUFFERED=1 VIRTUAL_ENV="$ROOT/.venv"
PY="$ROOT/.venv/Scripts/python.exe"
[ -x "$PY" ] || PY="$ROOT/.venv/bin/python"
cmd=${1:?usage: dev.sh build|test|parity|bench [args]}
shift
case $cmd in
  build) (cd "$ROOT" && "$PY" -m maturin develop --release -q "$@") ;;
  test) "$PY" -m pytest -q "$ROOT/tests" "$@" ;;
  parity) "$PY" "$ROOT/tools/parity.py" "$@" ;;
  bench) "$PY" "$ROOT/tools/bench.py" "$@" ;;
  *) echo "unknown command: $cmd" >&2; exit 2 ;;
esac
