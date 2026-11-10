"""fuzz_docs.py <out.bin> [n_docs] — adversarial docs: rule keys glued to punctuation, odd whitespace."""

import json
import os
import random
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
rules = [k for k, _ in json.load(open(f"{ROOT}/data/en_rules.json", encoding="utf-8"))["rules"]]
punct = list("()[]{}\"'`’“”.,;:!?…-–—~+*^/<>=%$€£#&_@°") + ["..", "...", "--", "'s", "’s", "US$", "km", "a.m."]
words = ["the", "U.S.", "e-mail", "10am", "5km", "3.5", "1,000", "a/b", "x=y", "hello", "Mr.", "C++", "http://ex.com/a?b=c", "www.test.org", "192.168.0.1", "8.8.8.8", "user@mail.com", "ÉCOLE", "straße", "٣٤", "ｆｕｌｌ"]
spaces = [" ", " ", " ", "  ", "\n", "\n\n", "\t", "\xa0", " \n ", "\u2028", "\u3000", "\x1c", "\r\n", "\x0b"]


def piece(rng):
    base = rng.choice(rules if rng.random() < 0.5 else words)
    pre = "".join(rng.choice(punct) for _ in range(rng.choice([0, 0, 1, 2])))
    suf = "".join(rng.choice(punct) for _ in range(rng.choice([0, 0, 1, 2, 3])))
    mid = rng.choice(["", "", "", "-", "/", ".", ","]) + rng.choice(words) if rng.random() < 0.1 else ""
    return pre + base + mid + suf


def main():
    out = sys.argv[1]
    n = int(sys.argv[2]) if len(sys.argv) > 2 else 5000
    rng = random.Random(1)
    docs = []
    for _ in range(n):
        parts = [piece(rng) for _ in range(rng.randint(1, 60))]
        doc = rng.choice(["", " ", "\n"]) + "".join(p + rng.choice(spaces) for p in parts)
        docs.append(doc if rng.random() < 0.7 else doc.rstrip())
    with open(out, "wb") as f:
        f.write("\0".join(docs).encode("utf-8"))
    print(f"wrote {n} fuzz docs")


if __name__ == "__main__":
    main()
