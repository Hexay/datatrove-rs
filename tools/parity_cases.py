"""Step configurations parity is checked on: datatrove defaults, targeted variants, and random thresholds."""

import random

from datatrove.pipeline import filters as dt
from datatrove.pipeline.dedup.sentence_dedup import SentDedupConfig
from datatrove.utils.hashing import HashConfig
from datatrove.utils.text import TextNormConfig

import datatrove_rs as rs

FILTER_CASES = [
    ("GopherQuality", dt.GopherQualityFilter, rs.GopherQualityFilter, {}),
    ("GopherQuality[no min words]", dt.GopherQualityFilter, rs.GopherQualityFilter, {"min_doc_words": None}),
    ("GopherRepetition", dt.GopherRepetitionFilter, rs.GopherRepetitionFilter, {}),
    ("FineWebQuality", dt.FineWebQualityFilter, rs.FineWebQualityFilter, {}),
    ("FineWebQuality[exclude zero]", dt.FineWebQualityFilter, rs.FineWebQualityFilter, {"line_punct_exclude_zero": True}),
    ("C4Quality", dt.C4QualityFilter, rs.C4QualityFilter, {}),
    ("C4Quality[sentences]", dt.C4QualityFilter, rs.C4QualityFilter, {"split_paragraph": False}),
    ("C4Quality[batch 4]", dt.C4QualityFilter, rs.C4QualityFilter, {"batch_size": 4}),
    ("GopherRepetition[batch 3]", dt.GopherRepetitionFilter, rs.GopherRepetitionFilter, {"batch_size": 3}),
]

MINHASH_CASES = [
    ("MinHash[xxhash64]", {}),
    ("MinHash[sha1-32]", {"hash_config": HashConfig(precision=32, hash_fc="sha1")}),
    ("MinHash[xxhash32,dates]", {
        "hash_config": HashConfig(precision=32, hash_fc="xxhash"),
        "norm_config": TextNormConfig(norm_weekdays=True, norm_monthnames=True),
    }),
    ("MinHash[sha1-64,raw]", {
        "hash_config": HashConfig(precision=64, hash_fc="sha1"),
        "norm_config": TextNormConfig(lowercase=False, remove_punctuation=False, norm_numbers=False),
    }),
    ("MinHash[3-gram,4x6]", {"n_grams": 3, "num_buckets": 4, "hashes_per_bucket": 6, "seed": 7}),
]


def _sd(finder_workers=1, **cfg):
    return {"config": SentDedupConfig(**cfg), "finder_workers": finder_workers}


SENTDEDUP_CASES = [
    ("SentDedup", _sd()),
    ("SentDedup[lines,n2,span5]", _sd(split_sentences=False, n_sentences=2, min_words_to_remove_span=5)),
    ("SentDedup[n1,no mins,sha1-32,3 finders]", _sd(
        3, n_sentences=1, min_doc_words=0, min_num_sentences=0, hash_config=HashConfig(precision=32, hash_fc="sha1"),
    )),
    ("SentDedup[n5,span20,xxhash32,raw]", _sd(
        n_sentences=5, min_words_to_remove_span=20, hash_config=HashConfig(precision=32, hash_fc="xxhash"),
        norm_config=TextNormConfig(lowercase=False, remove_punctuation=False, norm_numbers=False),
    )),
    ("SentDedup[n2,10 words,1 sent,sha1-64,dates]", _sd(
        n_sentences=2, min_doc_words=10, min_num_sentences=1, hash_config=HashConfig(precision=64, hash_fc="sha1"),
        norm_config=TextNormConfig(norm_weekdays=True, norm_monthnames=True),
    )),
    ("SentDedup[lines,n3,2 finders]", _sd(2, split_sentences=False)),
]


def _maybe(rng, value, p_none=0.2, zero=False):
    if rng.random() < p_none:
        return 0 if zero and rng.random() < 0.5 else None
    return value


def _gopher_quality(rng):
    words = ["the", "be", "to", "of", "and", "that", "have", "with", "a", "is"]
    return {
        "min_doc_words": _maybe(rng, rng.randint(1, 200), zero=True),
        "max_doc_words": _maybe(rng, rng.randint(50, 3000)),
        "min_avg_word_length": _maybe(rng, rng.uniform(1, 5)),
        "max_avg_word_length": _maybe(rng, rng.uniform(4, 12)),
        "max_symbol_word_ratio": _maybe(rng, rng.uniform(0, 0.3)),
        "max_bullet_lines_ratio": _maybe(rng, rng.uniform(0, 1)),
        "max_ellipsis_lines_ratio": _maybe(rng, rng.uniform(0, 1)),
        "max_non_alpha_words_ratio": _maybe(rng, rng.uniform(0, 1)),
        "min_stop_words": _maybe(rng, rng.randint(1, 5), zero=True),
        "stop_words": rng.sample(words, rng.randint(0, len(words))),
    }


def _gopher_repetition(rng):
    return {
        "dup_line_frac": _maybe(rng, rng.uniform(0, 0.6)),
        "dup_para_frac": _maybe(rng, rng.uniform(0, 0.6)),
        "dup_line_char_frac": _maybe(rng, rng.uniform(0, 0.4)),
        "dup_para_char_frac": _maybe(rng, rng.uniform(0, 0.4)),
        # n = 0 is valid for top n-grams; for dup n-grams datatrove loops forever on it, so it starts at 1.
        "top_n_grams": tuple((rng.randint(0, 6), rng.uniform(0, 0.3)) for _ in range(rng.randint(0, 4))),
        "dup_n_grams": tuple((rng.randint(1, 12), rng.uniform(0, 0.3)) for _ in range(rng.randint(0, 6))),
    }


def _fineweb(rng):
    return {
        "line_punct_thr": rng.uniform(0, 0.5),
        "line_punct_exclude_zero": rng.random() < 0.5,
        "stop_chars": tuple(rng.sample([".", "!", "?", "...", '"', "…", ""], rng.randint(0, 4))),
        "short_line_thr": rng.uniform(0, 1),
        "short_line_length": rng.randint(0, 80),
        "char_duplicates_ratio": rng.uniform(0, 0.2),
        "new_line_ratio": rng.uniform(0, 1),
    }


def _c4(rng):
    flags = ["split_paragraph", "remove_citations", "filter_no_terminal_punct", "filter_lorem_ipsum",
             "filter_javascript", "filter_curly_bracket", "filter_policy"]
    return {f: rng.random() < 0.7 for f in flags} | {
        "min_num_sentences": rng.choice([-1, 0, 1, 3, 5, 8]),
        "min_words_per_line": rng.choice([-1, 0, 1, 3, 6]),
        "max_word_length": rng.choice([-1, 5, 20, 1000]),
    }


def random_filter_cases(k: int, seed: int = 0):
    rng = random.Random(seed)
    kinds = [
        ("GopherQuality", dt.GopherQualityFilter, rs.GopherQualityFilter, _gopher_quality),
        ("GopherRepetition", dt.GopherRepetitionFilter, rs.GopherRepetitionFilter, _gopher_repetition),
        ("FineWebQuality", dt.FineWebQualityFilter, rs.FineWebQualityFilter, _fineweb),
        ("C4Quality", dt.C4QualityFilter, rs.C4QualityFilter, _c4),
    ]
    return [
        (f"{name}[rand{i}]", py, rsc, make(rng) | {"batch_size": rng.choice([1, 1, 2, 5])})
        for i in range(k)
        for name, py, rsc, make in kinds
    ]
