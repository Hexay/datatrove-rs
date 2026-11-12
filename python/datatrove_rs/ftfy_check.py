"""A sound "`ftfy.fix_text` leaves this text unchanged" check, built from ftfy's own regexes and tables.

ftfy fixes text segment by segment (lines, cut at `max_decode_length`) and stops once every fixer is a no-op.
Each enabled fixer is a no-op on a segment when: `unescape_html` finds no `HTML_ENTITY_RE` match; `fix_encoding`
sees `not is_bad(segment)`, i.e. no `BADNESS_RE` match; the regex fixers (C1 controls, quotes, terminal escapes)
find no match; the `str.translate` fixers (ligatures, width, control chars) find none of their table's keys.
The regexes are translated to Rust from Python's parse tree, so they match exactly the same strings. They have no
lookarounds; `^` becomes `(?m:^)`, which is every segment start when the text is at most `max_decode_length` long,
so a match inside any segment is a match in the whole text. Lone surrogates can't reach Rust (`str` -> UTF-8
fails), so `fix_surrogates` needs no check. `fix_line_breaks` and `normalization` have no fast check.
"""

import re

try:
    from re import _constants as _c
    from re import _parser as _p
except ImportError:  # Python 3.10
    import sre_constants as _c
    import sre_parse as _p

from datatrove_rs import _core

_KNOWN_FIELDS = {
    "unescape_html", "remove_terminal_escapes", "fix_encoding", "restore_byte_a0", "replace_lossy_sequences",
    "decode_inconsistent_utf8", "fix_c1_controls", "fix_latin_ligatures", "fix_character_width", "uncurl_quotes",
    "fix_line_breaks", "fix_surrogates", "remove_control_chars", "normalization", "max_decode_length", "explain",
}  # fmt: skip


class Untranslatable(ValueError):
    pass


def _ranges(ranges) -> str:
    return "".join(_lit(lo) if lo == hi else f"{_lit(lo)}-{_lit(hi)}" for lo, hi in ranges)


def _categories():
    space, word, digit = (_ranges(r) for r in _core.re_category_ranges())
    return {
        _c.CATEGORY_SPACE: f"[{space}]",
        _c.CATEGORY_NOT_SPACE: f"[^{space}]",
        _c.CATEGORY_WORD: f"[{word}]",
        _c.CATEGORY_NOT_WORD: f"[^{word}]",
        _c.CATEGORY_DIGIT: f"[{digit}]",
        _c.CATEGORY_NOT_DIGIT: f"[^{digit}]",
    }


def _lit(cp: int) -> str:
    return f"\\x{{{cp:x}}}"


def to_rust(pattern: re.Pattern, cats=None) -> str:
    """The Rust regex matching the same strings as `pattern` (`^` as `(?m:^)`); raises `Untranslatable`."""
    if not isinstance(pattern.pattern, str) or pattern.flags & ~(re.UNICODE | re.VERBOSE):
        raise Untranslatable(f"flags {pattern.flags}")
    return _seq(_p.parse(pattern.pattern, pattern.flags), cats or _categories())


def _seq(items, cats) -> str:
    return "".join(_item(op, av, cats) for op, av in items)


def _item(op, av, cats) -> str:
    if op == _c.LITERAL:
        return _lit(av)
    if op == _c.NOT_LITERAL:
        return f"[^{_lit(av)}]"
    if op == _c.ANY:
        return "[^\\n]"
    if op == _c.IN:
        return _class(av, cats)
    if op == _c.BRANCH:
        return "(?:" + "|".join(_seq(b, cats) for b in av[1]) + ")"
    if op == _c.SUBPATTERN and not av[1] and not av[2]:
        return f"(?:{_seq(av[3], cats)})"
    if op in (_c.MAX_REPEAT, _c.MIN_REPEAT):
        lo, hi, sub = av
        hi = "" if hi == _c.MAXREPEAT else hi
        return f"(?:{_seq(sub, cats)}){{{lo},{hi}}}" + ("?" if op == _c.MIN_REPEAT else "")
    if op == _c.AT and av == _c.AT_BEGINNING:
        return "(?m:^)"
    raise Untranslatable(str(op))


def _class(items, cats) -> str:
    out = ["["]
    for op, av in items:
        if op == _c.NEGATE:
            out.append("^")
        elif op == _c.LITERAL:
            out.append(_lit(av))
        elif op == _c.RANGE:
            out.append(f"{_lit(av[0])}-{_lit(av[1])}")
        elif op == _c.CATEGORY and av in cats:
            out.append(cats[av])
        else:
            raise Untranslatable(str(op))
    return "".join(out) + "]"


def _key_ranges(*tables) -> list[tuple[int, int]]:
    cps = sorted({k for t in tables for k in t})
    out: list[list[int]] = []
    for cp in cps:
        if out and out[-1][1] == cp - 1:
            out[-1][1] = cp
        else:
            out.append([cp, cp])
    return [(lo, hi) for lo, hi in out]


def build(config):
    """`(_core.FtfyCheck, max_len)` for a `TextFixerConfig`, or None when no exact fast check exists for it."""
    import ftfy
    from ftfy import badness, fixes

    if set(config._fields) != _KNOWN_FIELDS or not ftfy.__version__.startswith("6."):
        return None
    if config.fix_line_breaks or config.normalization is not None:
        return None
    regexes, tables = [], []
    if config.unescape_html:
        regexes.append(fixes.HTML_ENTITY_RE)
    if config.fix_encoding:
        regexes.append(badness.BADNESS_RE)
    if config.fix_c1_controls:
        regexes.append(fixes.C1_CONTROL_RE)
    if config.uncurl_quotes:
        regexes += [fixes.SINGLE_QUOTE_RE, fixes.DOUBLE_QUOTE_RE]
    if config.remove_terminal_escapes:
        regexes.append(fixes.ANSI_RE)
    if config.fix_latin_ligatures:
        tables.append(fixes.LIGATURES)
    if config.fix_character_width:
        tables.append(fixes.WIDTH_MAP)
    if config.remove_control_chars:
        tables.append(fixes.CONTROL_CHARS)
    try:
        cats = _categories()
        patterns = [to_rust(r, cats) for r in regexes]
    except Untranslatable:
        return None
    return _core.FtfyCheck(_key_ranges(*tables), patterns), config.max_decode_length
