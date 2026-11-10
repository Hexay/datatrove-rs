# datatrove-rs

Drop-in Rust implementations of [datatrove](https://github.com/huggingface/datatrove)'s FineWeb filter chain and
MinHash stage 1, for English. The outputs match datatrove's exactly: the same keep/drop decisions, drop reasons,
rewritten `doc.text`, stats and signature files.

| datatrove step | datatrove_rs replacement |
|---|---|
| `filters.GopherRepetitionFilter` | `datatrove_rs.GopherRepetitionFilter` |
| `filters.GopherQualityFilter` | `datatrove_rs.GopherQualityFilter` |
| `filters.C4QualityFilter` | `datatrove_rs.C4QualityFilter` |
| `filters.FineWebQualityFilter` | `datatrove_rs.FineWebQualityFilter` |
| `dedup.MinhashDedupSignature` | `datatrove_rs.MinhashDedupSignature` |

Each class subclasses the datatrove step and takes the same arguments. Only `filter()` (MinHash: `get_shingles` /
`get_signature`) is replaced, so `run()`, stats, writers and executors behave exactly as in datatrove.

## Usage

```python
from datatrove_rs import (C4QualityFilter, FineWebQualityFilter, GopherQualityFilter,
                          GopherRepetitionFilter, MinhashDedupSignature)
```

Swap the imports in an existing pipeline; nothing else changes.

Requires datatrove `>=0.10,<0.11` and Python 3.10+ (one abi3 wheel). spaCy is still needed for the fallback paths
listed below.

## Speed

Single thread, 1,000 FineWeb docs (3.0 MB), median of 3 runs (`tools/dev.sh bench <fineweb.parquet> 1000 3`):

| Step | datatrove docs/s | datatrove_rs docs/s | Speedup |
|---|---|---|---|
| GopherRepetition | 50 | 2,376 | 48x |
| GopherQuality | 82 | 3,961 | 49x |
| C4Quality | 65 | 2,237 | 35x |
| FineWebQuality | 52 | 3,403 | 66x |
| Chain (`run()`, all 4 filters) | 20 | 707 | 36x |
| MinHash stage 1 | 55 | 1,964 | 36x |

The machine was under load from other processes, so absolute numbers swing a lot between runs. Across every run the
speedup stayed between 30x and 70x. The chain tokenizes each doc once and shares the result across steps (see `words.rs`).

Where the time goes (`cargo run --release -p datatrove-core --bin profile -- <docs.bin>`): the spaCy-compatible tokenizer is about
120 µs/doc and dominates. It is limited by per-chunk hash lookups, not by regexes. The Python `run()` plumbing adds
about 50 µs/doc for the 4-step chain.

## Parity

Every check compares against datatrove running unmodified in the same environment.

- `tools/dev.sh parity <fineweb.parquet> --real 6000 --fuzz 3000 --random-configs 3` runs 10,500 docs: 6,000 real
  FineWeb docs, 1,500 perturbed copies and 3,000 fuzzed docs (Unicode edge cases, separators, repetition). It covers
  7 fixed filter configs, 12 random-threshold configs and 5 MinHash configs (xxhash 32/64, sha1 32/64, 3-gram with
  4x6 buckets, dates/raw normalisation). It compares:
  - the per-doc result, or the raised exception type;
  - the rewritten `doc.text`;
  - the `run()` output and every stat;
  - the MinHash signature files, byte for byte.

  Last run: **0 mismatches across all 24 cases.** Pass `--workers N --cache DIR` to parallelise and cache the
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
  process history, so any doc containing `IS_ALPHA` runs through datatrove itself.
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
py -3 -m venv .venv && .venv/Scripts/python -m pip install maturin -e ".[test]"
tools/dev.sh build                 # maturin develop --release
tools/dev.sh test                  # pytest
tools/dev.sh parity <parquet> ...  # large parity run
tools/dev.sh bench <parquet> [n] [reps]
```

After upgrading Python or `regex`, re-export the Unicode tables with `tools/export_pytext.py`.
