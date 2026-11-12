# Sourced by parity.sh and lang_parity.sh.
#   PROFILE  cargo profile of the bins (default iter, Cargo.toml)
#   OUT      Rust dumps, per worktree (references stay cached next to docs.bin, shared)
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
REPO=$(cd "$ROOT/../.." && pwd)
PY=${PY:-$REPO/.venv/bin/python}
PROFILE=${PROFILE:-iter}
OUT=${SPACY_PARITY_OUT:-${XDG_CACHE_HOME:-$HOME/.cache}/datatrove-rs/out/$(basename "$REPO")}
mkdir -p "$OUT"
export PYTHONIOENCODING=utf-8
SPACY_VERSION=$("$PY" -c 'from importlib.metadata import version; print(version("spacy"))')

# build_bins: once per top-level run; sets BIN, the bins' directory.
build_bins() {
  if [ -z "${SPACY_PARITY_BIN:-}" ]; then
    [ "$PROFILE" = iter ] && unset CARGO_INCREMENTAL  # see tools/dev.sh
    cargo build -q --profile "$PROFILE" -j "${CARGO_BUILD_JOBS:-2}" --manifest-path "$REPO/Cargo.toml" \
      -p spacy-tokenizer-rs --bins || return 2
    local target
    target=$(cargo metadata -q --format-version 1 --no-deps --manifest-path "$REPO/Cargo.toml" \
      | "$PY" -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
    export SPACY_PARITY_BIN=$target/$PROFILE
  fi
  BIN=$SPACY_PARITY_BIN
}

# cached <file> <cmd with {} for the output path>...: (re)make a reference file unless it exists and was made with
# this spaCy version (<file>.spacy-version; a file without one is adopted). Atomic, so parallel runs can share it.
cached() {
  local f=$1 tmp
  shift
  if [ -f "$f" ]; then
    [ -f "$f.spacy-version" ] || echo "$SPACY_VERSION" > "$f.spacy-version"
    [ "$(cat "$f.spacy-version")" = "$SPACY_VERSION" ] && return 0
  fi
  tmp=$f.tmp.$$
  "${@//\{\}/$tmp}" && echo "$SPACY_VERSION" > "$f.spacy-version" && mv "$tmp" "$f"
}
