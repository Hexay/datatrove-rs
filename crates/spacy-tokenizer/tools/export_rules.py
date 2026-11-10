"""Dump spacy.blank(lang) tokenizer rules + Python's Unicode class semantics to JSON.

Usage: export_rules.py [lang] [out.json]
"""

import json
import re
import sys

import spacy
from spacy.attrs import ORTH
from spacy.util import registry  # noqa: F401  (ensures lang registry import side effects)


def ranges(pred):
    out = []
    lo = None
    for cp in range(0x110000):
        ok = pred(chr(cp))
        if ok and lo is None:
            lo = cp
        elif not ok and lo is not None:
            out.append([lo, cp - 1])
            lo = None
    if lo is not None:
        out.append([lo, 0x10FFFF])
    return out


def pattern(fn):
    return None if fn is None else fn.__self__.pattern


def orth(spec):
    return spec[ORTH] if ORTH in spec else spec["ORTH"]


def main():
    lang = sys.argv[1] if len(sys.argv) > 1 else "en"
    out = sys.argv[2] if len(sys.argv) > 2 else f"data/{lang}_rules.json"
    tok = spacy.blank(lang).tokenizer
    for name in ("prefix_search", "suffix_search", "infix_finditer", "token_match", "url_match"):
        fn = getattr(tok, name)
        if fn is not None and fn.__self__.flags & ~re.UNICODE:
            raise SystemExit(f"{name}: unsupported flags {fn.__self__.flags}")
    w, d, s = (re.compile(p) for p in (r"\w", r"\d", r"\s"))
    data = {
        "spacy_version": spacy.__version__,
        "python_version": sys.version.split()[0],
        "lang": lang,
        "faster_heuristics": tok.faster_heuristics,
        "prefix": pattern(tok.prefix_search),
        "suffix": pattern(tok.suffix_search),
        "infix": pattern(tok.infix_finditer),
        "token_match": pattern(tok.token_match),
        "url_match": pattern(tok.url_match),
        "rules": [[k, [orth(sp) for sp in v]] for k, v in sorted(tok.rules.items())],
        "py_isspace": ranges(str.isspace),
        "py_w": ranges(lambda c: w.match(c) is not None),
        "py_d": ranges(lambda c: d.match(c) is not None),
        "py_s": ranges(lambda c: s.match(c) is not None),
    }
    with open(out, "w", encoding="utf-8") as f:
        json.dump(data, f, ensure_ascii=False)
    print(f"wrote {out}: {len(data['rules'])} rules, spaCy {spacy.__version__}")


if __name__ == "__main__":
    main()
