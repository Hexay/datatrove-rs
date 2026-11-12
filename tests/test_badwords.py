"""datatrove_rs C4BadWordsFilter against datatrove's: per-doc verdicts and stats, with real word lists."""

import copy
import pickle
import random

import pytest
from datatrove.data import Document
from datatrove.pipeline.filters import c4_filters as dt

from datatrove_rs.badwords import C4BadWordsFilter, _RustSearch, _words

FLANKS = ["", " ", "x", "_", "é", "́", "²", "-", "\n", "1", ".", "ß", "İ"]


def _texts(words, rng, n):
    out = ["", " ", "plain text", "Hello World\n"]
    for _ in range(n):
        w = rng.choice(words)
        w = w.upper() if rng.random() < 0.2 else w
        out.append(rng.choice(FLANKS) + w + rng.choice(FLANKS) + (" tail" if rng.random() < 0.5 else ""))
    return out


@pytest.mark.parametrize("lang", ["en", "de", "ja", "zh", "ru"])
def test_matches_datatrove(lang):
    try:
        py = dt.C4BadWordsFilter(default_language=lang)
        regex = py._get_badwords(lang)
    except Exception as e:  # noqa: BLE001 - list download needs network
        pytest.skip(f"badwords list unavailable: {e}")
    parsed = _words(regex)
    assert parsed is not None and parsed[1] == (lang not in ("ja", "th", "zh"))
    rs = C4BadWordsFilter(default_language=lang)
    assert isinstance(rs._get_badwords(lang), _RustSearch)
    docs = [Document(text=t, id=str(i)) for i, t in enumerate(_texts(parsed[0], random.Random(lang), 3000))]
    docs.append(Document(text="lone \ud800 surrogate " + parsed[0][0], id="s"))
    for d in docs:
        assert rs.filter(copy.deepcopy(d)) == py.filter(copy.deepcopy(d)), repr(d.text)
    assert rs.stats.stats.to_dict() == py.stats.stats.to_dict()


def test_keep_fraction_draws_match():
    py = dt.C4BadWordsFilter(keep_fraction=0.5, seed=3)
    rs = C4BadWordsFilter(keep_fraction=0.5, seed=3)
    try:
        words = _words(py._get_badwords("en"))[0]
    except Exception as e:  # noqa: BLE001
        pytest.skip(f"badwords list unavailable: {e}")
    docs = [Document(text=f"a {w} b", id=str(i)) for i, w in enumerate(words[:200])]
    assert [rs.filter(d) for d in docs] == [py.filter(d) for d in docs]


def test_word_parsing_round_trips():
    import re

    for words in (["a|b", "c\\d", "x y", "", "new\nline", "日本"], ["only"]):
        pattern = "|".join(re.escape(w) for w in words)
        assert _words(re.compile(pattern)) == (words, False)
        assert _words(re.compile(r"(?:\W|^)({})(?:\W|$)".format(pattern))) == (words, True)
    assert _words(re.compile("a|b", re.IGNORECASE)) is None


def test_pickles_without_rust():
    rs = C4BadWordsFilter(fail_on_missing_language=False, default_language="xx-unknown")
    rs._rust = {"en": object()}
    assert pickle.loads(pickle.dumps(rs))._rust is None
