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
  splitting and sentence splitting on 2,500 random Unicode strings across 17 normalisation configs, per-language
  words, sentences, GopherRepetition, C4 and MinHash on rule-key fuzz docs (`tests/test_languages.py`), the fallback
  for languages without a Rust tokenizer, pickling, and the Trafilatura step when trafilatura-rs is installed.
- `crates/spacy-tokenizer/tools/parity.sh <docs.bin> [n] [split|fancy] [lang]` compares tokenizer spans against
  spaCy. `tools/lang_parity.sh <lang> <docs.bin>` runs one language's full tokenizer check: every prefix/suffix/infix,
  `token_match` and `url_match` decision on the language's rule keys, case variants and native letters
  (`affix_ref.py`), real docs, and fuzzed docs (`fuzz_docs.py`). `tools/fetch_fineweb2.py` samples the FineWeb-2
  subsets datatrove maps to each spaCy language.
- All 57 rule-based spaCy languages pass with 0 differences: 25.9M affix strings, 221,112 real docs (134.7M tokens,
  ~218,000 of them FineWeb-2 in 56 languages) and 171,000 fuzzed docs (18.6M tokens). `tools/parity.py --language
  <code>` then gives 0 filter and MinHash mismatches in all 56 non-English languages (81,825 docs, 50,100 real).
- Chinese and Japanese: `crates/cjk-tokenizer/tools/parity.sh <zh|ja> <docs.bin>` compares doc token spans with
  datatrove's own `SpaCyTokenizer`. 0 differences on 13,892 FineWeb-2 Chinese docs (cmn, wuu, yue, hak; 10.7M
  tokens), 8,000 Japanese docs (6.65M tokens) and 3,000 fuzzed docs per language (`tools/cjk_fuzz.py`; 200 Japanese
  ones span several 40,000-byte chunks). `tools/parity.py --language cmn_Hani|jpn_Jpan` on those corpora gives 0
  filter and MinHash mismatches: 22,184 Chinese and 11,021 Japanese docs, all 18 configs. `tests/test_cjk.py`
  repeats a small version in CI.
- Vietnamese, Thai and Indic: `tools/wordtok_parity.sh <fineweb2 dir> [n] [fuzz] [set:Tokenizer]...` compares words
  and per-line sentence counts (whole-doc sentences for Vietnamese) with datatrove's tokenizer. 0 differences on
  70,000 FineWeb-2 docs (4,000 per set, 2,000 for Konkani; 37.9M tokens) and 18,000 fuzzed docs (`tools/wordtok_fuzz.py`,
  20.3M tokens) over 18 sets: vie_Latn, tha_Thai and one FineWeb-2 subset per Indic tokenizer. `tools/parity.py
  --language <key> --script-fuzz N` then gives 0 filter and MinHash mismatches for each of the 16 Indic tokenizers
  (2,050 docs each) and for Thai and Vietnamese (5,750 docs each). `tests/test_wordtok.py` repeats a small version.
- Stats steps (WordStats, SentenceStats, DocStats, LineStats, ParagraphStats): `tools/stats_parity.py <parquet>
  [--language KEY]` runs both steps over docs split into 3 tasks (rank = task), then datatrove's `StatsMerger`, and
  compares each task's yielded docs (text, metadata items in order), step stats, every per-task JSON file of every
  group and every merged `metric.json`, byte for byte, or the raised exception. Docs carry synthetic URLs (few hosts,
  multi-label and private suffixes, IPs, IDNs, empty), token counts and preset `fqdn`s. 13 configs vary thresholds
  (with duplicates), groups (subsets, order, duplicates), histogram digits 0-6 and top-k 1/3; one switches to
  datatrove's loop mid-run (token counts past 2**53), one has no URLs (both raise `KeyError`). 0 differences on 10,500
  English docs (6,000 FineWeb, 1,500 perturbed, 2,000 fuzzed, 1,000 case-heavy fuzzed in `tools/stats_ref.py`), and,
  for WordStats/SentenceStats, on 23,125 FineWeb-2 and fuzzed docs in German, French, Russian, Arabic, Turkish,
  Chinese, Japanese, Vietnamese, Thai, Hindi, Bengali, Tamil and Urdu (Urdu SentenceStats raises in both without
  urduhack). `tests/test_stats.py` repeats a small version, plus `str(round(x, d))` on 10,000+ floats and ties.
- `tests/test_warc_reader.py` compares `WarcReader` output (documents, ids, metadata, stats, raised exception)
  on hand-built WARCs: every record type, missing payload types (libmagic), non-UTF-8 payloads (cchardet), chunked
  and gzip HTTP bodies, header continuations, broken framing, corrupt, truncated and padded gzip, `limit`/`skip`.
  With `DATATROVE_RS_WARC=<file.warc.gz> DATATROVE_RS_WARC_LIMIT=1000000` it reads a whole Common Crawl file:
  `CC-MAIN-20240220211055-20240221001055-00000.warc.gz` (1.1 GB) gives identical output.
- `tests/test_cluster.py` compares `MinhashDedupCluster` output files byte for byte and its stats on synthetic
  `.dups` (random graphs, chains, index `SENTINEL` pairs, every flag combination, truncated files). Also identical:
  real stage 1-2 output (25,729 FineWeb docs with injected near-duplicates plus an index, 161,480 pairs) and a
  synthetic 37M-pair / 4.5M-doc set.
- `tests/test_pii.py` runs `PIIFormatter` on adversarial strings, 5,000 fuzzed strings, every IPv4 `is_global`
  interval bound and 20,000 random addresses across five configs, comparing text and the round-robin counters.
- `tests/test_badwords.py` compares `C4BadWordsFilter` verdicts and stats with the real en/de/ru/ja/zh lists on
  words flanked by ASCII, Unicode word chars, combining marks and edges.
- `tests/test_ftfy.py` checks that each translated ftfy regex matches exactly the lines Python's does (6,000
  mojibake/entity/control-char strings) and that `FTFYFormatter` output equals datatrove's in three configs.
- On real text (12,000 FineWeb docs, 12,000 FineWeb-2 docs in 8 languages, each plus 1,000 PII-dense copies),
  `PIIFormatter`, `FTFYFormatter` and `C4BadWordsFilter` give 0 mismatches.
- `tests/test_url_lang.py` compares `URLFilter` (decisions, reasons, stats; the Rust host split against tldextract
  on ~2,800 URLs built from the suffix list, with and without PSL private domains) and `LanguageFilter` (metadata,
  decisions, stats across six configs).

## How exactness is kept, and its limits

- **Unicode tables.** Python's `str.isspace`/`isalpha`/`lower`, `unicodedata` NFD and categories, `re`'s `\d`/`\s`/`\w`,
  and the `regex` module's `\p{Nd}` are exported from Python into `crates/datatrove-core/data/py_unicode.json` by
  `tools/export_pytext.py`. The shipped tables come from Python 3.13 (Unicode 15.1). On import, the package warns if
  the running Python uses a different Unicode version, since results could then differ on rare characters.
- **Language.** The Rust path runs when datatrove would use a plain `spacy.blank(<code>)` tokenizer for one of the 57
  rule-based spaCy languages (`datatrove_rs._core.spacy_languages()`), which covers every language code datatrove
  maps to one of them. Their rules are exported from spaCy into `crates/spacy-tokenizer/data/rules/` by
  `export_rules.py`; case-insensitive patterns are rewritten to the exact code-point sets Python's `re` matches
  (`ignorecase.py`). Chinese, Japanese, Vietnamese, Thai and Indic have their own ports (below); Korean (Kiwi: a C++
  morphological analyzer with a 105 MB language model, not reachable through kiwipiepy's abi3 wheel) and Stanza
  languages run datatrove's Python code unchanged.
- **Vietnamese, Thai, Indic** (`crates/datatrove-core/src/wordtok`). Each port embeds data exported from one library
  version (`tools/export_wordtok.py`, `tools/export_vi.py`) and runs only when that library is absent or installed at
  that version (`python/datatrove_rs/_wordtok.py`: pythainlp 5.4.0, indic-nlp-library 0.92, pyvi 0.1.1).
  - Vietnamese is spaCy's `vi` tokenizer with pyvi: the syllable regex, pyvi's CRF features and Viterbi over the
    weights read from the binary crfsuite model (ties as crfsuite breaks them), its merge rules, then spaCy's gaps
    and sentencizer.
  - Thai is pythainlp's `newmm` in safe mode (TCC regex, dictionary trie, its graph search and chunking),
    `rejoin_formatted_num`, then `strip_strings`. Sentences come from pythainlp's CRF `crfcut`, which isn't ported:
    Rust C4 only uses the fact that each kept line has at least one sentence and asks datatrove when a doc keeps too
    few lines to decide. That needs pythainlp and python-crfsuite installed, and `split_paragraph=True`.
  - Indic is `trivial_tokenize` (including the number-joining quirk that skips a match at offset 0) and
    `sentence_split` with its abbreviation merges and transliterated acronym check. Where datatrove's
    `simple_span_tokenize` raises on a merged sentence, or for Urdu (urduhack), C4 runs datatrove's code for that doc.
    C4 with `split_paragraph=False` runs datatrove's code for Thai and Indic.
- **Chinese.** datatrove's `SpaCyTokenizer("zh")` is spaCy's Chinese tokenizer with `segmenter="jieba"`:
  `jieba.cut(text, HMM=True)` then `util.get_words_and_spaces`. It covers every key datatrove maps to it (`cmn`,
  `zh_Hani`, `yue`, `wuu_Hani`, `hak_Hani`, `nan_Hani`, `zho_Hani`, ...); note that bare `"zh"` maps to spaCy `xx`
  and `"zho"` to the whitespace tokenizer in datatrove's table, and `lzh` is Stanza. jieba-rs is not exact (it keeps
  ASCII runs like `Booking.com` and `e-mail` whole, which jieba's HMM splits; 29-46% of FineWeb-2 docs differ), so
  `crates/cjk-tokenizer/src/jieba` ports jieba 0.42.1's `cut` (DAG, `calc` with Python's tuple tie-breaks, the
  finalseg Viterbi). Its data is jieba's live `dt.FREQ`/`total` and `finalseg` tables, read once per process when
  the first Chinese step builds (`python/datatrove_rs/cjk.py`), so words a program adds to jieba before that are
  honoured and later changes are not. Needs jieba 0.42.1 installed, as datatrove does.
- **Japanese.** datatrove's `SpaCyTokenizer("ja")` splits text with `chunk_text_on_bytes(text, 40000)`, runs
  SudachiPy (`Dictionary().create(mode=A)`: default settings, SudachiDict-core) per chunk, drops empty and repeated
  "空白" tokens and aligns them (`datatrove.utils.japanese_tokenizer`). SudachiPy is a binding of sudachi.rs, so Rust
  links the same release (0.7.0) and loads the same settings, resources and dictionary files; any other SudachiPy
  version, or no SudachiDict-core, runs datatrove's code. Sentence spans keep datatrove's per-chunk offset quirk
  (each chunk's spans start at the previous span's end, which drifts by one when a chunk ends in a token's trailing
  space). Where datatrove's alignment would raise (E194), Rust raises `TokenizerFallback` and that doc runs
  datatrove's code, raising the same error; it never happened on the corpora above.
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
- **MinhashDedupCluster.** Union by size with datatrove's tie rule (the pair's first root wins) and the index
  `SENTINEL` always becoming the root; roots don't depend on path compression, so nodes `file << 32 | doc` sorted
  give the same `.remove`/`.clusters`/`.sizes` bytes and cluster ids. Stats are built by the same sequence of
  `MetricStats.update` float operations, in datatrove's key order. Python still lists, reads and writes the files
  through fsspec. A root reached only by `(x, x)` pairs raises `KeyError` like datatrove.
- **PIIFormatter.** Rust `regex` and Python `re` agree on leftmost-first matches for these ASCII patterns; the
  email pattern's `\b` is checked separately with Python's `\w` table. IPv4 `is_global` comes from the running
  Python's `ipaddress` networks, evaluated once per interval between their bounds, and leading-zero rejection is
  probed. The round-robin counters live on datatrove's replacers, so a doc with lone surrogates (which runs in
  Python) continues the same sequence. Non-default patterns, validators or replacement types run datatrove's code.
- **C4BadWordsFilter.** The word list is read back from datatrove's own compiled regex and accepted only if
  re-escaping it rebuilds the identical pattern. A search succeeds iff some occurrence is flanked by a Python
  non-`\w` char or the text edge (an empty word, from a blank line in the list, is handled too). Lists, sampling
  and stats stay datatrove's.
- **FTFYFormatter.** Never changes text itself: it returns the input when ftfy provably would, and calls ftfy
  otherwise. The proof per fixer is in `python/datatrove_rs/ftfy_check.py`; ftfy's regexes are translated from
  Python's parse tree. Configs with `fix_line_breaks` or `normalization`, or ftfy other than 6.x, always call ftfy.
- **Stats steps.** `extract_stats` runs in Rust on the step's tokenizer (as the filters pick it) with Python's
  `isupper`/`istitle`/`isdigit` tables; SentenceStats uses the sentencizer spans (per 40,000-byte chunk for Japanese)
  or the Indic `sentence_split` port, and Thai and Urdu sentences run datatrove's code. `run()` keeps datatrove's loop
  shape (`python/datatrove_rs/_stats_run.py`) with the counters in Rust: `counters[stat][key] += value` is
  `MetricStats.__add__` with `from_dict(value)` (unit "task"), redone operation for operation with int totals kept
  exact and int/float `min`/`max` kept apart; histogram keys are CPython's `round` (correctly rounded, ties to even)
  and float `repr` (on a tie between two shortest digit strings, the correctly rounded one). At the end the counters
  become real `MetricStats` and datatrove's save loop writes the files (`json.dumps` in one write instead of
  `json.dump`'s streamed chunks: same text). A value, key or token count that isn't a plain int (|v| <= 2**53),
  finite float or str hands the counters to datatrove's loop for the rest of the run. Only datatrove 0.10.1
  (`VERIFIED_DATATROVE`), unmodified `extract_stats`/`get_kv`/`track_time`, int `histogram_round_digits` >= 0 and
  plain int thresholds and str stop words take the Rust path; LineStats docs with `metadata["lines"]` extract in
  Python.
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
