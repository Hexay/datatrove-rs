"""Reference side for the Chinese/Japanese tokenizers: datatrove's own `SpaCyTokenizer("zh"|"ja")`.

  cjk_ref.py dump <docs.bin> <out.bin> <zh|ja>   per doc: u32 ntok (0xFFFFFFFF if it raises), (u32 idx, u32 len) chars
  cjk_ref.py hmm <out.json>                      jieba's finalseg tables, floats as IEEE-754 bits (for cjkdump)
  cjk_ref.py paths                               jieba dict.txt, then SudachiPy settings, resource dir, system dict
"""

import json
import struct
import sys
import warnings

warnings.filterwarnings("ignore")


def dump(path, out, lang):
    from datatrove.utils.word_tokenizers import SpaCyTokenizer, chunk_text_on_bytes

    docs = open(path, "rb").read().decode("utf-8").split("\0")
    tok = SpaCyTokenizer(lang)
    raised = 0
    with open(out, "wb") as f:
        for text in docs:
            try:
                parts = tok._do_tokenize(text)
            except Exception:
                raised += 1
                f.write(struct.pack("<I", 0xFFFFFFFF))
                continue
            chunks = chunk_text_on_bytes(text, 40000) if lang == "ja" else [text]
            spans, base = [], 0
            for chunk, doc in zip(chunks, parts, strict=True):
                assert doc.text == chunk
                spans += [(base + t.idx, len(t.text)) for t in doc]
                base += len(chunk)
            f.write(struct.pack(f"<I{2 * len(spans)}I", len(spans), *(x for s in spans for x in s)))
    print(f"dumped {len(docs)} docs, {raised} raised")


def bits(x):
    return struct.unpack("<Q", struct.pack("<d", x))[0]


def hmm(out):
    from jieba import finalseg

    table = {
        "start": {k: bits(v) for k, v in finalseg.start_P.items()},
        "trans": {k: {k2: bits(v) for k2, v in row.items()} for k, row in finalseg.trans_P.items()},
        "emit": {k: {k2: bits(v) for k2, v in row.items()} for k, row in finalseg.emit_P.items()},
        "force_split": sorted(finalseg.Force_Split_Words),
    }
    with open(out, "w", encoding="utf-8") as f:
        json.dump(table, f, ensure_ascii=False)


def paths():
    import jieba
    import sudachipy

    print(jieba.get_dict_file().name)
    print(sudachipy._DEFAULT_SETTINGFILE)
    print(sudachipy._DEFAULT_RESOURCEDIR)
    print(sudachipy._find_dict_path("core"))


if __name__ == "__main__":
    cmd, *a = sys.argv[1:]
    {"dump": dump, "hmm": hmm, "paths": paths}[cmd](*a)
