"""Adversarial documents for parity checks: the Unicode, whitespace and threshold edges the ports must match."""

import random

EDGE_DOCS = [
    "",
    " ",
    "\n",
    "\n\n\n",
    "word",
    "IS_ALPHA is a spaCy attribute name. IS_ALPHA again.",
    "ΟΔΥΣΣΕΥΣ ΚΑΙ ΠΗΝΕΛΟΠΗ. ΑΣ ΠΑΜΕ. Σ. ΑΣ' ΑΣ.Σ ΑΣ-Β",
    "The Kelvin cookie policy. Uses cooKies! İstanbul İ̇. Straße ﬁne.",
    "Café naïve résumé coöperate á̧ ȩ́ 한국어 텍스트.",
    "Prices: 1,000.50 and ١٢٣٫٤٥ and 3.14.15 and ½ ² ௧௨ 12:30 1.a 7,",
    "See [1] and [23][edit] and [citation needed] and [١٢] and [a] and [] end.",
    "Lorem ipsum dolor sit amet, consectetur adipiscing elit.\nSecond line here now.",
    "Enable JavaScript to view this page.\nThis line is fine and long enough.",
    "function() { return 1; } is code.\nAnother normal line with words.",
    "• item one\n• item two\n- item three\n• item four",
    "Ends with ellipsis...\nAnd another…\nAnd one more...\nNormal line.",
    "line sep\x1cfs\x1dgs\x1ers\r\ncrlf\rcr\x0bvt\x0cff\x85nel end.",
    "x" * 1500 + " long word line.\nShort ok line here.",
    "# # # hash heavy # # # text with words and more words here # #",
    "Mr. Smith went to Washington D.C. at 5 p.m. He said: \"Hi!\" Then left... OK?! Yes.",
    "http://example.com/a?b=c www.test.org user@mail.com 192.168.0.1 :) :-( <3",
]

FRAGMENTS = [
    "the",
    "and",
    "of",
    "The quick brown fox jumps over the lazy dog.",
    "Terms of use",
    "privacy policy",
    "Click here!",
    "U.S.",
    "e-mail",
    "don't",
    "can't",
    "O'Neill's",
    "¿Qué?",
    "«quoted»",
    "“smart quotes”",
    "—",
    "…",
    "...",
    "#",
    "•",
    "-",
    "{",
    "}",
    "[1]",
    "[edit]",
    "javascript",
    "lorem ipsum",
    "ΣΟΦΟΣ",
    "Σ",
    "ΑΣ",
    "K",
    "İ",
    "ß",
    "á",
    "١٢٣",
    "1,000",
    "3.5",
    "10km",
    "5pm",
    "IS_ALPHA",
    "日本語のテキスト",
    "中文",
    "🙂",
    "x" * 60,
]

SEPARATORS = [" ", " ", " ", "  ", "\n", "\n", "\n\n", "\t", "\xa0", "　", "\r\n", " ", ". ", "! ", "? ", "\n- ", "\n• "]


def fuzz_docs(n: int, seed: int = 0) -> list[str]:
    rng = random.Random(seed)
    docs = list(EDGE_DOCS)
    while len(docs) < n:
        parts = []
        for _ in range(rng.randint(1, 120)):
            parts.append(rng.choice(FRAGMENTS))
            parts.append(rng.choice(SEPARATORS))
        doc = "".join(parts)
        if rng.random() < 0.3:
            lines = doc.split("\n")
            doc = "\n".join(lines + rng.sample(lines, min(len(lines), rng.randint(1, 5))))
        docs.append(doc)
    return docs


def perturb(docs: list[str], seed: int = 1) -> list[str]:
    """Real docs with duplicated lines/paragraphs and spliced fragments, to hit repetition thresholds."""
    rng = random.Random(seed)
    out = []
    for d in docs:
        lines = d.split("\n")
        k = rng.randint(0, max(1, len(lines)))
        lines += [rng.choice(lines) for _ in range(k)]
        rng.shuffle(lines)
        if rng.random() < 0.5:
            lines.insert(rng.randint(0, len(lines)), rng.choice(FRAGMENTS))
        out.append(("\n\n" if rng.random() < 0.3 else "\n").join(lines))
    return out
