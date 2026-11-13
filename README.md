# datatrove-rs

[![PyPI](https://img.shields.io/pypi/v/datatrove-rs.svg)](https://pypi.org/project/datatrove-rs/)
[![Python versions](https://img.shields.io/pypi/pyversions/datatrove-rs.svg)](https://pypi.org/project/datatrove-rs/)
[![CI](https://github.com/Hexay/datatrove-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/Hexay/datatrove-rs/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Rust implementations of the hot steps in [datatrove](https://github.com/huggingface/datatrove) pipelines, with
**byte-identical output**. Turn it on with one environment variable, keep your pipeline code as it is, and the
FineWeb recipe runs about 5x faster.

<p align="center">
  <img src="https://raw.githubusercontent.com/Hexay/datatrove-rs/master/docs/assets/fineweb-speed.svg"
       alt="FineWeb recipe on 5,000 Common Crawl pages: datatrove 87.7 s, datatrove-rs 17.3 s" width="720">
</p>

## Installation

```bash
pip install "datatrove-rs[trafilatura]"
```

The `[trafilatura]` extra adds [trafilatura-rs](https://github.com/Hexay/trafilatura-rs) for HTML extraction, which
is where most of the recipe's time goes. Leave the extra off to accelerate only the filters and MinHash.

Requires Python 3.10+ and datatrove `>=0.10.1,<0.11`. Wheels are built for Linux, macOS and Windows.

## Quickstart

Your existing pipeline, unchanged:

```bash
DATATROVE_RS=1 python my_pipeline.py
```

That's it. Any step datatrove-rs covers runs in Rust; every other step runs datatrove's own code. The switch also
reaches executor worker processes.

Prefer it in code? Either of these works too:

```python
import datatrove_rs
datatrove_rs.patch()  # datatrove's own imports now return the Rust-backed steps

# or import from the mirror: any `datatrove.X` module is available as `datatrove_rs.X`
from datatrove_rs.pipeline.filters import GopherQualityFilter, URLFilter
```

You can also run a script unmodified with `python -m datatrove_rs my_pipeline.py`.

## What's accelerated

| datatrove step | Speedup | Notes |
|---|---:|---|
| `extractors.Trafilatura` | ~7x | via trafilatura-rs, with the `[trafilatura]` extra |
| `filters.GopherRepetitionFilter` | 87x | |
| `filters.GopherQualityFilter` | 79x | |
| `filters.C4QualityFilter` | 49x | |
| `filters.FineWebQualityFilter` | 86x | |
| `dedup.MinhashDedupSignature` | 48x | MinHash stage 1 |
| `dedup.MinhashDedupCluster` | 14-24x | MinHash stage 3; 2.7-3.3x less peak memory |
| `dedup.SingleBloomFilter` | 38x | `step()`; 30x German, 20x Chinese; same saved bloom file |
| `decont.NGramsDecontFilter` | 43x | 35x German, 16x Chinese; `NGramsDecontIndexer.compute_hashes` too |
| `filters.UnigramLogProbFilter` | 75x | 50x German, 27x Chinese |
| `dedup.SentenceDedupSignature` | 65x | sentence dedup stage 1; 52x German, 24x Chinese, 4x Hindi; same signature files |
| `dedup.SentenceDedupFilter` | 119x | stage 3; 83x German, 38x Chinese, 1.8x Hindi; Thai/Urdu sentences stay Python |
| `formatters.PIIFormatter` | 9-16x | |
| `formatters.FTFYFormatter` | 21-34x | skips ftfy where it provably changes nothing; the rest still runs ftfy |
| `filters.C4BadWordsFilter` | 13x | |
| `readers.WarcReader` | ~2.2x | gzip WARCs; charset detection stays in Python |
| `filters.URLFilter` | ~7x | mostly blocklist loading and tldextract setup |
| `filters.LanguageFilter` | ~1.3x | the rest is fastText's own C++ inference |
| `stats.WordStats` | 24-32x | full `run()`, all four groups; 20-31x German/Chinese, 2x Hindi |
| `stats.SentenceStats` | 50x | 54x German, 28x Chinese, 2.5x Hindi; Thai/Urdu sentences stay Python |
| `stats.DocStats` | 8x | |
| `stats.LineStats`, `stats.ParagraphStats` | 3-4x | mostly the per-group bookkeeping |

Filters and MinHash stage 1 run in Rust for the 57 languages spaCy tokenizes with rules (English plus German,
French, Spanish, Russian, Italian, Portuguese, Polish, Dutch, Turkish, Indonesian and 46 more), with 53-88x on the
four-filter chain in the ten largest, and for Chinese (jieba, 33x on the chain, 28x MinHash) and Japanese (SudachiPy,
4.5x: Sudachi's own analysis, already Rust in SudachiPy, dominates both sides). They also run for Vietnamese (pyvi,
31x chain, 40x MinHash), Thai (pythainlp newmm, 9x chain, 5x MinHash) and every language datatrove gives the Indic NLP
tokenizer (16 tokenizers behind 129 language keys, Hindi, Bengali, Tamil, Urdu and more: 14-18x chain, 4-6x MinHash).
Korean (Kiwi, a C++ morphological analyzer) and Stanza languages stay on datatrove's code. Filter and MinHash
speedups are single-thread throughput on 1,000 FineWeb or FineWeb-2 documents. Trafilatura's is inside the
full pipeline; reader, URL and language numbers are CPU time on 5,000 Common Crawl records. MinHash stage 3 is CPU
time on 37M duplicate pairs over 4.5M documents (default flags / with cluster ids and sizes); the formatters and
C4BadWordsFilter on 12,000 FineWeb and 12,000 FineWeb-2 documents. Full numbers and method:
[docs/fineweb-profile.md](docs/fineweb-profile.md).

Each replacement subclasses the datatrove step, takes the same arguments and produces the same results: keep/drop
decisions, drop reasons, rewritten text, stats, MinHash signature and cluster files, and Bloom filter files. Pickled pipelines work on Slurm and
local multiprocessing workers whether or not the worker patched.

## Exactness

Output equality with datatrove is the product, so every change is checked against unmodified datatrove:

- **Filters and MinHash:** 10,500 real, perturbed and fuzzed documents across 26 configurations, 0 mismatches, plus
  81,825 FineWeb-2 and fuzzed documents across the 56 other languages, 33,205 in Chinese and Japanese and 44,300 in
  Vietnamese, Thai and the 16 Indic tokenizers. The tokenizers match datatrove's on 153M tokens in 57 spaCy
  languages, 21.4M Chinese and Japanese tokens and 58.3M Vietnamese, Thai and Indic tokens.
- **Stats steps:** per-task and merged stats files, per-doc metadata and step stats byte-identical on 10,500
  English documents across 13 configurations and 23,125 documents in 13 other languages.
- **Bloom filter, decontamination, unigram log-probs:** decisions, stats, metadata and the saved bloom file
  identical on 4,000 FineWeb documents plus 7,000 perturbed, fuzzed and duplication-heavy ones (with datatrove's own
  unigram table), and 9,000 FineWeb-2 documents plus 13,950 derived ones in 9 languages, across 18 configurations.
- **Sentence dedup:** signature files, stage 2 duplicates, kept and removed docs and stats byte-identical on 22,000
  English documents and 58,500 in 13 other languages, across 6 configurations.
- **Trafilatura:** 10,666 of 10,666 Common Crawl pages identical (see trafilatura-rs).
- **End to end:** the FineWeb recipe gives identical output on 5,000 Common Crawl records.

When the Rust path can't guarantee identical results, the step falls back to datatrove's own code for that case.
This applies to languages without a Rust tokenizer (Korean, Stanza languages), to Chinese without
jieba 0.42.1 and Japanese without SudachiPy 0.7.0 and SudachiDict-core installed, to pythainlp, indic-nlp-library or
pyvi at other versions than the ported ones, to sentence counts the ports don't reproduce, to the stats steps on datatrove releases other than 0.10.1, to sentence dedup when datatrove's reproduced methods
change or a subclass overrides them, and to Trafilatura options
trafilatura-rs doesn't cover. Details and
reproduced quirks are in [docs/parity.md](docs/parity.md).

## Contributing

Development setup, the parity harness, benchmarks and releasing: [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Apache-2.0, like datatrove. datatrove-rs is an independent project and is not affiliated with Hugging Face.
