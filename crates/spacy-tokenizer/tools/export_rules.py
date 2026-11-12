"""Dump spacy.blank(lang) tokenizer rules, and Python's Unicode class semantics, to JSON.

Usage: export_rules.py tables [out.json]          -> data/py_tables.json
       export_rules.py <lang>... [--out-dir DIR]  -> DIR/<lang>.json (default data/rules)
"""

import json
import os
import re
import sys

import spacy
from spacy.attrs import ORTH
from spacy.tokenizer import Tokenizer

from ignorecase import check, expand

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CALLABLES = ("prefix_search", "suffix_search", "infix_finditer", "token_match", "url_match")


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


def orth(spec):
    return spec[ORTH] if ORTH in spec else spec["ORTH"]


def pattern(tok, name):
    fn = getattr(tok, name)
    if fn is None:
        return None
    rx = fn.__self__
    if rx.flags & ~(re.UNICODE | re.IGNORECASE):
        raise SystemExit(f"{name}: unsupported flags {rx.flags}")
    if not rx.flags & re.IGNORECASE:
        return rx.pattern
    expanded = expand(rx.pattern, rx.flags)
    check(rx, expanded, list(tok.rules) + ["l'homme", "New-York", "aujourd'hui", "KM", "ſ", "ı", "İ", "K"])
    return expanded


def export_lang(lang, out):
    tok = spacy.blank(lang).tokenizer
    if type(tok) is not Tokenizer:
        raise SystemExit(f"{lang}: {type(tok).__name__} is not spaCy's rule-based Tokenizer")
    data = {
        "spacy_version": spacy.__version__,
        "python_version": sys.version.split()[0],
        "lang": lang,
        "faster_heuristics": tok.faster_heuristics,
        "prefix": pattern(tok, "prefix_search"),
        "suffix": pattern(tok, "suffix_search"),
        "infix": pattern(tok, "infix_finditer"),
        "token_match": pattern(tok, "token_match"),
        "url_match": pattern(tok, "url_match"),
        "rules": [[k, [orth(sp) for sp in v]] for k, v in sorted(tok.rules.items())],
    }
    with open(out, "w", encoding="utf-8") as f:
        json.dump(data, f, ensure_ascii=False)
    print(f"wrote {out}: {len(data['rules'])} rules, spaCy {spacy.__version__}")


def export_tables(out):
    w, d, s = (re.compile(p) for p in (r"\w", r"\d", r"\s"))
    data = {
        "python_version": sys.version.split()[0],
        "py_isspace": ranges(str.isspace),
        "py_w": ranges(lambda c: w.match(c) is not None),
        "py_d": ranges(lambda c: d.match(c) is not None),
        "py_s": ranges(lambda c: s.match(c) is not None),
    }
    with open(out, "w", encoding="utf-8") as f:
        json.dump(data, f)
    print(f"wrote {out}")


def main():
    args = sys.argv[1:]
    if args[:1] == ["tables"]:
        return export_tables(args[1] if len(args) > 1 else f"{ROOT}/data/py_tables.json")
    out_dir = f"{ROOT}/data/rules"
    if "--out-dir" in args:
        k = args.index("--out-dir")
        out_dir = args[k + 1]
        del args[k : k + 2]
    os.makedirs(out_dir, exist_ok=True)
    for lang in args:
        export_lang(lang, f"{out_dir}/{lang}.json")


if __name__ == "__main__":
    main()
