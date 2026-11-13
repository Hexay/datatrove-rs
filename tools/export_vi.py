"""Export pyvi's word-segmentation CRF and n-gram lists for the Rust Vietnamese tokenizer (`wordtok/vi`).

The weights are read from the crfsuite model file itself (`pycrfsuite`'s text dump rounds them) and stored as
IEEE-754 bit patterns so Rust sums exactly the same doubles in the same order as crfsuite's tagger.

Usage: export_vi.py [out.json]   (default crates/datatrove-core/data/wordtok/vi.json)
"""

import json
import os
import re
import struct
import sys
import warnings

sys.path.insert(0, os.path.dirname(__file__))
from export_pytext import ranges  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def cqdb_strings(buf, off):
    """id -> string of the CQDB chunk at `off` (crfsuite's constant quark database)."""
    chunk, _size, _flag, _order, bwd_size, bwd_offset = struct.unpack_from("<4sIIIII", buf, off)
    assert chunk == b"CQDB"
    out = []
    for i in range(bwd_size):
        (rec,) = struct.unpack_from("<I", buf, off + bwd_offset + 4 * i)
        sid, ksize = struct.unpack_from("<II", buf, off + rec)
        assert sid == i
        out.append(buf[off + rec + 8 : off + rec + 8 + ksize - 1].decode("utf-8"))
    return out


def read_model(path):
    buf = open(path, "rb").read()
    magic, _size, typ, _ver, _nf, n_labels, n_attrs, off_feat, off_labels, off_attrs = struct.unpack_from(
        "<4sI4sIIIIIII", buf, 0
    )
    assert magic == b"lCRF" and typ == b"FOMC"
    labels = cqdb_strings(buf, off_labels)
    attrs = cqdb_strings(buf, off_attrs)
    assert len(labels) == n_labels and len(attrs) == n_attrs
    chunk, _size, n_feat = struct.unpack_from("<4sII", buf, off_feat)
    assert chunk == b"FEAT"
    state = {}
    trans = [[0.0] * n_labels for _ in range(n_labels)]
    for k in range(n_feat):
        ftype, src, dst, w = struct.unpack_from("<IIId", buf, off_feat + 12 + 20 * k)
        if ftype == 0:
            state.setdefault(attrs[src], [0.0] * n_labels)[dst] = w
        else:
            assert ftype == 1
            trans[src][dst] = w
    return labels, state, trans


def bits(x: float) -> int:
    return struct.unpack("<Q", struct.pack("<d", x))[0]


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "crates/datatrove-core/data/wordtok/vi.json")
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", SyntaxWarning)
        from pyvi import ViTokenizer as V
    vt = V.ViTokenizer
    labels, state, trans = read_model(vt.model.modelfile.name)
    assert labels == ["B_W", "I_W"], labels
    # spaCy's syllabelizer splits on re `\s`; get_words_and_spaces re-checks with str.split(), so they must agree
    assert ranges(lambda c: re.fullmatch(r"\s", c) is not None) == ranges(str.isspace)
    data = {
        # Py_UNICODE_IS{LOWER,UPPER,TITLE,DIGIT}, from which str.isupper/istitle/isdigit are built
        "lower": ranges(str.islower),
        "upper": ranges(str.isupper),
        "title": ranges(lambda c: c.istitle() and not c.isupper()),
        "digit": ranges(str.isdigit),
        "trans": [[bits(w) for w in row] for row in trans],
        "state": {a: [bits(w) for w in ws] for a, ws in sorted(state.items())},
        "bi_grams": sorted(vt.bi_grams),
        "tri_grams": sorted(vt.tri_grams),
    }
    with open(out, "w", encoding="utf-8") as f:
        json.dump(data, f, ensure_ascii=False, separators=(",", ":"))
    print(f"wrote {out}: {len(state)} attributes, {len(vt.bi_grams)} bi-grams, {len(vt.tri_grams)} tri-grams")


if __name__ == "__main__":
    main()
