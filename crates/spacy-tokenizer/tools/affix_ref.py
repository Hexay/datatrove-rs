"""Regex-level parity corpus + reference.

  affix_ref.py corpus <docs.bin> <out_strings.bin> [n_random]   unique ws-chunks + rule keys + fuzz
  affix_ref.py ref <strings.bin> <out.txt>                      spaCy find_prefix/suffix/infix + url_match
  affix_ref.py diff <strings.bin> <a.txt> <b.txt>               list differing strings
"""

import json
import os
import random
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def corpus(docs_path, out, n_random=200_000):
    docs = open(docs_path, "rb").read().decode("utf-8").split("\0")
    chunks = {c for d in docs for c in d.split()}
    rules = json.load(open(f"{ROOT}/data/en_rules.json", encoding="utf-8"))["rules"]
    chunks.update(k for k, _ in rules)
    alphabet = list("aZé9٣.,-–—~+*^:<>=/()[]{}'\"’%$€°Fk…!?@#&_") + ["km", "US$", "a.m", "'s", "http://", "www.", ".com"]
    rng = random.Random(0)
    for _ in range(int(n_random)):
        chunks.add("".join(rng.choice(alphabet) for _ in range(rng.randint(1, 8))))
    chunks.discard("")
    items = sorted(chunks)
    with open(out, "wb") as f:
        f.write("\0".join(items).encode("utf-8"))
    print(f"{len(items)} strings")


def ref(path, out):
    import spacy

    tok = spacy.blank("en").tokenizer
    strings = open(path, "rb").read().decode("utf-8").split("\0")
    with open(out, "w", encoding="utf-8", newline="\n") as f:
        for s in strings:
            infs = ",".join(f"{m.start()}-{m.end()}" for m in tok.find_infix(s))
            url = int(tok.url_match(s) is not None)
            f.write(f"{tok.find_prefix(s)} {tok.find_suffix(s)} {url} {infs}\n")


def diff(path, a, b):
    strings = open(path, "rb").read().decode("utf-8").split("\0")
    la = open(a, encoding="utf-8").read().split("\n")
    lb = open(b, encoding="utf-8").read().split("\n")
    bad = [(s, x, y) for s, x, y in zip(strings, la, lb) if x != y]
    print(f"{len(strings)} strings, {len(bad)} differ")
    for s, x, y in bad[:40]:
        print(f"  {s!r}: ref={x!r} rust={y!r}")


if __name__ == "__main__":
    cmd, *a = sys.argv[1:]
    {"corpus": corpus, "ref": ref, "diff": diff}[cmd](*a)
