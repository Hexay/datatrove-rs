"""Every embedded non-English tokenizer against datatrove on adversarial docs built from that language's spaCy rules.

The large per-language runs on FineWeb-2 text are in docs/parity.md; this keeps each language honest in CI.
"""

import io
import os
import random
import sys
from functools import partial

import pytest
from datatrove.pipeline import filters as dt
from datatrove.utils.text import split_into_sentences, split_into_words
from datatrove.utils.word_tokenizers import load_tokenizer_assignments

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "tools"))
import datatrove_rs as rs  # noqa: E402
from datatrove_rs import _base, _core  # noqa: E402
from fuzz import FRAGMENTS, SEPARATORS  # noqa: E402
from parity import check_filter, check_minhash  # noqa: E402
from parity_ref import settle_is_alpha_state  # noqa: E402

LANGS = sorted(_base.rust_languages() - {"en"})
PUNCT = list("()[]\"'`’“”«».,;:!?…-–—/%$€#&") + ["...", "--"]


def language_key(code):
    """A datatrove language key datatrove tokenizes with plain `SpaCyTokenizer(code)`."""
    for key, factory in load_tokenizer_assignments().items():
        if isinstance(factory, partial) and factory.args == ("SpaCyTokenizer", code):
            return key
    pytest.skip(f"no datatrove language maps to {code}")


def lang_docs(code, n=120, seed=0):
    import spacy

    keys = list(spacy.blank(code).tokenizer.rules)
    keys += [v for k in keys for v in (k.upper(), k.title())]
    rng = random.Random(seed)
    docs = []
    for _ in range(n):
        parts = []
        for _ in range(rng.randint(1, 80)):
            word = rng.choice(keys if rng.random() < 0.6 else FRAGMENTS)
            pre = "".join(rng.choice(PUNCT) for _ in range(rng.choice([0, 0, 0, 1, 2])))
            suf = "".join(rng.choice(PUNCT) for _ in range(rng.choice([0, 0, 1, 2])))
            parts += [pre + word + suf, rng.choice(SEPARATORS)]
        docs.append("".join(parts))
    return docs


@pytest.mark.parametrize("code", LANGS)
def test_words_and_sentences(code):
    key = language_key(code)
    settle_is_alpha_state(key)
    docs = [d for d in lang_docs(code) if "IS_ALPHA" not in d]
    bad_words = [d for d in docs if split_into_words(d, key) != _core.split_into_words(d, code)]
    bad_sents = [d for d in docs if split_into_sentences(d, key) != _core.split_into_sentences(d, code)]
    assert not bad_words and not bad_sents, (bad_words + bad_sents)[0][:500]


@pytest.mark.parametrize("code", LANGS)
def test_steps(code):
    key = language_key(code)
    settle_is_alpha_state(key)
    docs = lang_docs(code, n=60, seed=1)
    report = io.StringIO()
    assert rs.GopherRepetitionFilter(language=key).rust() is not None
    for name, py_cls, rs_cls in [
        ("GopherRepetition", dt.GopherRepetitionFilter, rs.GopherRepetitionFilter),
        ("C4Quality[sentences]", dt.C4QualityFilter, rs.C4QualityFilter),
    ]:
        kwargs = {"language": key} | ({"split_paragraph": False} if "C4" in name else {})
        assert check_filter(name, py_cls, rs_cls, kwargs, docs, report) == (0, True), report.getvalue()[:3000]
    assert check_minhash("MinHash", {}, docs, report, language=key) == (0, True), report.getvalue()[:3000]
