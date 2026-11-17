"""A sound "`ftfy.fix_text` leaves this text unchanged" check, built from ftfy's own regexes and tables.

ftfy fixes text segment by segment (lines, cut at `max_decode_length`) and stops once every fixer is a no-op.
Each enabled fixer is a no-op on a segment when: `unescape_html` finds no `HTML_ENTITY_RE` match; `fix_encoding`
sees `not is_bad(segment)`, i.e. no `BADNESS_RE` match; the regex fixers (C1 controls, quotes, terminal escapes)
find no match; the `str.translate` fixers (ligatures, width, control chars) find none of their table's keys.
The regexes are translated to Rust from Python's parse tree, so they match exactly the same strings. They have no
lookarounds; `^` becomes `(?m:^)`, which is every segment start when the text is at most `max_decode_length` long,
so a match inside any segment is a match in the whole text. Lone surrogates can't reach Rust (`str` -> UTF-8
fails), so `fix_surrogates` needs no check. `fix_line_breaks` and `normalization` have no fast check.

`fast_path` uses the Rust `fix_text` port (`build_fixer`) where it applies and this check otherwise.
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
# The ftfy release `tools/ftfy_parity.py` verified the Rust port against; the embedded tables must match it.
VERIFIED_FTFY = "6.3.1"
_NOT_PORTED = ("fix_latin_ligatures", "fix_character_width", "uncurl_quotes", "fix_line_breaks")


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


def _byte_lit(b: int) -> str:
    return f"\\x{b:02x}"


class _Ctx:
    def __init__(self, cats, begin, is_bytes):
        self.cats, self.begin, self.lit = cats, begin, _byte_lit if is_bytes else _lit


def to_rust(pattern: re.Pattern, cats=None, begin="(?m:^)", lookbehind=False):
    """The Rust regex matching the same strings as `pattern`, with `^` as `begin`; raises `Untranslatable`.

    Bytes patterns become `(?-u)` patterns for `regex::bytes`. With `lookbehind`, a leading one-char negative
    lookbehind `(?<![...])` is split off and `(class, rest)` is returned for the caller to check by hand.
    """
    is_bytes = isinstance(pattern.pattern, bytes)
    if pattern.flags & ~(re.VERBOSE | (0 if is_bytes else re.UNICODE)):
        raise Untranslatable(f"flags {pattern.flags}")
    ctx = _Ctx({} if is_bytes else cats or _categories(), begin, is_bytes)
    items = list(_p.parse(pattern.pattern, pattern.flags))
    if lookbehind:
        op, av = items.pop(0)
        if op != _c.ASSERT_NOT or av[0] != -1 or len(av[1]) != 1 or av[1][0][0] != _c.IN:
            raise Untranslatable("expected a leading one-char negative lookbehind")
        return _class(av[1][0][1], ctx), _seq(items, ctx)
    return ("(?-u)" if is_bytes else "") + _seq(items, ctx)


def _seq(items, ctx) -> str:
    return "".join(_item(op, av, ctx) for op, av in items)


def _item(op, av, ctx) -> str:
    if op == _c.LITERAL:
        return ctx.lit(av)
    if op == _c.NOT_LITERAL:
        return f"[^{ctx.lit(av)}]"
    if op == _c.ANY:
        return "[^\\n]"
    if op == _c.IN:
        return _class(av, ctx)
    if op == _c.BRANCH:
        return "(?:" + "|".join(_seq(b, ctx) for b in av[1]) + ")"
    if op == _c.SUBPATTERN and not av[1] and not av[2]:
        return f"(?:{_seq(av[3], ctx)})"
    if op in (_c.MAX_REPEAT, _c.MIN_REPEAT):
        lo, hi, sub = av
        hi = "" if hi == _c.MAXREPEAT else hi
        return f"(?:{_seq(sub, ctx)}){{{lo},{hi}}}" + ("?" if op == _c.MIN_REPEAT else "")
    if op == _c.AT and av == _c.AT_BEGINNING:
        return ctx.begin
    raise Untranslatable(str(op))


def _class(items, ctx) -> str:
    out = ["["]
    for op, av in items:
        if op == _c.NEGATE:
            out.append("^")
        elif op == _c.LITERAL:
            out.append(ctx.lit(av))
        elif op == _c.RANGE:
            out.append(f"{ctx.lit(av[0])}-{ctx.lit(av[1])}")
        elif op == _c.CATEGORY and av in ctx.cats:
            out.append(ctx.cats[av])
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


def fast_path(config):
    """A `text -> str | None` giving `ftfy.fix_text(text, config)`, or None where ftfy itself must run; or None
    when nothing in Rust applies to `config`. The full port (`build_fixer`) is preferred over the check."""
    import ftfy

    if set(config._fields) != _KNOWN_FIELDS or not ftfy.__version__.startswith("6."):
        return None
    fixer = build_fixer(config)
    if fixer is not None:

        def fix(text):
            try:
                new = fixer.fix(text)
            except UnicodeEncodeError:
                return None
            return text if new is None else new

        return fix
    built = build(config)
    if built is None:
        return None
    check, max_len = built

    def unchanged(text):
        if len(text) > max_len:
            return None
        try:
            return text if check.first_hit(text) is None else None
        except UnicodeEncodeError:
            return None

    return unchanged


def build_fixer(config):
    """`_core.FtfyFixer` (`crates/datatrove-core/src/ftfy`) for a `TextFixerConfig` it reproduces, else None: the
    installed ftfy and Unicode must be the ones its tables were exported from (`tools/export_ftfy.py`), and the
    fixers it doesn't port (ligatures, width, quotes, line breaks, normalization) must be off."""
    import unicodedata

    import ftfy

    tables_ftfy, _, tables_unicode = _core.ftfy_tables_version()
    if not (ftfy.__version__ == tables_ftfy == VERIFIED_FTFY and unicodedata.unidata_version == tables_unicode):
        return None
    if any(getattr(config, f) for f in _NOT_PORTED) or config.normalization is not None:
        return None
    m = config.max_decode_length
    if type(m) is not int or m <= 0:
        return None
    u = config.unescape_html
    unescape = 2 if isinstance(u, str) and u == "auto" else int(bool(u))
    # without `explain`, fix_and_explain calls `fix_encoding(text)`, which ignores the config
    enc_fields = ("restore_byte_a0", "replace_lossy_sequences", "decode_inconsistent_utf8", "fix_c1_controls")
    enc = tuple(bool(getattr(config, f)) if config.explain else True for f in enc_fields)
    flags = (bool(config.remove_terminal_escapes), bool(config.fix_encoding), enc, bool(config.fix_c1_controls))
    return _core.FtfyFixer(unescape, *flags, bool(config.remove_control_chars), m)


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
