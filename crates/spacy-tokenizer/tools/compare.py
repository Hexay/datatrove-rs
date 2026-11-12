"""compare.py <docs.bin> <spacy.bin> <rust.bin> [max_examples] — per-doc exact match on (idx, len)."""

import struct
import sys
from collections import Counter


def read(path):
    data = open(path, "rb").read()
    out, p = [], 0
    while p < len(data):
        (n,) = struct.unpack_from("<I", data, p)
        p += 4
        out.append(list(struct.iter_unpack("<II", data[p : p + 8 * n])))
        p += 8 * n
    return out


def main():
    docs = open(sys.argv[1], "rb").read().decode("utf-8").split("\0")
    ref, rs = read(sys.argv[2]), read(sys.argv[3])
    max_ex = int(sys.argv[4]) if len(sys.argv) > 4 else 20
    n = min(len(ref), len(rs))
    bad = [i for i in range(n) if ref[i] != rs[i]]
    ntok = sum(len(r) for r in ref[:n])
    tok_bad = sum(len(set(ref[i]) ^ set(rs[i])) for i in bad)
    print(f"docs {n}: exact {n - len(bad)} ({100 * (n - len(bad)) / n:.3f}%), mismatched {len(bad)}")
    print(f"tokens {ntok}: symmetric-difference spans {tok_bad}")
    kinds = Counter()
    for i in bad:
        a, b = set(ref[i]), set(rs[i])
        text = docs[i]
        only_ref = sorted(a - b)
        only_rs = sorted(b - a)
        lo = min(x[0] for x in only_ref + only_rs)
        hi = max(x[0] + x[1] for x in only_ref + only_rs)
        ctx = text[max(0, lo - 15) : hi + 15]
        key = text[lo:hi]
        kinds[key] += 1
        if kinds[key] == 1 and sum(1 for _ in kinds) <= max_ex:
            print(f"--- doc {i} ctx={ctx!r}")
            print("   spacy:", [text[s : s + ln] for s, ln in only_ref][:12])
            print("   rust: ", [text[s : s + ln] for s, ln in only_rs][:12])
    print("first-diff substrings:", kinds.most_common(30))
    sys.exit(1 if bad or len(ref) != len(rs) else 0)


if __name__ == "__main__":
    main()
