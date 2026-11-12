"""datatrove_rs FTFYFormatter: the translated ftfy regexes match exactly what Python's do, and the formatter's output
equals datatrove's on mojibake, control characters, entities and plain text."""

import random

import pytest

ftfy = pytest.importorskip("ftfy")

import ftfy.bad_codecs  # noqa: E402,F401 - registers sloppy-windows-*
from datatrove.pipeline.formatters import FTFYFormatter as Py  # noqa: E402
from ftfy import badness, fixes  # noqa: E402

from datatrove_rs import _core  # noqa: E402
from datatrove_rs.formatters import FTFYFormatter  # noqa: E402
from datatrove_rs.ftfy_check import to_rust  # noqa: E402

REGEXES = [badness.BADNESS_RE, fixes.HTML_ENTITY_RE, fixes.C1_CONTROL_RE, fixes.ANSI_RE, fixes.SINGLE_QUOTE_RE]
WORDS = ["café", "naïve", "Größe", "señor", "€5", "Москва", "Ελλάδα", "日本", "“quote”", "it’s", "½", "ﬁne", "Ａ"]
PIECES = list("aZ09 .,;&#<>\t\x1b[m\x85\x9d\xa0") + ["&amp;", "&eacute;", "&#233;", "\x1b[31m", "Ã©", "â€™", "Â "]


def _mojibake(rng, word):
    enc = rng.choice(["latin-1", "sloppy-windows-1252", "sloppy-windows-1251", "macroman", "cp437"])
    return word.encode("utf-8").decode(enc, errors="replace")


def _text(rng):
    parts = []
    for _ in range(rng.randrange(0, 12)):
        r = rng.random()
        w = rng.choice(WORDS)
        parts.append(_mojibake(rng, w) if r < 0.3 else w if r < 0.6 else rng.choice(PIECES))
        parts.append(rng.choice([" ", "", "\n"]))
    return "".join(parts)


@pytest.fixture(scope="module")
def texts():
    rng = random.Random(0)
    return [_text(rng) for _ in range(6000)] + ["", "plain ascii text\n", "Ã ok\nÃ ok", "x\nÂ y"]


@pytest.mark.parametrize("regex", REGEXES, ids=lambda r: r.pattern[:20])
def test_translated_regex_matches_python(regex, texts):
    check = _core.FtfyCheck([], [to_rust(regex)])
    for t in texts:
        for line in t.split("\n"):
            assert (check.first_hit(line) is not None) == (regex.search(line) is not None), repr(line)


@pytest.mark.parametrize(
    "kw",
    [{}, {"uncurl_quotes": True, "fix_latin_ligatures": True, "fix_character_width": True}, {"unescape_html": True}],
)
def test_formatter_matches_datatrove(kw, texts):
    py, rs = Py(**kw), FTFYFormatter(**kw)
    fast = 0
    for t in texts:
        assert rs.format(t) == py.format(t), repr(t)
        fast += rs._rust[0].first_hit(t) is None
    assert fast > len(texts) // 10


def test_configs_without_fast_check():
    for kw in ({"fix_line_breaks": True}, {"normalization": "NFC"}):
        rs = FTFYFormatter(**kw)
        assert rs.format("text") == "text" and rs._rust is False
