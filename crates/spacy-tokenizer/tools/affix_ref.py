"""Regex-level parity corpus + reference.

  affix_ref.py corpus <docs.bin> <out_strings.bin> [n_random] [lang]  unique ws-chunks + rule keys + fuzz
  affix_ref.py ref <strings.bin> <out.txt> [lang]                     spaCy find_prefix/suffix/infix + url/token_match
  affix_ref.py diff <strings.bin> <a.txt> <b.txt>                     list differing strings
"""

import random
import sys
from collections import Counter

import spacy

BASE_ALPHABET = list("aZé9٣.,-–—~+*^:<>=/()[]{}'\"’%$€°Fk…!?@#&_«»„“”‚‘") + [
    "km", "US$", "a.m", "'s", "http://", "www.", ".com", "l'", "d’", "-t-", "ſ", "ı", "İ", "K", "ß", "ẞ", "µ", "ς",
]


def corpus(docs_path, out, n_random=200_000, lang="en"):
    docs = open(docs_path, "rb").read().decode("utf-8").split("\0")
    chunks = {c for d in docs for c in d.split()}
    keys = list(spacy.blank(lang).tokenizer.rules)
    chunks.update(keys)
    chunks.update(v for k in keys for v in (k.upper(), k.title(), k.lower()))
    # The corpus' most frequent letters, so random strings exercise the language's own script.
    letters = Counter(ch for c in list(chunks)[:200_000] for ch in c if ch.isalpha())
    alphabet = BASE_ALPHABET + [ch for ch, _ in letters.most_common(40)] + keys[:200]
    rng = random.Random(0)
    for _ in range(int(n_random)):
        chunks.add("".join(rng.choice(alphabet) for _ in range(rng.randint(1, 8))))
    # The tokenizer only ever sees all-space or space-free chunks (pyre.rs relies on it for Python's `$`).
    items = sorted(c for c in chunks if c and (c.isspace() or not any(ch.isspace() for ch in c)))
    with open(out, "wb") as f:
        f.write("\0".join(items).encode("utf-8"))
    print(f"{len(items)} strings")


def ref(path, out, lang="en"):
    tok = spacy.blank(lang).tokenizer
    strings = open(path, "rb").read().decode("utf-8").split("\0")
    with open(out, "w", encoding="utf-8", newline="\n") as f:
        for s in strings:
            infs = ",".join(f"{m.start()}-{m.end()}" for m in tok.find_infix(s))
            url = int(tok.url_match is not None and tok.url_match(s) is not None)
            tm = int(tok.token_match is not None and tok.token_match(s) is not None)
            f.write(f"{tok.find_prefix(s) or 0} {tok.find_suffix(s) or 0} {url} {tm} {infs}\n")


def diff(path, a, b):
    strings = open(path, "rb").read().decode("utf-8").split("\0")
    la = open(a, encoding="utf-8").read().split("\n")
    lb = open(b, encoding="utf-8").read().split("\n")
    bad = [(s, x, y) for s, x, y in zip(strings, la, lb) if x != y]
    print(f"{len(strings)} strings, {len(bad)} differ")
    for s, x, y in bad[:40]:
        print(f"  {s!r}: ref={x!r} rust={y!r}")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    cmd, *a = sys.argv[1:]
    {"corpus": corpus, "ref": ref, "diff": diff}[cmd](*a)
