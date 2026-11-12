# datatrove-rs

See CONTRIBUTING.md for layout, parity and benches.

- Edit loop: `tools/dev.sh build` (profile `iter`, seconds) under heavy.lock, then a parity check; `release` only for
  benches and wheels (`tools/dev.sh bench` installs it).
- `tools/dev.sh test|parity|bench`, `lang_parity.sh` and the tokenizer `parity.sh` take a light.lock slot per run
  (tools/slot.sh). Never wrap a loop of jobs (e.g. languages), or these scripts, in `flock`: a slot held across a loop
  blocks every other worktree.
