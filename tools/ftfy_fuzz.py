"""Docs built to trigger every ftfy fixer: mojibake (whole lines, spans, doubled, CESU-8, Java nulls, lost bytes,
A0 turned into spaces) in each encoding ftfy tries, HTML entities (named, uppercased, numeric, invalid), C1 and
other control chars, ANSI escapes, plus the fixers datatrove leaves off (ligatures, widths, quotes, line breaks).

`mojibake_docs(texts, rng)` garbles real text; `fuzz_docs(n, rng, words)` builds synthetic docs from `words`.
"""

import html.entities
import random

import ftfy.bad_codecs  # noqa: F401 - registers sloppy-windows-*

ENCODINGS = [
    "latin-1", "sloppy-windows-1252", "sloppy-windows-1251", "sloppy-windows-1250", "sloppy-windows-1253",
    "sloppy-windows-1254", "sloppy-windows-1257", "iso-8859-2", "macroman", "cp437", "windows-1252",
    "windows-1251", "cp850", "iso-8859-7", "koi8-r",
]  # fmt: skip
WORDS = [
    "café", "naïve", "Größe", "señor", "€5", "Москва", "Ελλάδα", "日本語", "“quote”", "it’s", "½", "ﬁne", "Ａ",
    "à", "às", "à la", "às vezes", "Ã", "Å", "voilà", "façade", "Œuvre", "ŉ", "ǅ", "Ｈｅｌｌｏ", "ﾀｰﾝ", "한국어",
    "💩", "😀", "𝒥", "™", "…", "—", "–", "•", "©", "®", "°", "±", "×", "÷", "¿", "¡", "«", "»", "‘", "’", "„",
    "ğ", "ş", "ı", "ž", "č", "ł", "ő", "ų", "ā", "ē", "Ā", "Ș", "ț", "ẞ", "ß", "ﬀ", "ﬃ", "ʼ", "‚", "†", "‰",
    "Ω", "µ", "π", "ж", "Ё", "ї", "ґ", "ı", "ö", "ü", "ä", "é", "è", "ê", "ë", "í", "ó", "ú", "ñ", "ç",
]  # fmt: skip
ENTITY_NAMES = sorted(html.entities.html5)
CONTROLS = [chr(c) for c in [*range(0, 32), 0x7F, *range(0x80, 0xA0), 0x206A, 0x206F, 0xFEFF, 0xFFF9, 0xFFFC]]
NUMS = [0, 9, 10, 13, 38, 59, 60, 65, 0x7F, 0x80, 0x81, 0x8D, 0x9F, 0xA0, 0xE9, 0x2019, 0xD800, 0xDFFF, 0xFDD0,
        0xFFFE, 0x1FFFF, 0x10FFFF, 0x110000, 10**20, 0x1F4A9, 0xFFFD]  # fmt: skip
PIECES = [
    " ", " ", "\n", "\r\n", "\t", "\xa0", "<", ">", "&", "&amp;", "&;", "&#;", "&#x;", "&#12ab;", "&#xZZ;", "&nbsp",
    "\x1b[31m", "\x1b[0;1;4m", "\x1b[m", "\x1b[١٢m", "\x1b[", "\x1a", "?", "�", " ", " ", "\x85",
    "À", "Àx", "À€", "í", "í ", "Ã ", " Ã ", "Ã la", "Ã quele", "Ã s ", "â€", "â€\x9d", "â€œ", "Â", "Â ", "Ã©",
]  # fmt: skip


def garble(text: str, rng: random.Random) -> str:
    """`text` as UTF-8 bytes decoded in a single-byte encoding, sometimes with ftfy's known extra damage."""
    data = text.encode("utf-8")
    r = rng.random()
    if r < 0.08:  # CESU-8: astral chars as surrogate pairs
        data = text.encode("utf-16-le", "surrogatepass").decode("utf-16-le", "surrogatepass")
        data = data.encode("utf-8", "surrogatepass")
    elif r < 0.12:  # Java's two-byte null
        data = data.replace(b"\x00", b"\xc0\x80") + rng.choice([b"\xc0\x80", b"", b"\xc0"])
    out = data.decode(rng.choice(ENCODINGS), errors="replace")
    r = rng.random()
    if r < 0.2:
        out = out.replace("\xa0", " ")
    elif r < 0.3:
        out = "".join(c if rng.random() > 0.1 or ord(c) < 0x80 else rng.choice("�?") for c in out)
    elif r < 0.35:
        out = garble(out, rng)
    return out


def entity(rng: random.Random) -> str:
    r = rng.random()
    if r < 0.4:
        name = rng.choice(ENTITY_NAMES)
        name = name.upper() if rng.random() < 0.3 else name
        return "&" + (name if name.endswith(";") or rng.random() < 0.5 else name + ";")
    num = rng.choice(NUMS) if rng.random() < 0.6 else rng.randrange(0x30000)
    if rng.random() < 0.5:
        return f"&#{'x' if rng.random() < 0.7 else 'X'}{num:x};"
    return f"&#{'0' * rng.randrange(3)}{num};"


def mojibake_docs(texts: list[str], rng: random.Random) -> list[str]:
    """Real docs with garbled lines, spans or whole text."""
    out = []
    for text in texts:
        r = rng.random()
        if r < 0.3:
            out.append(garble(text, rng))
            continue
        lines = text.split("\n")
        for i, line in enumerate(lines):
            if rng.random() < 0.3 and line:
                a = rng.randrange(len(line))
                b = rng.randrange(a, min(len(line), a + 40) + 1)
                lines[i] = line[:a] + garble(line[a:b], rng) + line[b:]
        out.append("\n".join(lines))
    return out


def fuzz_docs(n: int, rng: random.Random, words=WORDS) -> list[str]:
    docs = []
    for _ in range(n):
        parts = []
        for _ in range(rng.randrange(1, 40)):
            r = rng.random()
            if r < 0.3:
                parts.append(garble(" ".join(rng.choices(words, k=rng.randrange(1, 4))), rng))
            elif r < 0.5:
                parts.append(rng.choice(words))
            elif r < 0.65:
                parts.append(entity(rng))
            elif r < 0.75:
                parts.append(rng.choice(CONTROLS))
            else:
                parts.append(rng.choice(PIECES))
        docs.append("".join(parts))
    return docs
