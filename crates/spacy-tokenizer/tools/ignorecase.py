"""Rewrite a Python `re.IGNORECASE` pattern into an equivalent case-sensitive one.

Every single-char atom (literal, escape, class) becomes the exact set of code points Python's `re` matches with
IGNORECASE, found by running the atom over a string of all code points, so sre's lower/upper rules and its
special equivalences (ı/i, ſ/s, K/k, µ/μ, ...) are reproduced without re-implementing them. Atoms whose set doesn't
change are kept verbatim. The syntax walked is what `src/pyre.rs` accepts.
"""

import re

_ALL = "".join(map(chr, range(0x110000)))
_SURROGATES = (0xD800, 0xDFFF)


def _match_set(atom: str, flags: int) -> list[int]:
    return [m.start() for m in re.compile(atom, flags).finditer(_ALL)]


def _esc(cp: int) -> str:
    return f"\\u{cp:04X}" if cp <= 0xFFFF else f"\\U{cp:08X}"


def _as_class(cps: list[int]) -> str:
    """Class of `cps` as ranges, minus surrogates (no Rust `&str` holds one)."""
    cps = [c for c in cps if not _SURROGATES[0] <= c <= _SURROGATES[1]]
    out, i = [], 0
    while i < len(cps):
        j = i
        while j + 1 < len(cps) and cps[j + 1] == cps[j] + 1:
            j += 1
        out.append(_esc(cps[i]) if i == j else f"{_esc(cps[i])}-{_esc(cps[j])}")
        i = j + 1
    return "[" + "".join(out) + "]"


def _class_end(p: str, i: int) -> int:
    j = i + 1
    if p[j : j + 1] == "^":
        j += 1
    if p[j : j + 1] == "]":
        j += 1
    while p[j] != "]":
        j += 2 if p[j] == "\\" else 1
    return j + 1


def _escape_end(p: str, i: int) -> int:
    width = {"u": 4, "U": 8, "x": 2}.get(p[i + 1], 0)
    return i + 2 + width


_ZERO_WIDTH = {"\\b", "\\B", "\\A", "\\Z"}
_FLAGS = re.compile(r"\(\?([aiLmsux]+)\)")
_QUANT = re.compile(r"\{\d*,?\d*\}")


def expand(pattern: str, flags: int) -> str:
    """`pattern` compiled with `flags` (which include IGNORECASE, possibly inline) -> case-sensitive equivalent."""
    base = flags & ~re.IGNORECASE
    cache: dict[str, str] = {}

    def rewrite(atom: str) -> str:
        if atom not in cache:
            cs, ci = _match_set(atom, base), _match_set(atom, base | re.IGNORECASE)
            cache[atom] = atom if cs == ci else _as_class(ci)
        return cache[atom]

    out, i = [], 0
    while i < len(pattern):
        c = pattern[i]
        if m := _FLAGS.match(pattern, i):
            rest = m.group(1).replace("i", "")
            out.append(f"(?{rest})" if rest else "")
            j = m.end()
        elif c == "\\":
            j = _escape_end(pattern, i)
            atom = pattern[i:j]
            out.append(atom if atom in _ZERO_WIDTH else rewrite(atom))
        elif c == "[":
            j = _class_end(pattern, i)
            out.append(rewrite(pattern[i:j]))
        elif c == "(" and pattern[i + 1 : i + 2] == "?":
            # Group heads like `(?:`, `(?<=`, `(?P<name>` are syntax, not literals.
            j = pattern.index(">", i) + 1 if pattern.startswith("(?P<", i) else i + 2
            while pattern[j] in ":=!<":
                j += 1
            out.append(pattern[i:j])
        elif m := _QUANT.match(pattern, i):
            j = m.end()
            out.append(m.group())
        elif c in "()|*+?^$.{}":
            j = i + 1
            out.append(c)
        else:
            j = i + 1
            out.append(rewrite(re.escape(c)))
        i = j
    return "".join(out)


def check(original: re.Pattern, expanded: str, samples: list[str]) -> None:
    """Both patterns must agree, as spaCy calls them (search/finditer/match), on every sample and its case variants."""
    new = re.compile(expanded, original.flags & ~re.IGNORECASE)
    for s in samples:
        for v in {s, s.upper(), s.lower(), s.title(), s.swapcase()}:
            a = [(x.start(), x.end()) for x in original.finditer(v)]
            b = [(x.start(), x.end()) for x in new.finditer(v)]
            if a != b or bool(original.match(v)) != bool(new.match(v)):
                raise SystemExit(f"IGNORECASE expansion differs on {v!r}: {a} vs {b}")
