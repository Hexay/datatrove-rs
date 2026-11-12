"""spaCy reference side.

  spacy_ref.py dump  <docs.bin> <out.bin> [limit] [lang]   per doc: u32 ntok, then (u32 idx, u32 len) in chars
  spacy_ref.py export-docs <fw.parquet> <out.bin> <n> [skip]
"""

import struct
import sys

import spacy


def load_docs(path, limit=None):
    with open(path, "rb") as f:
        docs = f.read().decode("utf-8").split("\0")
    return docs[:limit] if limit else docs


def dump(path, out, limit=None, lang="en"):
    docs = load_docs(path, limit)
    tok = spacy.blank(lang).tokenizer
    bad_text = 0
    with open(out, "wb") as f:
        for text in docs:
            doc = tok(text)
            f.write(struct.pack("<I", len(doc)))
            for t in doc:
                bad_text += t.text != text[t.idx : t.idx + len(t.text)]
                f.write(struct.pack("<II", t.idx, len(t.text)))
    print(f"dumped {len(docs)} docs; tokens whose text != text[idx:idx+len]: {bad_text}")


def export_docs(parquet, out, n, skip=0):
    import pyarrow.parquet as pq

    texts = []
    for batch in pq.ParquetFile(parquet).iter_batches(batch_size=2000, columns=["text"]):
        texts.extend(batch.column(0).to_pylist())
        if len(texts) >= n + skip:
            break
    texts = [t for t in texts[skip : skip + n] if "\0" not in t]
    with open(out, "wb") as f:
        f.write("\0".join(texts).encode("utf-8"))
    print(f"wrote {len(texts)} docs to {out}")


if __name__ == "__main__":
    cmd, *a = sys.argv[1:]
    if cmd == "dump":
        dump(a[0], a[1], int(a[2]) or None if len(a) > 2 else None, *a[3:4])
    elif cmd == "export-docs":
        export_docs(a[0], a[1], int(a[2]), int(a[3]) if len(a) > 3 else 0)
