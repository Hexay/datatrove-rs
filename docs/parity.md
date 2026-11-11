# Parity with datatrove

Every check compares against datatrove running unmodified in the same environment.

## Checks

- `tools/dev.sh parity <fineweb.parquet> --real 6000 --fuzz 3000 --random-configs 3` runs 10,500 docs: 6,000 real
  FineWeb docs, 1,500 perturbed copies and 3,000 fuzzed docs (Unicode edge cases, separators, repetition). It covers
  9 fixed filter configs, 12 random-threshold configs and 5 MinHash configs (xxhash 32/64, sha1 32/64, 3-gram with
  4x6 buckets, dates/raw normalisation). It compares:
  - the per-doc result, or the raised exception type;
  - the rewritten `doc.text`;
  - the `run()` output, every stat field (`repr`, so int vs float counts), the time-stat count and unit, and what
    the exclusion writer receives, at batch sizes 1 to 5;
  - the MinHash signature files, byte for byte.

  Pass `--workers N --cache DIR` to parallelise and cache the datatrove side.
- `tools/dev.sh test` (pytest) runs the parity cases on a smaller corpus. It also covers `simplify_text`, word
  splitting and sentence splitting on 2,500 random Unicode strings across 17 normalisation configs, the fallback for
  non-English languages, pickling, and the Trafilatura step when trafilatura-rs is installed.
- `crates/spacy-tokenizer/tools/parity.sh <docs.bin>` compares tokenizer spans against spaCy.

## How exactness is kept, and its limits

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
- **`run()`.** The filters override `run()` with an inlined copy of `BaseFilter.run()` whose stat bookkeeping is
  cheaper but bit-for-bit identical (`python/datatrove_rs/_run.py`). A subclass that overrides `filter`,
  `filter_batch`, `stat_update`, `update_doc_stats` or `track_time` gets datatrove's `run()` instead.
- **Trafilatura.** Only `extract()` is replaced, so datatrove's sandbox, timeout and stats are unchanged. Configs
  outside `trafilatura_rs.VERIFIED` run Python trafilatura for the whole step, so trafilatura's process-global dedup
  cache never mixes the two engines.
- **Reproduced quirks.** `Counter.most_common` tie-breaking; the `"".join` n-gram keys in `find_all_duplicate`;
  `ZeroDivisionError` on degenerate docs; the one-sentence result for empty text; numpy `uint64` wrap-around in the
  MinHash permutation.

## datatrove 0.10.x issues found along the way

- `utils/text.py` imports `regex` without declaring it (this package depends on `regex` for that reason).
- The Python MinHash passes `str` to `xxhash`, which xxhash 4 rejects. The reference side needs `xxhash<4`; this
  package's MinHash is unaffected.
- `importlib.metadata` is used without being imported, and `pipeline.dedup` requires `tokenizers`. For the latter,
  `datatrove_rs.MinhashDedupSignature` is imported lazily, so filter-only installs don't need `tokenizers`.
- The Trafilatura step's 1 s wall-clock sandbox timeout makes the set of extracted docs load-dependent in datatrove
  itself (4,610 vs 4,611 of 4,956 on identical runs); compare extractor output in-process, not through the sandbox.
