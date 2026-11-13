"""Sentence dedup stages 1 and 3 against datatrove: the whole pipeline on fuzzed and sentence-sharing docs (English and
each kind of ported tokenizer), the fallbacks, and numpy overflow of the signature fields.

The large runs (`tools/parity.py`, FineWeb and FineWeb-2) are in docs/parity.md.
"""

import io
import os
import pickle
import sys
import tempfile

import pytest
from datatrove.data import Document
from datatrove.pipeline.dedup import sentence_dedup as dts

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "tools"))
import cjk_fuzz  # noqa: E402
import wordtok_fuzz  # noqa: E402
from datatrove_rs import cjk  # noqa: E402
from datatrove_rs import sentence_dedup as sd  # noqa: E402
from datatrove_rs.sentence_dedup import SentenceDedupFilter, SentenceDedupSignature  # noqa: E402
from fuzz import fuzz_docs  # noqa: E402
from parity import check_sentdedup  # noqa: E402
from parity_cases import SENTDEDUP_CASES  # noqa: E402
from parity_ref import settle_is_alpha_state  # noqa: E402
from sentdedup_fuzz import dup_docs  # noqa: E402

DOCS = fuzz_docs(150, seed=5) + dup_docs(fuzz_docs(120, seed=3), 250, seed=2)


def _check(case_name, case, docs, language="eng"):
    settle_is_alpha_state(language)
    report = io.StringIO()
    bad, differ = check_sentdedup(case_name, case, docs, report, language=language)
    assert (bad, differ) == (0, []), report.getvalue()[:4000]


@pytest.mark.parametrize("name,case", SENTDEDUP_CASES, ids=[c[0] for c in SENTDEDUP_CASES])
def test_pipeline_parity(name, case):
    _check(name, case, DOCS)


@pytest.mark.parametrize("case_i", [0, 1])
def test_lone_surrogates_fall_back(case_i):
    """Text Rust can't take as UTF-8 runs datatrove's code for that doc (which raises where datatrove does)."""
    docs = [d.replace("e", "\ud800", 1) if i % 3 == 0 else d for i, d in enumerate(DOCS[:150])]
    _check(*SENTDEDUP_CASES[case_i], docs)


def _lang_docs(name):
    if name in ("zh", "ja"):
        if not cjk.load(name):
            pytest.skip(f"{name}: tokenizer package not installed or a different version")
        seed = cjk_fuzz.fuzz_docs(name, 80)
    elif name == "de":
        seed = fuzz_docs(80, seed=9)
    else:
        pytest.importorskip({"vi": "pyvi", "thai": "pythainlp"}.get(name, "indicnlp"))
        seed = wordtok_fuzz.fuzz_docs([], 80, 0, name)
    return seed + dup_docs(seed, 120, seed=4)


LANGUAGES = {"de": "deu_Latn", "zh": "cmn_Hani", "ja": "jpn_Jpan", "vi": "vie_Latn", "indic:hi": "hin_Deva"}


@pytest.mark.parametrize("name", list(LANGUAGES) + ["thai"])
@pytest.mark.parametrize("case_i", [0, 1])
def test_languages(name, case_i):
    if name == "thai" and case_i == 0:
        pytest.skip("Thai sentences aren't ported: covered by test_fallbacks")
    case_name, case = SENTDEDUP_CASES[case_i]
    _check(case_name, case, _lang_docs(name), LANGUAGES.get(name, "tha_Thai"))


def test_fallbacks():
    pytest.importorskip("pythainlp")
    pytest.importorskip("indicnlp")
    with tempfile.TemporaryDirectory() as tmp:
        for language in ("tha_Thai", "urd_Arab"):
            lines = dts.SentDedupConfig(split_sentences=False)
            for cls in (SentenceDedupSignature, SentenceDedupFilter):
                assert cls(tmp, config=dts.SentDedupConfig(), language=language).rust() is None
                assert cls(tmp, config=lines, language=language).rust() is not None
        odd = dts.SentDedupConfig(n_sentences=2.0)
        assert SentenceDedupSignature(tmp, config=odd).rust() is None


def test_changed_datatrove_falls_back(monkeypatch):
    assert sd.datatrove_verified()
    monkeypatch.setattr(sd, "VERIFIED_SOURCE", "0" * 32)
    sd.datatrove_verified.cache_clear()
    try:
        with tempfile.TemporaryDirectory() as tmp:
            for cls in (SentenceDedupSignature, SentenceDedupFilter):
                assert cls(tmp, config=dts.SentDedupConfig()).rust() is None
    finally:
        sd.datatrove_verified.cache_clear()


def test_signature_overflow_matches_datatrove():
    """A doc with more than 65,536 n-grams overflows numpy's `<u2` sentence field; datatrove's save raises."""
    doc = "line\n" * 70_000
    config = dts.SentDedupConfig(split_sentences=False, n_sentences=1)
    outcomes = []
    for cls in (dts.SentenceDedupSignature, SentenceDedupSignature):
        with tempfile.TemporaryDirectory() as tmp:
            try:
                cls(tmp, config=config).run([Document(text=doc, id="0")])
                outcomes.append("ok")
            except Exception as e:  # noqa: BLE001
                outcomes.append((type(e).__name__, str(e)))
    assert outcomes[0] == outcomes[1]


def test_get_hashes_and_pickling():
    with tempfile.TemporaryDirectory() as tmp:
        config = dts.SentDedupConfig(n_sentences=2)
        py, rs = dts.SentenceDedupSignature(tmp, config=config), SentenceDedupSignature(tmp, config=config)
        for text in DOCS[:60]:
            doc = Document(text=text, id="0")
            assert py.get_hashes(doc, 7) == rs.get_hashes(doc, 7)
        rs.rust()
        again = pickle.loads(pickle.dumps(rs))
        assert again._rust is None and again.rust() is not None
