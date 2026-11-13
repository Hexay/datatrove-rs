# Sourced by tools/dev.sh and the tokenizer parity scripts. On the shared dev box, CPU-bound runs share
# LIGHT_SLOTS light.lock slots in LIGHT_LOCK_DIR; without that directory (CI, elsewhere) jobs just run.
LIGHT_LOCK_DIR=${LIGHT_LOCK_DIR:-/srv/agents/cache/trafilatura-rs-data}

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
