# FineWeb recipe: where the CPU goes (datatrove 0.10.1 vs datatrove_rs, 2026-11-10)

Scripts and raw results: `/srv/agents/work/fineweb-profile/` (outside the repo; `results/tables.md`).

**Bottom line:** with datatrove_rs, Trafilatura is ~93% of end-to-end CPU. Next: WarcReader 2.2%,
LanguageFilter 2.1%, URLFilter 0.7%, TokensCounter 0.4%; everything else (MinHash stages 2-4 included) < 0.5%.
datatrove_rs makes its own steps 21-28x faster, but the whole recipe only ~1.12x (Amdahl).

## With trafex (byte-exact Rust trafilatura, Hexay/trafex) — same 5000 records

Output digest identical across pure / rs / rs+trafex. `run_trafex.sh` in the profile dir.

| Run | pure | rs | rs + trafex |
|---|---:|---:|---:|
| Executor (tasks=1, sandboxed), wall s | 87.7 | – | **17.3 (5.1x)** |
| Executor CPU s (parent + sandbox child) | 87.6 | – | 17.3 |
| Executor Trafilatura step, s | 71.7 | – | 10.4 |
| Chained in-process, CPU s (2 reps) | 77-100 | 74-82 | **8.4-11.0 (~9x)** |
| Trafilatura share of chained CPU | 86% | 96% | 77% |

The executor result is lower than in-process because of fixed costs: startup, WarcReader (2 s), sandbox IPC.
Trafilatura is still the largest step; after it come WarcReader, URLFilter, and LanguageFilter at ~1.5-2 s each.

## With Rust WarcReader, URLFilter and LanguageFilter (2026-11-11)

`executor_run.py 5000 {pure,rsx,rsy}`: `rsy` = `rsx` plus `datatrove_rs` WarcReader/URLFilter/LanguageFilter.
Output digest identical across pure / rsx / rsy, and every non-time step stat identical between rsx and rsy (pure
differs only in LanguageFilter's input count: 2 docs hit Trafilatura's 1 s wall-clock sandbox timeout under load).
Two runs each, alternated, load avg ~10:

| | rsx | rsy |
|---|---:|---:|
| Executor wall, s | 25.1 / 26.9 | **15.9 / 18.3** |
| Executor parent CPU, s | 8.7 / 9.1 | **3.6 / 4.0** |
| WarcReader time stat, s | 2.13 / 2.26 | 0.45 / 0.52 |
| URLFilter time stat, s | 2.52 / 2.59 | 0.21 / 0.27 |
| LanguageFilter time stat, s | 1.61 / 1.78 | 1.01 / 1.11 |

datatrove's WarcReader time stat covers only `process_record`, not warcio's iteration; the Rust reader's covers its
whole record loop, so the reader's true speedup is the in-process number below. URLFilter's time includes loading the
4.6M-domain blocklist and tldextract's suffix tries on the first document. What LanguageFilter keeps is fastText's own
C++ inference (the thresholded call costs the same as `f.predict(text, k=1)`, 150 us/doc on 2.6k-char docs), so
fastText itself is not ported.

In-process CPU per step, 5,000 records (ranges over alternated runs, identical output and stats):

| Step | datatrove s | datatrove_rs s | Speedup |
|---|---:|---:|---:|
| WarcReader (whole read) | 1.7-2.3 | 0.84-0.87 | ~2.2x |
| URLFilter (setup + run) | 1.35-2.67 | 0.21-0.30 | ~7x |
| LanguageFilter (run) | 0.56-0.61 | 0.44-0.45 | ~1.3x |

## Setup

- WARC: `CC-MAIN-2024-10/.../CC-MAIN-20240220211055-20240221001055-00000.warc.gz`; first 5,000 HTML responses
  (`WarcReader(limit=5000)`, 417 MB decoded HTML).
- Funnel: 5000 → URLFilter 4956 → Trafilatura 4611 → LanguageFilter(en) 1343 → GopherRep 1010 → GopherQual 651
  → C4 584 → FineWebQual 481 → MinHash −11 → 470. Pure and rs outputs identical (same sha256).
- Python 3.13, trafilatura 1.11.0, lxml 6.1.3 / libxml2 2.14.6, spaCy 3.8, fasttext lid.176, single process.
- Measured as chained generators (as the executor runs them): materializing each step over the whole list
  breaks datatrove's `lru_cache(5)` on `split_into_parts` and overstates pure filter cost ~15x for GopherQuality.
- CPU time varies up to ±25% between identical runs (load avg 25-40); per-step shares are stable to ±1-2 points.

## Stage A, 5000 HTML records, chained, CPU s (median of 3)

| Step | in → out | pure s | pure % | rs s | rs % |
|---|---|---:|---:|---:|---:|
| WarcReader | – → 5000 | 1.80 | 2.2% | 1.80 | 2.3% |
| URLFilter | 5000 → 4956 | 0.49 | 0.6% | 0.56 | 0.7% |
| **Trafilatura** | 4956 → 4611 | **69.7** | **84.3%** | **75.3** | **94.2%** |
| LanguageFilter | 4611 → 1343 | 1.64 | 2.0% | 1.72 | 2.2% |
| GopherRepetition | 1343 → 1010 | 4.79 | 5.8% | 0.30 | 0.4% |
| GopherQuality | 1010 → 651 | 0.22 | 0.3% | 0.04 | 0.1% |
| C4Quality | 651 → 584 | 2.52 | 3.0% | 0.07 | 0.1% |
| FineWebQuality | 584 → 481 | 1.42 | 1.7% | 0.02 | 0.0% |
| JsonlWriter (gzip) | 481 | 0.11 | 0.1% | 0.13 | 0.2% |

Trafilatura pure vs rs differ only by noise (same code; range 62-117 s across 6 runs).

## MinHash 1-4 and post-dedup (481 docs, CPU s)

| Step | pure | rs |
|---|---:|---:|
| MinhashDedupSignature | 1.60 | 0.06 (28x) |
| Buckets / Cluster / Filter | ~0.01 | ~0.01 |
| TokensCounter (gpt2, already Rust) | 0.30 | 0.31 |
| PIIFormatter (two `re.sub`) | 0.07 | 0.08 |

Synthetic stage 2 at realistic fan-in (500 sig files): ~33 µs/doc over 14 buckets, < 0.05% of stage A.

## MinHash stage 3 at scale, formatters and C4BadWordsFilter

Stage 3 on synthetic `.dups` (14 buckets, 1.5M chain clusters over 400 files: 37.0M pairs, 4.49M docs), one
process each, CPU time and peak RSS (64 MB of it is the interpreter and imports). Outputs identical.

| MinhashDedupCluster | datatrove | datatrove-rs |
|---|---:|---:|
| default flags | 72.8 s, 829 MB | 5.2 s, 350 MB |
| `save_cluster_id` + `save_cluster_size` | 89.6 s, 1,015 MB | 3.7 s, 350 MB |

That is ~2 µs vs 0.1-0.14 µs per pair and ~180-220 vs ~64 bytes per doc above the baseline (a hash index from
node to slot, parent and size arrays, then a sorted node array once the index is freed). Extrapolated to 10⁹ pairs
over 10⁸ docs: ~35-40 min and ~20 GB in datatrove, ~2-3 min and ~6.5 GB in Rust.

Text steps, CPU µs/doc, one instance per side over all docs (+1,000 copies with injected emails and IPs). FineWeb-2
is 1,500 docs each of de, fr, ru, ar, el, tr, pl and xx (already FTFY/PII-processed upstream, so few changes).

| Step | FineWeb datatrove | rs | FineWeb-2 datatrove | rs |
|---|---:|---:|---:|---:|
| PIIFormatter | 263 | 16.5 (16x) | 173 | 19.1 (9x) |
| FTFYFormatter (datatrove defaults) | 417 | 12.4 (34x) | 577 | 27.5 (21x) |
| C4BadWordsFilter (en list) | 403 | 31.6 (13x) | 349 | 27.6 (13x) |

## Inside Trafilatura (`favor_precision=True`, fallbacks on, datatrove defaults)

| Phase (inclusive) | share |
|---|---:|
| `prune_unwanted_nodes` (libxml2 XPath + node removal) | ~50% |
| └ in `compare_extraction`, re-pruning a deepcopy of the full tree | ~27% |
| readability + jusText fallback (`compare_extraction` total) | ~45% |
| `extract_content` main path | ~33% |
| lxml parsing (29k parses for 4956 docs) | ~5% |

By leaf: libxml2 C 57%, CPython 22%, libc 21%. p50 6.8 ms/doc, p99 67 ms. `no_fallback=True` halves the cost
but changes output.

Exactness hazards for any port: output depends on libxml2 2.14 HTML error recovery (parser can't be swapped),
XPath semantics, readability float scoring order, jusText thresholds; trafilatura's process-global dedup cache
(`LRU_TEST`, 4096) makes output depend on earlier docs; datatrove's 1 s wall-clock sandbox timeout makes the doc
set load-dependent (4610 vs 4611 passed on identical runs).

## Ranked port candidates

1. **Trafilatura (93.5%).** Full port (~3.9k lines trafilatura + 0.9k jusText over libxml2 via FFI, XPath →
   compiled predicates): est. 5-10x on the step, 4-7x end to end; high difficulty and exactness risk. Partial port
   (only the XPath pruning, on lxml's libxml2 nodes): est. 1.6-1.9x on the step; fragile coupling to lxml's
   bundled libxml2.
2. **WarcReader (2.2%).** zlib 40%, warcio header parsing 35%, charset detection 12%. Rust reader ~2-3x; low risk
   if decoding stays in Python.
3. **LanguageFilter (2.1%).** fasttext C++ is 62%; the rest is glue requesting all 176 labels. ≤ 0.8% to gain.
4. **URLFilter (0.7%).** Banned-word generator loop; a set intersection in Python fixes it. Not worth Rust.
5. TokensCounter, writers, PIIFormatter, MinHash 2-4: already native or negligible.

Items 2-4 are done (section "With Rust WarcReader, URLFilter and LanguageFilter"). The executor measurement showed
URLFilter's real cost is the one-time blocklist load (1.5 s) and tldextract trie build (0.3-0.5 s), not the per-doc
loop, which is why it got a Rust loader and host split.
