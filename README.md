# datatrove-rs

Drop-in Rust implementations of [datatrove](https://github.com/huggingface/datatrove)'s FineWeb filter chain and
MinHash stage 1, for English. The outputs match datatrove's exactly: the same keep/drop decisions, drop reasons,
rewritten `doc.text`, stats and signature files.

| datatrove step | Rust-backed replacement |
|---|---|
| `pipeline.filters.GopherRepetitionFilter` | `datatrove_rs.GopherRepetitionFilter` |
| `pipeline.filters.GopherQualityFilter` | `datatrove_rs.GopherQualityFilter` |
| `pipeline.filters.C4QualityFilter` | `datatrove_rs.C4QualityFilter` |
| `pipeline.filters.FineWebQualityFilter` | `datatrove_rs.FineWebQualityFilter` |
| `pipeline.dedup.MinhashDedupSignature` | `datatrove_rs.MinhashDedupSignature` |
| `pipeline.extractors.Trafilatura` | `datatrove_rs.extractors.Trafilatura`, with the `[trafilatura]` extra |

The Trafilatura step runs [trafilatura-rs](https://github.com/Hexay/trafilatura-rs), a byte-exact Rust port of
trafilatura 1.11 (`pip install "datatrove-rs[trafilatura]"`). Without it, datatrove's own Trafilatura runs. Configs
trafilatura-rs doesn't cover fall back to Python per step. On the FineWeb recipe, where Trafilatura is ~85% of CPU,
this takes the whole stage from 87.7 s to 17.3 s on 5,000 Common Crawl records (5.1x, identical output; see
`docs/fineweb-profile.md`).

Each class subclasses the datatrove step and takes the same arguments. `filter()` (MinHash: `get_shingles` /
`get_signature`) is replaced. The filters also override `run()` with an inlined copy of `BaseFilter.run()` whose stat
bookkeeping is cheaper but bit-for-bit identical (see `python/datatrove_rs/_run.py`); a subclass that overrides
`filter`, `filter_batch`, `stat_update`, `update_doc_stats` or `track_time` gets datatrove's `run()` instead.

## Adopting

`pip install datatrove-rs`, then pick one; every other datatrove step keeps running datatrove's own code.

1. **No code changes.** Set `DATATROVE_RS=1` (an installed `.pth` hook calls `patch()` at interpreter startup, so
   executor worker processes get it too), or run the unmodified script with `python -m datatrove_rs pipeline.py`.
2. **One line.** `import datatrove_rs; datatrove_rs.patch()` before building the pipeline. datatrove's own import
   paths (`datatrove.pipeline.filters.GopherQualityFilter`, ...) then hand out the Rust classes; `unpatch()` undoes it.
3. **Explicit imports, no patching.** `datatrove_rs.<path>` mirrors every `datatrove.<path>` module with the Rust
   classes swapped in, so replacing `datatrove.` with `datatrove_rs.` in the imports is enough:

```python
from datatrove_rs.pipeline.filters import GopherQualityFilter, URLFilter  # Rust, datatrove's own
from datatrove_rs.pipeline.readers import WarcReader                       # datatrove's own
```

The Rust classes keep `__module__ = "datatrove_rs.*"`, so pickled pipelines (Slurm, local multiprocessing) load them
on workers whether or not the worker patched. Unsupported languages fall back per step, as described below.

Requires datatrove `>=0.10.1,<0.11` and Python 3.10+ (one abi3 wheel). English needs no spaCy install; spaCy is
only used for the non-English fallback and, when installed, to reproduce the `IS_ALPHA` quirk below.

## Speed

Single thread, 1,000 FineWeb docs (3.0 MB), median of 3 runs (`tools/dev.sh bench <fineweb.parquet> 1000 3`),
Ryzen 7 9700X, datatrove 0.10.1, Python 3.13:

| Step | datatrove docs/s | datatrove_rs docs/s | Speedup |
|---|---|---|---|
| GopherRepetition | 195 | 16,987 | 87x |
| GopherQuality | 267 | 21,087 | 79x |
| C4Quality | 268 | 13,119 | 49x |
| FineWebQuality | 268 | 23,063 | 86x |
| Chain (`run()`, all 4 filters) | 99 | 6,831 | 69x |
| MinHash stage 1 | 269 | 12,780 | 48x |

The machine was under heavy load from other processes, so absolute numbers swing between runs; compare the ratio.
All filters on a thread share one tokenizer state and the last doc's tokens (see `words.rs`).

Where the time goes (`profile <docs.bin> [reps] [stage]`, `tools/icount.sh`): the spaCy-compatible tokenizer still
dominates (~45 µs/doc streamed). It runs about three times per doc in the chain: whole doc, C4's per-line sentence
split, and FineWeb on C4's rewritten text (mostly chunk-cache hits). Python `run()` plumbing is about
60% cheaper than datatrove's `BaseFilter.run()` (`tools/profile_chain.py <fineweb.parquet>`: 99k vs 249k Python calls
for the 4-step chain over 1,000 docs).

## Parity

Every check compares against datatrove running unmodified in the same environment.

- `tools/dev.sh parity <fineweb.parquet> --real 6000 --fuzz 3000 --random-configs 3` runs 10,500 docs: 6,000 real
  FineWeb docs, 1,500 perturbed copies and 3,000 fuzzed docs (Unicode edge cases, separators, repetition). It covers
  9 fixed filter configs, 12 random-threshold configs and 5 MinHash configs (xxhash 32/64, sha1 32/64, 3-gram with
  4x6 buckets, dates/raw normalisation). It compares:
  - the per-doc result, or the raised exception type;
  - the rewritten `doc.text`;
  - the `run()` output, every stat field (`repr`, so int vs float counts), the time-stat count and unit, and what
    the exclusion writer receives, at batch sizes 1 to 5;
  - the MinHash signature files, byte for byte.

  Last run (`--real 4000 --fuzz 2000 --random-configs 3`): **0 mismatches across all 26 cases.** Pass `--workers N --cache DIR` to parallelise and cache the
  datatrove side.
- `tools/dev.sh test` (pytest) runs the parity cases on a smaller corpus. It also covers `simplify_text`, word
  splitting and sentence splitting on 2,500 random Unicode strings across 17 normalisation configs, the fallback for
  non-English languages, and pickling.

### How exactness is kept, and its limits

- **Unicode tables.** Python's `str.isspace`/`isalpha`/`lower`, `unicodedata` NFD and categories, `re`'s `\d`/`\s`,
  and the `regex` module's `\p{Nd}` are exported from Python into `crates/datatrove-core/data/py_unicode.json` by
  `tools/export_pytext.py`. The shipped tables come from Python 3.13 (Unicode 15.1). On import, the package warns if
  the running Python uses a different Unicode version, since results could then differ on rare characters.
- **Language.** The Rust path runs only when datatrove would use a plain `spacy.blank("en")` tokenizer. That covers
  `eng` and every language code datatrove maps to it. Other languages run datatrove's Python code unchanged.
- **The `IS_ALPHA` token.** Inside `memory_zone`, spaCy 3.8 raises an error on the token `IS_ALPHA` once the same
  process has seen that token before, and datatrove then retokenizes the text without it. The result depends on
  process history, so when spaCy is installed any doc containing `IS_ALPHA` runs through datatrove itself.
  Without spaCy, datatrove could not run at all and Rust handles these docs as spaCy would on first sight.
- **Reproduced quirks.**
  - `Counter.most_common` tie-breaking;
  - the `"".join` n-gram keys in `find_all_duplicate`;
  - `ZeroDivisionError` on degenerate docs;
  - the one-sentence result for empty text;
  - numpy `uint64` wrap-around in the MinHash permutation.

### datatrove 0.10.0 issues found along the way

- `utils/text.py` imports `regex` without declaring it (this package depends on `regex` for that reason).
- The Python MinHash passes `str` to `xxhash`, which xxhash 4 rejects. The reference side needs `xxhash<4`; this
  package's MinHash is unaffected.
- `importlib.metadata` is used without being imported, and `pipeline.dedup` requires `tokenizers`. For the latter,
  `datatrove_rs.MinhashDedupSignature` is imported lazily, so filter-only installs don't need `tokenizers`.

## Layout

```
crates/spacy-tokenizer   spaCy English tokenizer port (prefix/suffix/infix rules, specials, URL matcher, chunk cache)
crates/datatrove-core    filters, MinHash, Python-compatible str/Unicode helpers, `profile` binary
crates/datatrove-py      PyO3 bindings (datatrove_rs._core)
python/datatrove_rs      datatrove subclasses and fallback logic
tools/                   dev.sh, parity/bench harnesses, fuzzers, Unicode table exporter
tests/                   pytest parity suite
```

## Development

```
uv venv -p 3.13 .venv && uv pip install -p .venv/bin/python maturin -e ".[test]"
tools/dev.sh build                 # maturin develop --release
tools/dev.sh test                  # pytest
tools/dev.sh parity <parquet> ...  # large parity run
tools/dev.sh bench <parquet> [n] [reps]
tools/icount.sh <docs.bin> <stage>...                  # instructions + cycles/doc per `profile` stage
crates/spacy-tokenizer/tools/parity.sh <docs.bin>      # tokenizer vs spaCy, token spans
```

Use Python 3.13 so the running Unicode version matches the shipped tables. To test against an upstream checkout,
add `-e <path-to-datatrove>` to the install. `docs.bin` is UTF-8 docs joined by `\0`.

Wall time is unreliable on a loaded machine; A/B Rust changes with `tools/icount.sh` (instruction counts are
load-independent) and confirm wall time by alternating old/new binaries.

After upgrading Python or `regex`, re-export the Unicode tables with `tools/export_pytext.py`.

Rust code is formatted with the repo's `rustfmt.toml`; CI (`.github/workflows/ci.yml`) runs `cargo fmt --check`,
`clippy -D warnings`, `cargo test --release` and the pytest suite on Python 3.13 against datatrove 0.10.1 and the
latest 0.10.x.

## Releasing

1. Bump `version` in `pyproject.toml` (the published version; the crate versions don't matter) and merge to `master`.
2. Tag and push: `git tag v0.1.0 && git push origin v0.1.0`. The tag must equal `v` + the pyproject version.
3. `.github/workflows/release.yml` builds abi3 wheels (Linux x86_64/aarch64 manylinux_2_28, macOS x86_64/arm64,
   Windows x64) and an sdist, import-tests each, and publishes to PyPI by trusted publishing.

One-time setup:

- PyPI: on https://pypi.org/manage/account/publishing/ add a pending publisher (or, once the project exists,
  Manage project > Publishing): PyPI project name `datatrove-rs`, owner `Hexay`, repository `datatrove-rs`, workflow
  `release.yml`, environment `pypi`.
- GitHub: Settings > Environments > New environment `pypi` (optionally with required reviewers and a `v*` tag rule).
