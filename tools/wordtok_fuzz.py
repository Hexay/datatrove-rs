"""Adversarial docs for `wordtok_parity.py`: corpus words and slices spliced with script letters, digits in several
scripts, separators, whitespace variants and the patterns each ported tokenizer special-cases."""

import random

COMMON = [
    " ", "  ", "   ", "\t", " \t ", "\n", "\n\n", "\r\n", "\r", "\x0b", "\x0c", "\x1c", "\x85", " ", " ",
    "​", "‌", "‍", "　", "﻿", "́", "0", "7", "42", "2024", "١٢", "۳", "१२", "৫", "๑๒",
    "೭", ",", ".", ":", "/", "-", "--", "!", "?", "...", "\\", '"', "'", "(", ")", "[", "]", "{", "}", "_", "@", "#",
    "|", "~", "`", "^", "।", "॥", "؉", "،", "؛", "۔", "٪", "۔", "꫰", "᱾", "abc", "XYZ", "A.",
    "Dr.", "e.g.", "1, 2", "1 , 2", "10 . 5", "3:30", "3 : 30 : 15", "1/2/2024", "1,234.56", "1.2.3", "12,3 4",
    "0 , 1", "http://x.y/z?a=1", "a@b.co", "😀", "Ⅻ", "½", "²",
]


def _letters(lo, hi):
    return [chr(c) for c in range(lo, hi) if chr(c).isprintable()]


def _script_pieces(name):
    if name == "vi":
        from pyvi import ViTokenizer as V

        grams = sorted(V.ViTokenizer.bi_grams | V.ViTokenizer.tri_grams)
        rng = random.Random(1)
        letters = "aăâbcdđeêghiklmnoôơpqrstuưvxyàảãáạằẳẵắặầẩẫấậèẻẽéẹềểễếệìỉĩíịòỏõóọồổỗốộờởỡớợùủũúụừửữứựỳỷỹýỵ"
        abbr = ["Tp.", "TP.", "Mr.", "Mrs.", "Ms.", "Dr.", "ThS.", "UBND.", "Đ.", "==>", "->", ">>", "...", "1.000,5"]
        sample = rng.sample(grams, 3000)
        return list(letters + letters.upper()) + sample + [g.title() for g in sample[:500]] + abbr + ["3_4", "a_b"]
    if name == "thai":
        from pythainlp.tokenize import word_dict_trie

        words = sorted(word_dict_trie())
        rng = random.Random(1)
        return _letters(0xE01, 0xE5C) + rng.sample(words, 3000) + ["ๆ", "ฯ", "ก็", "เก็บ", "๑,๒๓๔.๕๖"]
    from indicnlp import langinfo
    from indicnlp.transliterate.unicode_transliterate import UnicodeIndicTransliterator as tr
    from indicnlp.tokenize.sentence_tokenize import is_acronym_abbvr

    lang = name.split(":", 1)[1]
    if lang == "ur":
        return _letters(0x600, 0x700) + _letters(0x750, 0x780)
    base = langinfo.SCRIPT_RANGES[lang][0]
    acks = [c for c in is_acronym_abbvr.__code__.co_consts if isinstance(c, frozenset)][0]
    ack_native = [tr.transliterate(a, "hi", lang) for a in sorted(acks)]
    return _letters(base, base + 0x80) + [a + "." for a in ack_native] + [" " + a + ". " for a in ack_native]


def fuzz_docs(docs, n, seed, name):
    rng = random.Random(seed)
    words = [w for d in docs[:500] for w in d.split()][:200_000] or ["x"]
    script = _script_pieces(name)
    out = []
    for _ in range(n):
        parts = []
        for _ in range(rng.randint(1, 120)):
            r = rng.random()
            if r < 0.3:
                parts.append(rng.choice(words))
            elif r < 0.55:
                parts.append(rng.choice(script))
            elif r < 0.85:
                parts.append(rng.choice(COMMON))
            elif r < 0.95 and docs:
                d = rng.choice(docs)
                a = rng.randrange(len(d))
                parts.append(d[a : a + rng.randint(1, 300)])
            else:
                parts.append("".join(rng.choice(script) for _ in range(rng.randint(50, 400))))
        sep = rng.choice(["", " ", " ", "\n"])
        out.append(sep.join(parts))
    return out
