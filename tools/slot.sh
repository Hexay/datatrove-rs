# Sourced by tools/dev.sh and the tokenizer parity scripts. On the shared dev box, CPU-bound runs share
# LIGHT_SLOTS light.lock slots in LIGHT_LOCK_DIR; without that directory (CI, elsewhere) jobs just run.
LIGHT_LOCK_DIR=${LIGHT_LOCK_DIR:-/srv/agents/cache/trafilatura-rs-data}

# in_heavy_lock cmd...: run a compile holding heavy.lock (one build at a time across worktrees, -j 2); nested calls
# run directly. Don't also wrap tools/dev.sh in `flock heavy.lock`: a second flock on the same file deadlocks.
in_heavy_lock() {
  if [ -n "${HEAVY_LOCK_HELD:-}" ] || [ ! -d "$LIGHT_LOCK_DIR" ]; then
    "$@"
    return
  fi
  HEAVY_LOCK_HELD=1 CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2} flock "$LIGHT_LOCK_DIR/heavy.lock" "$@"
}

# in_light_slot cmd...: run one job holding whichever light.lock.N (N < LIGHT_SLOTS) frees first; nested calls
# run directly. Take it per job, never around a loop of jobs: a held slot blocks every other worktree.
in_light_slot() {
  local s fd rc
  if [ -n "${LIGHT_SLOT_HELD:-}" ] || [ ! -d "$LIGHT_LOCK_DIR" ]; then
    "$@"
    return
  fi
  while :; do
    for ((s = 0; s < ${LIGHT_SLOTS:-3}; s++)); do
      exec {fd}>"$LIGHT_LOCK_DIR/light.lock.$s"
      if flock -w 2 "$fd"; then
        LIGHT_SLOT_HELD=1 "$@"
        rc=$?
        exec {fd}>&-
        return $rc
      fi
      exec {fd}>&-
    done
  done
}
