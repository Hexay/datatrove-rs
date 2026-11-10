"""Random-Unicode parity of the text primitives every step builds on, against datatrove's own functions."""

import itertools
import random

import pytest
from datatrove.utils.text import TextNormConfig, simplify_text, split_into_sentences, split_into_words

from datatrove_rs import _core

POOLS = [
    "abcxyzABCXYZ0123456789 .,;:!?'\"()[]{}-_/\\#…",
    " \t\n\r\x0b\x0c\x1c\x1d\x1e\x85\xa0  　​‌‍\xad",
    "ÀÉÎÕÜßÿĀİıŁœŉǅǈǋǲİKÅΩﬁﬀﬃ",
    "़्ְุ̧̨̀́̈ؐ᷀⃗︠ͅ",
    "ΣσςΑΒΓΔΟΔΥΣΣΕΥΣάέήΐΰ",
    "АБВЖЯабвжяЁё",
    "٠١٢٣٤٥٦٧٨٩٫٬،۰۱۲",
    "०१२३४५६७८९क्षत्रज्ञ",
    "한국어텍스트가각",
    "日本語のテキスト中文",
    "ＡＢＣａｂｃ０１２！？",
    "𝟘𝟙𝟚𝟛𝟜𝟝",
    "\U00010d40\U00010d41\U00010d49\U00016d70\U00016d79\U0001ccf0",
    "🙂👍🏽👨‍👩‍👧🇫🇷",
    "«»“”„‘’‹›¿¡",
]
SEPS = [" ", "  ", "\n", "\n\n", ". ", "! ", "? ", "\t", " ", "\r\n", "\xa0"]


def random_text(rng: random.Random) -> str:
    parts = []
    for _ in range(rng.randint(0, 40)):
        if rng.random() < 0.15:
            cp = rng.randint(0x20, 0x10FFFF)
            parts.append(chr(cp) if not 0xD800 <= cp <= 0xDFFF else "x")
        else:
            pool = rng.choice(POOLS)
            parts.append("".join(rng.choice(pool) for _ in range(rng.randint(1, 8))))
        parts.append(rng.choice(SEPS))
    return "".join(parts)


TEXTS = [random_text(random.Random(i)) for i in range(2500)]
FIELDS = ["lowercase", "norm_whitespace", "remove_punctuation", "norm_unicode_diacritics", "norm_numbers",
          "norm_weekdays", "norm_monthnames"]
NORMS = [c for i, c in enumerate(itertools.product([True, False], repeat=7)) if i % 8 == 0] + [
    tuple(getattr(TextNormConfig(), f) for f in FIELDS)
]


@pytest.mark.parametrize("norm", NORMS, ids=lambda n: "".join("1" if x else "0" for x in n))
def test_simplify_text(norm):
    cfg = TextNormConfig(**dict(zip(FIELDS, norm)))
    bad = [t for t in TEXTS if simplify_text(t, cfg) != _core.simplify_text(t, norm)]
    assert not bad, f"{len(bad)} differ, first: {bad[0]!r}"


def test_split_into_words():
    bad = [t for t in TEXTS if split_into_words(t, "eng") != _core.split_into_words(t)]
    assert not bad, f"{len(bad)} differ, first: {bad[0]!r}"


def test_split_into_sentences():
    bad = [t for t in TEXTS if split_into_sentences(t, "eng") != _core.split_into_sentences(t)]
    assert not bad, f"{len(bad)} differ, first: {bad[0]!r}"
