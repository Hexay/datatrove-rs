"""fuzz_docs.py <out.bin> [n_docs] [lang] [seed_docs.bin] — adversarial docs: rule keys (and their case variants)
glued to punctuation, odd whitespace, plus words sampled from real docs in the language."""

import random
import sys

import spacy

punct = list("()[]{}\"'`’“”.,;:!?…-–—~+*^/<>=%$€£#&_@°«»„‚‘") + ["..", "...", "--", "'s", "’s", "US$", "km", "a.m."]
words = ["the", "U.S.", "e-mail", "10am", "5km", "3.5", "1,000", "a/b", "x=y", "hello", "Mr.", "C++", "http://ex.com/a?b=c",
         "www.test.org", "192.168.0.1", "8.8.8.8", "user@mail.com", "ÉCOLE", "straße", "٣٤", "ｆｕｌｌ", "l'homme",
         "aujourd'hui", "New-York", "ſ", "ıİ", "K", "ẞ", "µ"]
spaces = [" ", " ", " ", "  ", "\n", "\n\n", "\t", "\xa0", " \n ", " ", "　", "\x1c", "\r\n", "\x0b"]


def vocab(lang, seed_docs):
    rules = list(spacy.blank(lang).tokenizer.rules)
    rules += [v for k in rules for v in (k.upper(), k.title())]
    native = []
    if seed_docs:
        with open(seed_docs, "rb") as f:
            native = sorted({w for d in f.read().decode("utf-8").split("\0")[:2000] for w in d.split()})
    return rules, words + native


def piece(rng, rules, pool):
    base = rng.choice(rules if rng.random() < 0.5 else pool)
    pre = "".join(rng.choice(punct) for _ in range(rng.choice([0, 0, 1, 2])))
    suf = "".join(rng.choice(punct) for _ in range(rng.choice([0, 0, 1, 2, 3])))
    mid = rng.choice(["", "", "", "-", "/", ".", ",", "'", "’"]) + rng.choice(pool) if rng.random() < 0.1 else ""
    return pre + base + mid + suf


def main():
    out = sys.argv[1]
    n = int(sys.argv[2]) if len(sys.argv) > 2 else 5000
    rules, pool = vocab(sys.argv[3] if len(sys.argv) > 3 else "en", sys.argv[4] if len(sys.argv) > 4 else None)
    rng = random.Random(1)
    docs = []
    for _ in range(n):
        parts = [piece(rng, rules, pool) for _ in range(rng.randint(1, 60))]
        doc = rng.choice(["", " ", "\n"]) + "".join(p + rng.choice(spaces) for p in parts)
        docs.append(doc if rng.random() < 0.7 else doc.rstrip())
    with open(out, "wb") as f:
        f.write("\0".join(docs).encode("utf-8"))
    print(f"wrote {n} fuzz docs")


if __name__ == "__main__":
    main()
