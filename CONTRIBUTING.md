# Contributing

## Layout

```
crates/spacy-tokenizer   spaCy rule-based tokenizer port, 57 languages (affix rules, specials, URL matcher, chunk cache)
crates/cjk-tokenizer     jieba port (zh) and sudachi.rs wrapper (ja), with datatrove/spaCy's doc alignment
crates/datatrove-core    filters, MinHash, Python-compatible str/Unicode helpers, `profile` binary;
                         `src/wordtok`: Vietnamese (pyvi), Thai (newmm) and Indic NLP tokenizer ports
crates/datatrove-py      PyO3 bindings (datatrove_rs._core)
python/datatrove_rs      datatrove subclasses, patch/mirror/env-var adoption, fallback logic
tools/                   dev.sh, parity/bench harnesses, fuzzers, Unicode table exporter
tests/                   pytest parity suite
docs/                    parity notes, FineWeb profile, README assets
```

## Setup

```bash
uv venv -p 3.13 .venv && uv pip install -p .venv/bin/python maturin -e ".[test,trafilatura]"
tools/dev.sh build                 # maturin develop, profile iter
tools/dev.sh test                  # pytest
```

`tools/dev.sh build` and the tokenizer parity scripts build with `PROFILE` (default `iter`: no LTO, 16 codegen
units, incremental for the workspace crates, so an edit rebuilds in seconds; same output). Benches and wheels need
`release`: `tools/dev.sh bench` installs it itself, `tools/icount.sh` builds it. Leave `CARGO_INCREMENTAL` alone:
the scripts unset it for `iter` so dependencies stay sccache-cached. `cargo test --profile iter` runs the Rust tests.

Use Python 3.13 so the running Unicode version matches the shipped tables. To test against an upstream checkout,
add `-e <path-to-datatrove>` to the install. After upgrading Python or `regex`, re-export the Unicode tables with
`tools/export_pytext.py`.

## Parity and benchmarks

```bash
tools/dev.sh parity <fineweb.parquet> ...             # large parity run, see docs/parity.md
tools/dev.sh bench <fineweb.parquet> [n] [reps]       # docs/s per step vs datatrove
tools/icount.sh <docs.bin> <stage>...                 # instructions + cycles/doc per `profile` stage
crates/spacy-tokenizer/tools/parity.sh <docs.bin>     # tokenizer vs spaCy, token spans
crates/spacy-tokenizer/tools/lang_parity.sh <lang> <docs.bin>  # one language: affixes, real and fuzzed docs
crates/cjk-tokenizer/tools/parity.sh <zh|ja> <docs.bin>       # jieba/Sudachi port vs datatrove's tokenizer
tools/cjk_fuzz.py <out.bin> <n> <zh|ja>                        # adversarial Chinese/Japanese docs
tools/wordtok_parity.sh <fineweb2 dir> [n] [fuzz] [set:Tokenizer]... # vi/Thai/Indic ports vs datatrove's tokenizers
tools/stats_parity.py <parquet> [--language KEY]      # stats steps: files, merged files, metadata, stats
```

`tools/dev.sh parity` caches the datatrove side in `$DATATROVE_RS_PARITY_CACHE` (default
`/srv/agents/cache/parity-cache` when present, else `~/.cache/datatrove-rs/parity`), keyed by corpus, config and the
datatrove and spaCy versions. The tokenizer scripts cache spaCy's dumps next to `docs.bin`, each with a
`.spacy-version` stamp (redone when spaCy changes), and write the Rust dumps to `$SPACY_PARITY_OUT` (default
`~/.cache/datatrove-rs/out/<worktree dir>`), so parallel worktrees don't overwrite each other's.

On the shared box, `tools/dev.sh test|parity|bench` and each `lang_parity.sh`/`parity.sh` run take one of the two
light.lock slots in `$LIGHT_LOCK_DIR` (`tools/slot.sh`) for that run only. Never wrap a loop of runs, or these
scripts, in `flock`: a slot held across a loop blocks every other worktree for its whole length.

After upgrading spaCy, re-export the rules with `crates/spacy-tokenizer/tools/export_rules.py tables` and
`export_rules.py <lang>...`, then rerun `lang_parity.sh` for each language. To follow a new SudachiPy release, bump
the `sudachi` tag in `crates/cjk-tokenizer/Cargo.toml`, `SUDACHI_VERSION` and the test pin, then rerun
`cjk-tokenizer/tools/parity.sh ja`. To follow pythainlp, indic-nlp-library or pyvi, re-export with
`tools/export_wordtok.py` / `tools/export_vi.py`, rerun `tools/wordtok_parity.sh`, then add the version to
`VERIFIED` in `python/datatrove_rs/_wordtok.py` and bump the test pin. For a new datatrove release, rerun
`tools/stats_parity.py` and add it to `VERIFIED_DATATROVE` in `python/datatrove_rs/_stats_run.py`. `PROFILE_LANG=thai|vi|indic:<code>` profiles a
port with the `profile` binary.

`docs.bin` is UTF-8 docs joined by `\0`. Wall time is unreliable on a loaded machine: A/B Rust changes with
`tools/icount.sh` (instruction counts are load-independent) and confirm wall time by alternating old/new binaries.

Where the time goes: the spaCy-compatible tokenizer dominates (~45 µs/doc streamed). It runs about three times per
doc in the chain (whole doc, C4's per-line sentence split, FineWeb on C4's rewritten text, mostly chunk-cache hits).
All filters on a thread share one tokenizer state and the last doc's tokens (`words.rs`).

## CI

`.github/workflows/ci.yml` runs `cargo fmt --check`, `clippy -D warnings`, `cargo test --release` and the pytest
suite on Python 3.13 against datatrove 0.10.1 and the latest 0.10.x, with trafilatura-rs installed from PyPI.

## Releasing

1. Bump `version` in `pyproject.toml` (the published version; crate versions don't matter) and merge to `master`.
2. Tag and push: `git tag vX.Y.Z && git push origin vX.Y.Z`. The tag must equal `v` + the pyproject version.
3. `.github/workflows/release.yml` builds abi3 wheels (Linux x86_64/aarch64 manylinux_2_28, macOS x86_64/arm64,
   Windows x64) and an sdist, import-tests each, and publishes to PyPI by trusted publishing (environment `pypi`).
