"""datatrove_rs FTFYFormatter: the translated ftfy regexes match exactly what Python's do, and the formatter's output
equals datatrove's on mojibake, control characters, entities and plain text, through the Rust `fix_text` port
(`tools/ftfy_parity.py` runs the large version) and the unchanged-text check for configs the port doesn't cover."""

import os
import random
import sys

import pytest

ftfy = pytest.importorskip("ftfy")

import ftfy.bad_codecs  # noqa: E402,F401 - registers sloppy-windows-*
from datatrove.pipeline.formatters import FTFYFormatter as Py  # noqa: E402
from ftfy import badness, fixes  # noqa: E402

from datatrove_rs import _core  # noqa: E402
from datatrove_rs.formatters import FTFYFormatter  # noqa: E402
from datatrove_rs.ftfy_check import VERIFIED_FTFY, to_rust  # noqa: E402

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "tools"))
from ftfy_fuzz import fuzz_docs, mojibake_docs  # noqa: E402

REGEXES = [badness.BADNESS_RE, fixes.HTML_ENTITY_RE, fixes.C1_CONTROL_RE, fixes.ANSI_RE, fixes.SINGLE_QUOTE_RE]
WORDS = ["café", "naïve", "Größe", "señor", "€5", "Москва", "Ελλάδα", "日本", "“quote”", "it’s", "½", "ﬁne", "Ａ"]
PIECES = list("aZ09 .,;&#<>\t\x1b[m\x85\x9d\xa0") + ["&amp;", "&eacute;", "&#233;", "\x1b[31m", "Ã©", "â€™", "Â "]
PORTED = ftfy.__version__ == VERIFIED_FTFY


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


@pytest.fixture(scope="module")
def fuzzed(texts):
    rng = random.Random(1)
    return fuzz_docs(1500, rng) + mojibake_docs(texts[:1500], rng)


@pytest.mark.parametrize("regex", REGEXES, ids=lambda r: r.pattern[:20])
def test_translated_regex_matches_python(regex, texts):
    check = _core.FtfyCheck([], [to_rust(regex)])
    for t in texts:
        for line in t.split("\n"):
            assert (check.first_hit(line) is not None) == (regex.search(line) is not None), repr(line)


def _formatters(kw, over=None):
    py, rs = Py(**kw), FTFYFormatter(**kw)
    if over:
        py.config = py.config._replace(**over)
        rs.config = rs.config._replace(**over)
    return py, rs


@pytest.mark.parametrize(
    "kw",
    [{}, {"uncurl_quotes": True, "fix_latin_ligatures": True, "fix_character_width": True}, {"unescape_html": True}],
)
def test_formatter_matches_datatrove(kw, texts):
    py, rs = _formatters(kw)
    for t in texts:
        assert rs.format(t) == py.format(t), repr(t)
    assert sum(rs._rust(t) is not None for t in texts) > len(texts) // 10


@pytest.mark.skipif(not PORTED, reason=f"the fix_text port is verified on ftfy {VERIFIED_FTFY}")
@pytest.mark.parametrize(
    "kw,over",
    [
        ({}, None),
        ({"unescape_html": True, "restore_byte_a0": False}, None),
        ({"replace_lossy_sequences": False, "decode_inconsistent_utf8": False, "fix_c1_controls": False}, None),
        ({"fix_encoding": False, "unescape_html": False}, None),
        ({"restore_byte_a0": False}, {"explain": False}),
        ({}, {"max_decode_length": 23}),
    ],
)
def test_fix_text_port_matches_ftfy(kw, over, fuzzed):
    py, rs = _formatters(kw, over)
    rs.format("")
    assert rs._rust.__name__ == "fix"
    changed = 0
    for t in fuzzed:
        want = py.format(t)
        assert rs._rust(t) == want, repr(t)
        changed += want != t
    assert changed > len(fuzzed) // 4


def test_lone_surrogates_go_to_ftfy():
    py, rs = Py(), FTFYFormatter()
    for t in ["a\ud83d", "\udca9\ud83d b Ã©"]:
        assert rs.format(t) == py.format(t)
        assert rs._rust(t) is None


def test_configs_without_fast_check():
    for kw in ({"fix_line_breaks": True}, {"normalization": "NFC"}):
        rs = FTFYFormatter(**kw)
        assert rs.format("text") == "text" and rs._rust is False
