"""Chinese (jieba) and Japanese (SudachiPy) against datatrove on adversarial docs (`tools/cjk_fuzz.py`).

The large runs on FineWeb-2 text are in docs/parity.md. Skipped without jieba / SudachiPy + SudachiDict-core.
"""

import io
import os
import sys

import pytest
from datatrove.pipeline import filters as dt
from datatrove.utils.text import split_into_sentences, split_into_words
from datatrove.utils.word_tokenizers import SpaCyTokenizer

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "tools"))
import datatrove_rs as rs  # noqa: E402
from cjk_fuzz import fuzz_docs  # noqa: E402
from datatrove_rs import _base, _core, cjk  # noqa: E402
from parity import check_filter, check_minhash  # noqa: E402
from parity_ref import settle_is_alpha_state  # noqa: E402

KEYS = {"zh": "cmn_Hani", "ja": "jpn_Jpan"}
EDGES = [
    "", " ", "  ", "\n", "\r\n", "　", "a", "。", "IS", "a b", " a ", "a  b", "a　b", "。 。", "\r\n\r\n",
    "C++和c#", "3.14%的人", "50%以上", "他说：“你好。”我说：“再见！”", "東京(とうきょう)に行く。", "すご〜い！！",
]


def loaded(code):
    if not cjk.load(code):
        pytest.skip(f"{code}: tokenizer package not installed or a different version")
    return KEYS[code]


@pytest.mark.parametrize("code", ["zh", "ja"])
def test_words_and_sentences(code):
    key = loaded(code)
    settle_is_alpha_state(key)
    docs = [d for d in EDGES + fuzz_docs(code, 150) if "IS_ALPHA" not in d]
    bad_words = [d for d in docs if split_into_words(d, key) != _core.split_into_words(d, code)]
    bad_sents = [d for d in docs if split_into_sentences(d, key) != _core.split_into_sentences(d, code)]
    assert not bad_words and not bad_sents, (bad_words + bad_sents)[0][:500]


@pytest.mark.parametrize("code", ["zh", "ja"])
def test_steps(code):
    key = loaded(code)
    settle_is_alpha_state(key)
    docs = EDGES + fuzz_docs(code, 60, seed=1)
    report = io.StringIO()
    assert rs.GopherRepetitionFilter(language=key).rust() is not None
    for name, py_cls, rs_cls, kwargs in [
        ("GopherRepetition", dt.GopherRepetitionFilter, rs.GopherRepetitionFilter, {}),
        ("GopherQuality", dt.GopherQualityFilter, rs.GopherQualityFilter, {}),
        ("C4Quality", dt.C4QualityFilter, rs.C4QualityFilter, {}),
        ("C4Quality[sentences]", dt.C4QualityFilter, rs.C4QualityFilter, {"split_paragraph": False}),
        ("FineWebQuality", dt.FineWebQualityFilter, rs.FineWebQualityFilter, {}),
    ]:
        kwargs = {"language": key} | kwargs
        assert check_filter(name, py_cls, rs_cls, kwargs, docs, report) == (0, True), report.getvalue()[:3000]
    assert check_minhash("MinHash", {}, docs, report, language=key) == (0, True), report.getvalue()[:3000]


def test_language_variants():
    for key in ("zh_Hani", "cmn", "cmn_Hani", "yue", "yue_Hani", "wuu_Hani", "hak_Hani", "nan_Hani", "zho_Hani"):
        assert _base.rust_language(key) == ("zh" if cjk.load("zh") else None), key
    for key in ("ja", "ja_Jpan", "jpn", "jpn_Jpan"):
        assert _base.rust_language(key) == ("ja" if cjk.load("ja") else None), key
    # datatrove's table maps bare "zh" to spaCy "xx" and "zho" to the whitespace tokenizer; Literary Chinese is Stanza.
    assert _base.rust_language("zh") == "xx"
    assert _base.rust_language("hak_Latn") == "xx"
    assert _base.rust_language("zho") is None
    assert _base.rust_language("zho_Latn") is None
    assert _base.rust_language("lzh_Hani") is None


def test_tokenizer_instances():
    if cjk.load("ja"):
        assert _base.rust_language(SpaCyTokenizer("ja")) == "ja"
    custom = SpaCyTokenizer("ja")
    custom.config = {"nlp": {"tokenizer": {"split_mode": "C"}}}  # __init__ ignores a passed config for ja
    assert _base.rust_language(custom) is None


def test_version_mismatch_falls_back(monkeypatch):
    jieba = pytest.importorskip("jieba")
    monkeypatch.setattr(jieba, "__version__", "0.0")
    assert cjk.load.__wrapped__("zh") is False
