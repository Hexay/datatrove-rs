"""Export the ftfy data the Rust `fix_text` port uses (`crates/datatrove-core/data/ftfy.json`).

Regexes are translated from ftfy's compiled patterns (`ftfy_check.to_rust`), with `\\d`/`\\w`/`\\s` taken from the
running Python's `re`; charmaps come from the codecs ftfy registers; HTML tables from ftfy and `html`. Patterns the
port reimplements by hand (lookaheads, the utf-8-variants decoder) are asserted to be unchanged. Re-run after
upgrading ftfy or Python, then rerun `tools/ftfy_parity.py` and bump `VERIFIED_FTFY` in `ftfy_check.py`.

Usage: export_ftfy.py [out.json]
"""

import html
import json
import re
import sys
import unicodedata
from pathlib import Path

import ftfy
import ftfy.bad_codecs  # noqa: F401 - registers sloppy-windows-*, utf-8-variants
from ftfy import badness, chardata, fixes
from ftfy.bad_codecs import utf8_variants

from datatrove_rs.ftfy_check import _c, _key_ranges, _ranges, to_rust

OUT = Path(__file__).resolve().parents[1] / "crates/datatrove-core/data/ftfy.json"

# Patterns reimplemented by hand in crates/datatrove-core/src/ftfy; a change here needs a code change there.
HAND_PORTED = {
    "A_GRAVE_WORD_RE": (fixes.A_GRAVE_WORD_RE, b"\xc3 (?! |quele|quela|quilo|s )"),
    "C1_CONTROL_RE": (chardata.C1_CONTROL_RE, "[\\x80-\\x9f]"),
    "SPECIAL_BYTES_RE": (
        utf8_variants.SPECIAL_BYTES_RE,
        b"(\xc0(\x80|$))|(\xed([\xa0-\xaf]|$)([\x80-\xbf]|$)(\xed|$)([\xb0-\xbf]|$)([\x80-\xbf]|$))"
        b"|(\xed([\xa0-\xbf]|$)([\x80-\xbf]|$))",
    ),
    "CESU8_RE": (
        utf8_variants.CESU8_RE,
        b"(\xed([\xa0-\xaf]|$)([\x80-\xbf]|$)(\xed|$)([\xb0-\xbf]|$)([\x80-\xbf]|$))",
    ),
    "html._charref": (html._charref, r"&(#[0-9]+;?|#[xX][0-9a-fA-F]+;?|[^\t\n\f <&#;]{1,32};?)"),
}
ENCODINGS = [
    "latin-1", "sloppy-windows-1252", "sloppy-windows-1251", "sloppy-windows-1250", "sloppy-windows-1253",
    "sloppy-windows-1254", "sloppy-windows-1257", "iso-8859-2", "macroman", "cp437",
]  # fmt: skip


def categories():
    def ranges_of(pat):
        rx = re.compile(pat)
        out = []
        for cp in range(0x110000):
            if rx.match(chr(cp)):
                if out and out[-1][1] == cp - 1:
                    out[-1][1] = cp
                else:
                    out.append([cp, cp])
        return _ranges(out)

    space, word, digit = (ranges_of(p) for p in (r"\s", r"\w", r"\d"))
    return {
        _c.CATEGORY_SPACE: f"[{space}]",
        _c.CATEGORY_NOT_SPACE: f"[^{space}]",
        _c.CATEGORY_WORD: f"[{word}]",
        _c.CATEGORY_NOT_WORD: f"[^{word}]",
        _c.CATEGORY_DIGIT: f"[{digit}]",
        _c.CATEGORY_NOT_DIGIT: f"[^{digit}]",
    }


def charmap(encoding: str) -> list[str]:
    """Byte -> char for the chars `possible_encoding(text, encoding)` accepts, checked against ftfy's regex."""
    allowed = chardata.ENCODING_REGEXES[encoding]
    table: dict[int, str] = {}
    for cp in range(0x110000):
        ch = chr(cp)
        if 0xD800 <= cp < 0xE000 or not allowed.match(ch):
            continue
        b = ch.encode(encoding)
        assert len(b) == 1 and b[0] not in table, (encoding, ch)
        assert b.decode(encoding) == ch, (encoding, ch)
        table[b[0]] = ch
    assert sorted(table) == list(range(256)), encoding
    return [table[b] for b in range(256)]


def decode_table(encoding: str) -> list[str | None]:
    out = []
    for b in range(256):
        try:
            out.append(bytes([b]).decode(encoding))
        except UnicodeDecodeError:
            out.append(None)
    return out


def main(out: Path) -> None:
    for name, (rx, expected) in HAND_PORTED.items():
        assert rx.pattern == expected, f"ftfy {name} changed; update the Rust port"
    assert chardata.CHARMAP_ENCODINGS == ENCODINGS, "ftfy CHARMAP_ENCODINGS changed"
    assert set(html._invalid_charrefs) | set(html._invalid_codepoints) <= set(range(0x110000))
    cats = categories()
    behind, detector = to_rust(chardata.UTF8_DETECTOR_RE, cats, lookbehind=True)
    data = {
        "ftfy_version": ftfy.__version__,
        "python_version": sys.version.split()[0],
        "unidata_version": unicodedata.unidata_version,
        "badness": to_rust(badness.BADNESS_RE, cats, begin="\\A"),
        "badness_lines": to_rust(badness.BADNESS_RE, cats),
        "html_entity": to_rust(chardata.HTML_ENTITY_RE, cats),
        "ansi": to_rust(fixes.ANSI_RE, cats),
        "utf8_detector": detector,
        "utf8_detector_behind": behind,
        "altered_utf8": to_rust(chardata.ALTERED_UTF8_RE),
        "lossy_utf8": to_rust(chardata.LOSSY_UTF8_RE),
        "a_grave_exceptions": [" ", "quele", "quela", "quilo", "s "],
        "html_entities": dict(sorted(chardata.HTML_ENTITIES.items())),
        "invalid_charrefs": sorted(html._invalid_charrefs.items()),
        "invalid_codepoints": sorted(html._invalid_codepoints),
        "control_chars": _key_ranges(chardata.CONTROL_CHARS),
        "encodings": [{"name": e, "chars": charmap(e)} for e in ENCODINGS],
        "windows_1252": decode_table("windows-1252"),
        "sloppy_windows_1252": decode_table("sloppy-windows-1252"),
    }
    out.write_text(json.dumps(data, ensure_ascii=True, separators=(",", ":")) + "\n")
    print(f"wrote {out} ({out.stat().st_size:,} bytes), ftfy {ftfy.__version__}, Unicode {data['unidata_version']}")


if __name__ == "__main__":
    main(Path(sys.argv[1]) if len(sys.argv) > 1 else OUT)
