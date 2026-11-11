# Contributing

## Layout

```
crates/spacy-tokenizer   spaCy English tokenizer port (prefix/suffix/infix rules, specials, URL matcher, chunk cache)
crates/datatrove-core    filters, MinHash, Python-compatible str/Unicode helpers, `profile` binary
crates/datatrove-py      PyO3 bindings (datatrove_rs._core)
python/datatrove_rs      datatrove subclasses, patch/mirror/env-var adoption, fallback logic
tools/                   dev.sh, parity/bench harnesses, fuzzers, Unicode table exporter
tests/                   pytest parity suite
docs/                    parity notes, FineWeb profile, README assets
```

## Setup

```bash
uv venv -p 3.13 .venv && uv pip install -p .venv/bin/python maturin -e ".[test,trafilatura]"
tools/dev.sh build                 # maturin develop --release
tools/dev.sh test                  # pytest
```

Use Python 3.13 so the running Unicode version matches the shipped tables. To test against an upstream checkout,
add `-e <path-to-datatrove>` to the install. After upgrading Python or `regex`, re-export the Unicode tables with
`tools/export_pytext.py`.

## Parity and benchmarks

```bash
tools/dev.sh parity <fineweb.parquet> ...             # large parity run, see docs/parity.md
tools/dev.sh bench <fineweb.parquet> [n] [reps]       # docs/s per step vs datatrove
tools/icount.sh <docs.bin> <stage>...                 # instructions + cycles/doc per `profile` stage
crates/spacy-tokenizer/tools/parity.sh <docs.bin>     # tokenizer vs spaCy, token spans
```

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
