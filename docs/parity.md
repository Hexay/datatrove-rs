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
- `tests/test_warc_reader.py` compares `WarcReader` output (documents, ids, metadata, stats, raised exception)
  on hand-built WARCs: every record type, missing payload types (libmagic), non-UTF-8 payloads (cchardet), chunked
  and gzip HTTP bodies, header continuations, broken framing, corrupt, truncated and padded gzip, `limit`/`skip`.
  With `DATATROVE_RS_WARC=<file.warc.gz> DATATROVE_RS_WARC_LIMIT=1000000` it reads a whole Common Crawl file:
  `CC-MAIN-20240220211055-20240221001055-00000.warc.gz` (1.1 GB) gives identical output.
- `tests/test_url_lang.py` compares `URLFilter` (decisions, reasons, stats; the Rust host split against tldextract
  on ~2,800 URLs built from the suffix list, with and without PSL private domains) and `LanguageFilter` (metadata,
  decisions, stats across six configs).

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
- **WarcReader.** Rust reproduces fsspec's gzip stream plus warcio's `ArchiveIterator` and datatrove's
  `process_record` for well-formed records (`crates/datatrove-core/src/warc`). A selected record datatrove might
  treat specially (no `WARC-Identified-Payload-Type`, a non-UTF-8 payload, HTTP `Content-Encoding` or
  `Transfer-Encoding`, missing URL or date, non-ASCII header names) is handed to datatrove's `process_record` as raw
  bytes. Anything that could make warcio frame records differently (bad status line, `Content-Length`, record
  trailer, URI rewriting) or any gzip error hands the rest of the file to datatrove's loop from that record on.
  Python's gzip and warcio read ahead, so on a damaged file datatrove stops or raises while still on an earlier
  record; Rust returns a record only after the next 4 MB decompressed cleanly, so it hands over early enough for
  the output and the raised error to match. Only gzip input (`compression="infer"` with `.gz`, or `"gzip"`) takes
  the Rust path; the time stat counts records yielded, not every record read.
- **URLFilter.** The blocklists are read from the same extracted files with Python's text-mode line semantics
  (universal newlines, `#` lines skipped, `str.strip()`); a sorted list is binary-searched in place. The host split
  reproduces tldextract 5.4 (`lenient_netloc`, the suffix trie with wildcards, exceptions and private domains) from
  the suffix lists tldextract itself loads; non-ASCII, bracketed and examined `xn--` hosts, and other tldextract
  versions, go through tldextract. Needs `use_integrated_lists=True` and a UTF-8 locale, else datatrove's code runs.
- **LanguageFilter.** fastText is asked for labels scoring at least half of `language_threshold` instead of all
  176 (see `python/datatrove_rs/language_filter.py`); scores don't depend on the threshold, so the best label and
  every requested language that could pass are the same. Datatrove's full prediction runs when the top label is
  tied or within 0.1% of that floor, with `keep_top_pairs_threshold`, or a non-positive threshold.
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
