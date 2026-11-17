# Upstream issues found along the way

datatrove 0.10.x unless noted.

- `utils/text.py` imports `regex` without declaring it (this package depends on `regex` for that reason).
- The Python MinHash passes `str` to `xxhash`, which xxhash 4 rejects. The reference side needs `xxhash<4`; this
  package's MinHash is unaffected.
- `SentenceDedupSignature(config=None)` raises `AttributeError` in 0.10.1 (`config.hash_config`); pass a
  `SentDedupConfig`. Fixed on main.
- `SentenceDedupFilter` hands the exclusion writer a doc with `text=None` when the doc is empty and has no
  duplicates.
- `importlib.metadata` is used without being imported, and `pipeline.dedup` requires `tokenizers`. For the latter,
  `datatrove_rs.MinhashDedupSignature` is imported lazily, so filter-only installs don't need `tokenizers`.
- The Trafilatura step's 1 s wall-clock sandbox timeout makes the set of extracted docs load-dependent in datatrove
  itself (4,610 vs 4,611 of 4,956 on identical runs); compare extractor output in-process, not through the sandbox.
- spaCy/preshed: a doc's output for the token `IS_ALPHA` depends on process history; see [is-alpha.md](is-alpha.md).
