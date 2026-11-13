# datatrove-rs

[![PyPI](https://img.shields.io/pypi/v/datatrove-rs.svg)](https://pypi.org/project/datatrove-rs/)
[![Python versions](https://img.shields.io/pypi/pyversions/datatrove-rs.svg)](https://pypi.org/project/datatrove-rs/)
[![CI](https://github.com/Hexay/datatrove-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/Hexay/datatrove-rs/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](https://github.com/Hexay/datatrove-rs/blob/master/LICENSE)

Rust implementations of the slow steps in [datatrove](https://github.com/huggingface/datatrove) pipelines. Each one
subclasses the datatrove step, takes the same arguments and produces the same output: keep/drop decisions, drop
reasons, rewritten text, stats, and MinHash, sentence-dedup and Bloom filter files. Set `DATATROVE_RS=1` and an
unchanged pipeline uses them.

<p align="center">
  <img src="https://raw.githubusercontent.com/Hexay/datatrove-rs/master/docs/assets/fineweb-speed.svg"
       alt="Wall time, LocalPipelineExecutor with 1 task. FineWeb recipe on 5,000 Common Crawl pages: datatrove 113.1 s, datatrove-rs 13.0 s (8.7x). FineWeb-2 German, quality filters and MinHash signatures on 4,000 documents: datatrove 32.4 s, datatrove-rs 0.97 s (33x). Output identical."
       width="720">
</p>

## Installation

```bash
pip install "datatrove-rs[trafilatura]"
```

The `[trafilatura]` extra adds [trafilatura-rs](https://github.com/Hexay/trafilatura-rs) for the `Trafilatura`
extractor, the most expensive step of the FineWeb recipe. Without it, every other step below is still accelerated.

Requires Python 3.10+ and datatrove `>=0.10.1,<0.11`. Wheels (abi3) are built for Linux x86_64/aarch64, macOS
x86_64/arm64 and Windows x64.

## Quickstart

```bash
DATATROVE_RS=1 python my_pipeline.py
```

Covered steps run in Rust; every other step runs datatrove's code. Executor workers inherit the variable, and
pickled steps refer to the `datatrove_rs` classes, so local and Slurm workers get the same steps either way.

In code:

```python
import datatrove_rs
datatrove_rs.patch()  # datatrove's own imports now return the Rust-backed steps

# or import from the mirror: any `datatrove.X` module is available as `datatrove_rs.X`
from datatrove_rs.pipeline.filters import GopherQualityFilter, URLFilter
```

`python -m datatrove_rs my_pipeline.py` runs a script with the patch applied.

## What's covered

| datatrove step | English | Other languages | Notes |
|---|---:|---|---|
| `extractors.Trafilatura` | 9.8x | | step time inside the FineWeb recipe; needs the `[trafilatura]` extra |
| `readers.WarcReader` | 2.2x [#1] | | gzip WARCs; charset detection stays in Python |
| `filters.URLFilter` | 7x [#1] | | mostly blocklist loading and tldextract setup |
| `filters.LanguageFilter` | 1.3x [#1] | | the rest is fastText's own inference |
| `filters.GopherRepetitionFilter` | 79x | | |
| `filters.GopherQualityFilter` | 65x | | |
| `filters.C4QualityFilter` | 58x | | |
| `filters.FineWebQualityFilter` | 77x | | |
| the four filters, chained | 68x | de 53x [#3], zh 33x, ja 4.5x [#5], vi 31x, th 8.9x, hi 16x [#6] | |
| `dedup.MinhashDedupSignature` | 47x | de 45x [#3], zh 28x, ja 4.2x [#5], vi 40x, th 5.3x, hi 5.7x [#6] | MinHash stage 1 |
| `dedup.MinhashDedupCluster` | 14-24x [#2] | | MinHash stage 3, CPU time on 37M pairs; peak RSS 350 MB vs 829-1,015 MB |
| `dedup.SentenceDedupSignature` | 69x | de 52x, zh 24x, hi 4.1x [#9] | sentence dedup stage 1 |
| `dedup.SentenceDedupFilter` | 105x | de 83x, zh 38x, hi 1.8x [#9] | sentence dedup stage 3 |
| `dedup.SingleBloomFilter` | 37x | de 30x, zh 20x [#8] | `step()` |
| `decont.NGramsDecontFilter` | 53x | de 35x, zh 16x [#8] | `NGramsDecontIndexer.compute_hashes` too |
| `filters.UnigramLogProbFilter` | 60x | de 50x, zh 27x [#8] | |
| `formatters.PIIFormatter` | 16x [#2] | FineWeb-2 9x [#2] | |
| `formatters.FTFYFormatter` | 34x [#2] | FineWeb-2 21x [#2] | skips ftfy where it provably changes nothing |
| `filters.C4BadWordsFilter` | 13x [#2] | FineWeb-2 13x [#2] | |
| `stats.WordStats` | 27x | de 31x, zh 20x, hi 2.0x [#7] | full `run()`; datatrove 0.10.1 only |
| `stats.SentenceStats` | 43x | de 54x, zh 28x, hi 2.5x [#7] | datatrove 0.10.1 only |
| `stats.DocStats` | 6x | de 8x, zh 3.6x, hi 3.6x [#7] | datatrove 0.10.1 only |
| `stats.LineStats`, `stats.ParagraphStats` | 2-3x | 1.5-3.9x [#7] | mostly per-group bookkeeping; datatrove 0.10.1 only |

Unmarked numbers were measured on 2026-11-13 with 0.1.5 (see Benchmarks below). Numbers marked with a
pull request come from the results recorded in it: single-thread throughput on 500-1,000 FineWeb or FineWeb-2
documents, except `WarcReader`, `URLFilter` and `LanguageFilter` (CPU time on 5,000 Common Crawl records) and
`MinhashDedupCluster`.

[#1]: https://github.com/Hexay/datatrove-rs/pull/1
[#2]: https://github.com/Hexay/datatrove-rs/pull/2
[#3]: https://github.com/Hexay/datatrove-rs/pull/3
[#5]: https://github.com/Hexay/datatrove-rs/pull/5
[#6]: https://github.com/Hexay/datatrove-rs/pull/6
[#7]: https://github.com/Hexay/datatrove-rs/pull/7
[#8]: https://github.com/Hexay/datatrove-rs/pull/8
[#9]: https://github.com/Hexay/datatrove-rs/pull/9

The steps that tokenize (filters, MinHash stage 1, stats, sentence dedup, Bloom, decontamination, unigram) run in
Rust for:

- the 57 languages spaCy tokenizes with rules (English, German, French, Spanish, Russian, Italian, Portuguese,
  Polish, Dutch, Turkish, Indonesian, 45 more, and the multi-language `xx`);
- Chinese (jieba) and Japanese (SudachiPy);
- Vietnamese (pyvi), Thai (pythainlp `newmm`), and the 16 Indic NLP tokenizers datatrove uses for 129 language
  keys (Hindi, Bengali, Tamil, Urdu and more).

## What stays on datatrove's code

- Korean (Kiwi), Stanza languages, and datatrove's whitespace, Burmese, Tibetan, Lao and Khmer tokenizers.
- Thai and Urdu sentence splitting: `SentenceStats` and sentence dedup run datatrove's code for them (Thai sentence
  dedup in line mode runs in Rust).
- `FTFYFormatter`'s fixes. Rust only proves that ftfy would leave a document unchanged and then skips ftfy; every
  other document goes through ftfy.
- Any step whose dependency is at an unverified version (see below), a subclass that overrides a reproduced
  method, a document with lone surrogates, and options outside the reproduced set (Trafilatura options
  trafilatura-rs doesn't support, non-default PII patterns, `URLFilter` without the integrated lists, and so on).

The fallback is per language, per document or per step, decided at run time. The full list, per step, is in
[docs/parity.md](https://github.com/Hexay/datatrove-rs/blob/master/docs/parity.md).

## Exactness and version guards

Each step is checked against unmodified datatrove in the same environment: per-document results, rewritten text,
`run()` stats, exclusion-writer documents and output files, compared byte for byte on real FineWeb and FineWeb-2
documents plus perturbed and fuzzed ones in every covered language. Both pipelines in the chart give identical
output. Corpora, configurations and results: [docs/parity.md](https://github.com/Hexay/datatrove-rs/blob/master/docs/parity.md).

Where output depends on another library's version, the installed version is checked at run time, and on a
mismatch the affected step or language runs datatrove's code:

| Dependency | Rust path only when |
|---|---|
| datatrove | 0.10.1 for the stats steps; sentence dedup checks a fingerprint of the datatrove methods it reproduces |
| jieba | 0.42.1 |
| SudachiPy | 0.7.0 with SudachiDict-core installed |
| pyvi, pythainlp, indic-nlp-library | 0.1.1, 5.4.0, 0.92, or not installed |
| trafilatura, lxml | a pair trafilatura-rs reproduces |
| tldextract | 5.4.x (otherwise only the host split goes through tldextract) |
| ftfy | 6.x (otherwise ftfy always runs) |

Two things are not checked. The spaCy rules were exported from spaCy 3.8.18 and are tested on spaCy 3.8; the
installed spaCy version is not read. The Unicode tables come from Python 3.13 (Unicode 15.1); on another Unicode
version the package warns on import, since results could differ on rare characters.

## Benchmarks

The chart comes from `tools/recipe_bench.py`, which runs each pipeline through `LocalPipelineExecutor` (1 task),
alternating datatrove and datatrove-rs runs and comparing output digests. `tools/make_chart.py` draws it from
the raw runs in [docs/assets/fineweb-speed.json](https://github.com/Hexay/datatrove-rs/blob/master/docs/assets/fineweb-speed.json).
The 2026-11-13 runs were on a shared 16-CPU machine at load average 8-36. Each run is single-threaded, its CPU
time is within 4% of its wall time, and no run was memory-throttled.

The per-step table comes from `tools/bench.py` (single thread, the step alone on the same documents, median of
3). Per-language tables and older profiling are in
[docs/fineweb-profile.md](https://github.com/Hexay/datatrove-rs/blob/master/docs/fineweb-profile.md) and the
linked pull requests.

## Contributing

Development setup, the parity harness, benchmarks and releasing: [CONTRIBUTING.md](https://github.com/Hexay/datatrove-rs/blob/master/CONTRIBUTING.md).

## License

Apache-2.0, like datatrove. datatrove-rs is an independent project and is not affiliated with Hugging Face.
