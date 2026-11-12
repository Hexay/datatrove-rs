#!/usr/bin/env bash
# dev.sh build|test|parity|bench [args] — run from anywhere; uses the repo's .venv.
#   build   maturin develop with cargo profile PROFILE (default iter: fast rebuilds; release for benches and wheels)
#   parity  tools/parity.py, datatrove side cached in DATATROVE_RS_PARITY_CACHE
#   bench   installs a release build first
# test, parity and bench each take a light.lock slot for themselves (tools/slot.sh): don't wrap them in flock.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
export PATH="$HOME/.cargo/bin:$PATH" PYTHONIOENCODING=utf-8 PYTHONUNBUFFERED=1 VIRTUAL_ENV="$ROOT/.venv"
PY="$ROOT/.venv/Scripts/python.exe"
[ -x "$PY" ] || PY="$ROOT/.venv/bin/python"
cache=/srv/agents/cache/parity-cache
[ -d "${cache%/*}" ] || cache=$HOME/.cache/datatrove-rs/parity
export DATATROVE_RS_PARITY_CACHE=${DATATROVE_RS_PARITY_CACHE:-$cache}
cmd=${1:?usage: dev.sh build|test|parity|bench [args]}
shift
build() {
  local profile=${PROFILE:-iter}
  # iter's `incremental = true` covers workspace crates only, deps stay sccache-cached; sccache rejects the env var
  [ "$profile" = iter ] && unset CARGO_INCREMENTAL
  (cd "$ROOT" && "$PY" -m maturin develop --profile "$profile" -q "$@")
}
. "$ROOT/tools/slot.sh"
case $cmd in
  build) build "$@" ;;
  test) in_light_slot "$PY" -m pytest -q "$ROOT/tests" "$@" ;;
  parity) in_light_slot "$PY" "$ROOT/tools/parity.py" "$@" ;;
  bench) PROFILE=release build && in_light_slot "$PY" "$ROOT/tools/bench.py" "$@" ;;
  *) echo "unknown command: $cmd" >&2; exit 2 ;;
esac
