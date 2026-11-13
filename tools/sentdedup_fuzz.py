"""Documents that share sentences, for sentence dedup parity: exact copies, runs of other docs' sentences and lines
spliced together, short docs built from a small shared pool (min_num_sentences / min_doc_words edges), and Unicode,
whitespace and line-separator edges."""

import random
import re

from fuzz import FRAGMENTS

_SENT_END = re.compile(r"(?<=[.!?。！？।])\s+")

EDGE_UNITS = [
    "!!! ??? ...",
    "...",
    "   ",
    "",
    "1, 2, 3.",
    "IS_ALPHA is here.",
    "Café naïve résumé.",
    "ΟΔΥΣΣΕΥΣ ΚΑΙ ΠΗΝΕΛΟΠΗ.",
    "line sep para\x85nel.",
    "tab\tand\xa0nbsp　ideographic.",
    "é combining ä.",
    "Monday January 5th, 2024.",
    "🙂 emoji only 🙂.",
    "x" * 80 + ".",
]
_JOINERS = [" ", " ", "\n", "\n", "\n\n", "\r\n", "  ", "\t", ". "]


def _units(doc: str) -> list[str]:
    """Sentence-ish pieces of a doc, or its lines when it has few sentence ends."""
    sents = [s for s in _SENT_END.split(doc) if s.strip()]
    return sents if len(sents) >= 3 else [s for s in doc.split("\n") if s.strip()] or [doc]


def _run(rng, units: list[str]) -> list[str]:
    k = rng.randint(1, 8)
    start = rng.randrange(max(1, len(units) - k + 1))
    return units[start : start + k]


def _join(rng, units: list[str]) -> str:
    joiner = rng.choice(_JOINERS)
    return joiner.join(units) if rng.random() < 0.7 else "".join(u + rng.choice(_JOINERS) for u in units)


def dup_docs(real: list[str], n: int, seed: int = 0) -> list[str]:
    rng = random.Random(seed)
    units = [_units(d) for d in real if d.strip()] or [EDGE_UNITS]
    pool = [rng.choice(rng.choice(units)) for _ in range(30)] + EDGE_UNITS
    docs = []
    while len(docs) < n:
        kind = rng.random()
        if kind < 0.15:
            docs.append(rng.choice(real) if real else rng.choice(EDGE_UNITS))
        elif kind < 0.55:
            parts = [u for _ in range(rng.randint(2, 4)) for u in _run(rng, rng.choice(units))]
            if rng.random() < 0.3:
                parts.insert(rng.randint(0, len(parts)), rng.choice(FRAGMENTS + EDGE_UNITS))
            docs.append(_join(rng, parts))
        elif kind < 0.85:
            docs.append(_join(rng, [rng.choice(pool) for _ in range(rng.randint(1, 6))]))
        else:
            parts = [rng.choice(pool + FRAGMENTS) for _ in range(rng.randint(3, 12))]
            docs.append(rng.choice(["", " ", "\n", "  \n"]) + _join(rng, parts) + rng.choice(["", " ", "\n", "\n\n "]))
    return docs
