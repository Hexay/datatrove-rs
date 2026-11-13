"""Sample FineWeb-2 text for per-language parity runs.

Usage: fetch_fineweb2.py <out_dir> <n_docs> <code>...

<code> is a spaCy code, or `<TokenizerClass>[:<tok_code>]` for datatrove's other tokenizers (`ThaiTokenizer`,
`IndicNLPTokenizer:hi`). For each, takes the FineWeb-2 subsets datatrove tokenizes with it (the code's own
language first, then proxied languages) and reads their `test` split (`train` if absent) row group by row group
until n_docs, writing <out_dir>/<name>.parquet (one `text` column) and <out_dir>/<name>.bin ('\0'-joined), where
<name> is <code> with `:` replaced by `-`.
"""

import csv
import os
import sys

import pyarrow as pa
import pyarrow.parquet as pq
from datatrove.utils._import_utils import ASSETS_PATH
from huggingface_hub import HfFileSystem

REPO = "datasets/HuggingFaceFW/fineweb-2/data"
MAX_SUBSETS = 6


def tokenizer_key(tok_type: str, tok_code: str) -> str:
    """The spaCy code for SpaCyTokenizer rows, else e.g. `IndicNLPTokenizer:hi` or `ThaiTokenizer`."""
    return tok_code if tok_type == "SpaCyTokenizer" else f"{tok_type}:{tok_code}".rstrip(":")


# The native language of tokenizers that take no language argument.
NATIVE = {"ThaiTokenizer": "th", "KiwiTokenizer": "ko"}


def subsets_by_code():
    out = {}
    with open(os.path.join(ASSETS_PATH, "tokenizer_assignment.csv")) as f:
        for r in csv.DictReader(f):
            if r["type"]:
                own = r["tok_code"] or NATIVE.get(r["type"])
                native = r["code_1"] == own and r["default_script"] == "TRUE"
                key = tokenizer_key(r["type"], r["tok_code"])
                out.setdefault(key, []).append((not native, f"{r['code_3']}_{r['script']}"))
    return {code: sorted(v) for code, v in out.items()}


def read_subset(fs, subset, n):
    """Up to n texts from the subset's first test file, topped up from its first train file."""
    texts = []
    for split in ("test", "train"):
        files = sorted(fs.glob(f"{REPO}/{subset}/{split}/*.parquet"))
        if not files or len(texts) >= n:
            continue
        with fs.open(files[0], "rb", block_size=8 << 20) as fh:
            pf = pq.ParquetFile(fh)
            for g in range(pf.num_row_groups):
                texts += [t for t in pf.read_row_group(g, columns=["text"]).column(0).to_pylist() if "\0" not in t]
                if len(texts) >= n:
                    break
    return texts[:n]


def sample(fs, subsets, n):
    """Proxied languages (an evenly spaced pick of at most MAX_SUBSETS) share half of n, or all of it when the code
    has no language of its own; the code's own language fills the rest."""
    proxies = [s for proxied, s in subsets if proxied]
    proxies = proxies[:: max(1, -(-len(proxies) // MAX_SUBSETS))]
    natives = [s for proxied, s in subsets if not proxied]
    texts, used = [], []

    def take(group, budget):
        start = len(texts)
        for i, s in enumerate(group):
            got = read_subset(fs, s, -(-(budget - (len(texts) - start)) // (len(group) - i)))
            used.extend([f"{s}:{len(got)}"] if got else [])
            texts.extend(got)

    take(proxies, n // 2 if natives else n)
    take(natives, n - len(texts))
    return texts, used


def main():
    out_dir, n, codes = sys.argv[1], int(sys.argv[2]), sys.argv[3:]
    os.makedirs(out_dir, exist_ok=True)
    fs, table = HfFileSystem(), subsets_by_code()
    for code in codes:
        texts, used = sample(fs, table[code], n)
        name = code.replace(":", "-")
        pq.write_table(pa.table({"text": texts}), f"{out_dir}/{name}.parquet")
        with open(f"{out_dir}/{name}.bin", "wb") as f:
            f.write("\0".join(texts).encode("utf-8"))
        print(f"{code}: {len(texts)} docs from {' '.join(used)}", flush=True)


if __name__ == "__main__":
    main()
