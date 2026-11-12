"""Dump the Python/`regex`/spaCy/datatrove character data datatrove's filters rely on to JSON.

Rust's Unicode tables are a different version and some definitions differ (`isalpha` vs
`char::is_alphabetic`), so datatrove-core loads these instead. Re-run after upgrading any of them.

Usage: export_pytext.py [out.json]
"""

import json
import re
import sys
import unicodedata

import regex
import spacy
from datatrove.utils import text as dt_text

MAX = 0x110000


def ranges(pred):
    out, lo = [], None
    for cp in range(MAX):
        ok = pred(chr(cp))
        if ok and lo is None:
            lo = cp
        elif not ok and lo is not None:
            out.append([lo, cp - 1])
            lo = None
    if lo is not None:
        out.append([lo, MAX - 1])
    return out


def sigma_classes():
    # CPython's final-sigma rule needs Case_Ignorable and Cased, which unicodedata doesn't expose.
    # Probe them: (X+"Σ") lowers to "ς" iff X is cased and not ignorable; ("A"+X+"Σ") iff X is
    # ignorable or cased. Cased-ness of ignorable chars never matters (they are skipped both ways).
    ignorable, cased = [], []
    for cp in range(MAX):
        c = chr(cp)
        p1 = (c + "Σ").lower()[-1] == "ς"
        p2 = ("A" + c + "Σ").lower()[-1] == "ς"
        if p1:
            cased.append(cp)
        elif p2:
            ignorable.append(cp)
    return to_ranges(ignorable), to_ranges(cased)


def to_ranges(cps):
    out = []
    for cp in cps:
        if out and out[-1][1] == cp - 1:
            out[-1][1] = cp
        else:
            out.append([cp, cp])
    return out


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else "crates/datatrove-core/data/py_unicode.json"
    re_d, re_s, re_w, rx_nd = re.compile(r"\d"), re.compile(r"\s"), re.compile(r"\w"), regex.compile(r"\p{Nd}")
    ignorable, cased = sigma_classes()
    ccc = [[lo, hi, unicodedata.combining(chr(lo))] for lo, hi in ranges(lambda c: unicodedata.combining(c) > 0)]
    # split multi-valued ranges so each carries a single combining class
    ccc_exact = []
    for lo, hi, _ in ccc:
        for cp in range(lo, hi + 1):
            v = unicodedata.combining(chr(cp))
            if ccc_exact and ccc_exact[-1][1] == cp - 1 and ccc_exact[-1][2] == v:
                ccc_exact[-1][1] = cp
            else:
                ccc_exact.append([cp, cp, v])
    data = {
        "python_version": sys.version.split()[0],
        "unidata_version": unicodedata.unidata_version,
        "regex_version": regex.__version__,
        "isspace": ranges(str.isspace),
        "isalpha": ranges(str.isalpha),
        "punct": ranges(lambda c: unicodedata.category(c).startswith("P")),
        "mn": ranges(lambda c: unicodedata.category(c) == "Mn"),
        "re_d": ranges(lambda c: re_d.match(c) is not None),
        "re_s": ranges(lambda c: re_s.match(c) is not None),
        "re_w": ranges(lambda c: re_w.match(c) is not None),
        "regex_nd": ranges(lambda c: rx_nd.match(c) is not None),
        "linebreak": ranges(lambda c: len(("a" + c + "b").splitlines()) == 2),
        "sigma_ignorable": ignorable,
        "sigma_cased": cased,
        "ccc": ccc_exact,
        "lower": [[cp, chr(cp).lower()] for cp in range(MAX) if chr(cp).lower() != chr(cp) and cp != 0x3A3],
        "nfd": [[cp, d] for cp in range(MAX) if (d := unicodedata.normalize("NFD", chr(cp))) != chr(cp)],
        "sentencizer_punct": sorted(spacy.blank("en").add_pipe("sentencizer").punct_chars),
        "dt_punctuation": dt_text.PUNCTUATION,
        "dt_punctuation_set": sorted(dt_text.PUNCTUATION_SET),
    }
    with open(out, "w", encoding="utf-8") as f:
        json.dump(data, f, ensure_ascii=False, separators=(",", ":"))
    print(f"wrote {out}: Python {data['python_version']}, UCD {data['unidata_version']}, regex {data['regex_version']}")


if __name__ == "__main__":
    main()
