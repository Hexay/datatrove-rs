"""The ported non-spaCy tokenizers (Indic, Thai, Vietnamese) against datatrove on adversarial docs.

The large runs on FineWeb-2 text (`tools/wordtok_parity.sh`, `tools/parity.py --language`) are in docs/parity.md.
"""

import io
import os
import subprocess
import sys
from functools import partial

import pytest
from datatrove.data import Document
from datatrove.pipeline import filters as dt
from datatrove.pipeline import stats as dts
from datatrove.utils.text import split_into_sentences
from datatrove.utils.word_tokenizers import load_tokenizer_assignments, load_word_tokenizer

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "tools"))
import datatrove_rs as rs  # noqa: E402
from datatrove_rs import _base, _core  # noqa: E402
from datatrove_rs import stats as rs_stats_mod  # noqa: E402
from ngram_parity import check_language  # noqa: E402
from parity import check_filter, check_minhash  # noqa: E402
from wordtok_fuzz import fuzz_docs  # noqa: E402
from wordtok_parity import check_sentences  # noqa: E402

LIBS = {"vi": "pyvi", "thai": "pythainlp"}
NAMES = sorted(_core.word_tokenizers())


def language_key(name):
    if name == "vi":
        return "vie_Latn"
    if name == "thai":
        return "tha_Thai"
    code = name.split(":", 1)[1]
    for key, factory in load_tokenizer_assignments().items():
        if isinstance(factory, partial) and factory.args == ("IndicNLPTokenizer", code) and "_" in key:
            return key
    pytest.skip(f"no datatrove language maps to {name}")


def setup(name, n, seed=0):
    pytest.importorskip(LIBS.get(name, "indicnlp"))
    key = language_key(name)
    assert _base.rust_language(key) == name
    return key, [d for d in fuzz_docs([], n, seed, name) if "IS_ALPHA" not in d]


@pytest.mark.parametrize("name", NAMES)
def test_words_and_sentences(name):
    key, docs = setup(name, 80)
    tok = load_word_tokenizer(key)
    bad = [d for d in docs if tok.word_tokenize(d) != _core.split_into_words(d, name)]
    assert not bad, bad[0][:500]
    if name == "thai":
        docs = docs[:15]  # pythainlp's crfcut is slow
        bad = [d for d in docs if tok.sent_tokenize(d) != _core.sent_tokenize(d, name)]
        assert not bad, bad[0][:500]
    lines = [line.strip() for d in docs for line in d.splitlines()]
    bad = [line for line in lines if not check_sentences(line, key, name)]
    assert not bad, bad[0][:500]
    if name in ("vi", "thai"):
        bad = [d for d in docs if split_into_sentences(d, key) != _core.split_into_sentences(d, name)]
        assert not bad, bad[0][:500]


@pytest.mark.parametrize("name", NAMES)
def test_steps(name, tmp_path):
    key, docs = setup(name, 40, seed=1)
    report = io.StringIO()
    cases = [
        ("GopherRepetition", dt.GopherRepetitionFilter, rs.GopherRepetitionFilter, {}),
        ("GopherQuality", dt.GopherQualityFilter, rs.GopherQualityFilter, {}),
        ("FineWebQuality", dt.FineWebQualityFilter, rs.FineWebQualityFilter, {}),
        ("C4Quality", dt.C4QualityFilter, rs.C4QualityFilter, {"filter_no_terminal_punct": False}),
    ]
    if name in ("vi", "thai"):
        cases.append(("C4Quality[sentences]", dt.C4QualityFilter, rs.C4QualityFilter, {"split_paragraph": False}))
    if name == "thai":
        cases.append(("C4Quality[min 5]", dt.C4QualityFilter, rs.C4QualityFilter, {"min_num_sentences": 5}))
        f = rs.C4QualityFilter(language=key, split_paragraph=False)
        assert f.rust() is not None and f.rust().filter(docs[0]) is not None
        py_stats, rs_stats = (m.SentenceStats(str(tmp_path), language=key) for m in (dts, rs_stats_mod))
        assert rs_stats.rust()(docs[0]) is not None
        for d in docs[:10]:
            doc = Document(text=d, id="0")
            assert py_stats.extract_stats(doc) == rs_stats.extract_stats(doc), d[:300]
    for case, py_cls, rs_cls, kwargs in cases:
        bad = check_filter(case, py_cls, rs_cls, {"language": key} | kwargs, docs, report)
        assert bad == (0, True), report.getvalue()[:3000]
    assert check_minhash("MinHash", {}, docs, report, language=key) == (0, True), report.getvalue()[:3000]
    assert not [r for r in check_language(docs, report, key) if r[1] or not r[2]], report.getvalue()[:3000]


def test_c4_falls_back_where_rust_cannot_decide():
    pytest.importorskip("indicnlp")
    # Urdu sentences come from urduhack: C4 hands any doc with a kept line to datatrove.
    f = rs.C4QualityFilter(language="urd_Arab", min_words_per_line=1, filter_no_terminal_punct=False)
    assert f.rust() is not None and f.rust().filter("ایک دو تین") is None
    # `split_paragraph=False` runs datatrove's code for Indic.
    assert rs.C4QualityFilter(language="hin_Deva", split_paragraph=False).rust() is None


def test_thai_sentences_need_verified_crfsuite():
    """Another python-crfsuite (or pythainlp) leaves Thai sentences to datatrove; words stay in Rust. The model is
    loaded once per process, so this runs in a fresh one."""
    pytest.importorskip("pycrfsuite")
    code = (
        "import importlib.metadata as m, sys\n"
        "real = m.version\n"
        "m.version = lambda d: '0.9.7' if d == 'python-crfsuite' else real(d)\n"
        "from datatrove_rs import C4QualityFilter, _base, _core, _wordtok\n"
        "from datatrove_rs.stats import SentenceStats\n"
        "assert _base.rust_language('tha_Thai') == 'thai' and not _wordtok.load_thai_sentences()\n"
        "assert _core.sent_tokenize('ก ข. ค', 'thai') is None\n"
        "assert SentenceStats('/tmp', language='tha_Thai').rust()('ก ข. ค') is None\n"
        "assert C4QualityFilter(language='tha_Thai', split_paragraph=False).rust() is None\n"
        "assert _core.split_into_words('ก ข', 'thai') == ['ก', 'ข']\n"
    )
    subprocess.run([sys.executable, "-c", code], check=True)
