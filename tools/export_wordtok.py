"""Export the data of datatrove's non-spaCy word tokenizers ported to Rust (`crates/datatrove-core/src/wordtok`).

Character classes are probed from the libraries' own compiled regexes, so they hold for the installed versions.
Re-run after upgrading indic-nlp-library or pythainlp, then update `VERIFIED` in `python/datatrove_rs/_wordtok.py`.

Usage: export_wordtok.py [out_dir]   (default crates/datatrove-core/data/wordtok)
"""

import json
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
from export_pytext import ranges  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def char_class(pat):
    return ranges(lambda c: pat.fullmatch(c) is not None)


def indic():
    from indicnlp import langinfo
    from indicnlp.tokenize import indic_tokenize as it
    from indicnlp.tokenize import sentence_tokenize as st
    from indicnlp.transliterate.sinhala_transliterator import SinhalaDevanagariTransliterator as sdt

    ack = st.is_acronym_abbvr.__code__.co_consts
    ack_chars = next(c for c in ack if isinstance(c, frozenset))
    return {
        "indic_punct": char_class(it.triv_tokenizer_indic_pat),
        "urdu_punct": char_class(it.triv_tokenizer_urdu_pat),
        "delim_danda": char_class(st.DELIM_PAT_DANDA),
        "delim_no_danda": char_class(st.DELIM_PAT_NO_DANDA),
        "contains_danda": char_class(st.CONTAINS_DANDA),
        "isnumeric": ranges(str.isnumeric),
        "danda_langs": langinfo.DANDA_DELIM_LANGUAGES,
        "script_base": {lang: r[0] for lang, r in langinfo.SCRIPT_RANGES.items()},
        "coordinated_end": langinfo.COORDINATED_RANGE_END_INCLUSIVE,
        "sinhala_to_devanagari": sorted((ord(k), ord(v)) for k, v in sdt.sinhala_devnag_map.items()),
        "ack_chars": sorted(ack_chars),
    }


def thai_words():
    from pythainlp.tokenize import word_dict_trie

    return sorted(word_dict_trie())


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "crates/datatrove-core/data/wordtok")
    os.makedirs(out, exist_ok=True)
    with open(os.path.join(out, "indic.json"), "w", encoding="utf-8") as f:
        json.dump(indic(), f, ensure_ascii=False, separators=(",", ":"))
    words = thai_words()
    assert all(w and "\n" not in w for w in words)
    with open(os.path.join(out, "thai_words.txt"), "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(words))
    print(f"wrote {out}: thai words {len(words)}")


if __name__ == "__main__":
    main()
