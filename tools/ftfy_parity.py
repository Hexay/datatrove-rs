"""FTFYFormatter parity: datatrove_rs vs datatrove (Python ftfy) on real, garbled and fuzzed docs, several configs.

Compares each doc's `format()` output, then `run()` over the whole set (texts, stats), and counts docs the Rust
port hands to ftfy (fallback). Corpora are parquet files with a `text` column or `docs.bin` (`\\0`-joined).

Usage: ftfy_parity.py [--n N] [--fuzz N] [--seed S] [--all-configs] corpus...
"""

import argparse
import random
import sys
import time
from pathlib import Path

from datatrove.data import Document
from datatrove.pipeline.formatters import FTFYFormatter as Py

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ftfy_fuzz import fuzz_docs, mojibake_docs  # noqa: E402

from datatrove_rs.formatters import FTFYFormatter as Rs  # noqa: E402

# (name, FTFYFormatter kwargs, TextFixerConfig overrides)
CONFIGS = [
    ("default", {}, {}),
    ("html-on", {"unescape_html": True}, {}),
    ("html-off", {"unescape_html": False, "remove_terminal_escapes": False}, {}),
    ("no-a0-lossy", {"restore_byte_a0": False, "replace_lossy_sequences": False}, {}),
    ("no-inconsistent-c1", {"decode_inconsistent_utf8": False, "fix_c1_controls": False}, {}),
    ("no-encoding", {"fix_encoding": False, "remove_control_chars": False}, {}),
    ("no-explain", {"restore_byte_a0": False, "fix_c1_controls": False}, {"explain": False}),
    ("short-segments", {}, {"max_decode_length": 37}),
    ("ligatures-width-quotes", {"fix_latin_ligatures": True, "fix_character_width": True, "uncurl_quotes": True}, {}),
    ("nfc", {"normalization": "NFC"}, {}),
]


def read_corpus(path: Path, n: int) -> list[str]:
    if path.suffix == ".parquet":
        import pyarrow.parquet as pq

        out = []
        for batch in pq.ParquetFile(path).iter_batches(batch_size=500, columns=["text"]):
            out += batch.column(0).to_pylist()
            if len(out) >= n:
                break
        return out[:n]
    with open(path, "rb") as f:
        return [d.decode("utf-8", "surrogatepass") for d in f.read().split(b"\0")[:n] if d]


def make(cls, kw, over):
    f = cls(**kw)
    f.config = f.config._replace(**over)
    return f


def compare(name: str, texts: list[str], configs) -> int:
    bad = 0
    for cname, kw, over in configs:
        py, rs = make(Py, kw, over), make(Rs, kw, over)
        mism = changed = fallback = 0
        t_py = t_rs = 0.0
        for t in texts:
            s = time.perf_counter()
            want = py.format(t)
            t_py += time.perf_counter() - s
            s = time.perf_counter()
            got = rs.format(t)
            t_rs += time.perf_counter() - s
            changed += want != t
            fallback += bool(rs._rust) and rs._rust(t) is None
            if got != want:
                mism += 1
                if mism <= 3:
                    print(f"  MISMATCH {cname}: {t[:300]!r}\n    py={want[:300]!r}\n    rs={got[:300]!r}")
        stats_ok = run_matches(py, rs, texts)
        path = "port" if rs._rust and rs._rust.__name__ == "fix" else "check" if rs._rust else "python"
        print(
            f"{name:28} {cname:24} [{path:6}] docs={len(texts):6} changed={changed:5} fallback={fallback:4} "
            f"mismatch={mism} run={'ok' if stats_ok else 'DIFF'} py={t_py / len(texts) * 1e6:7.0f}us "
            f"rs={t_rs / len(texts) * 1e6:6.1f}us",
            flush=True,
        )
        bad += mism + (not stats_ok)
    return bad


def run_matches(py, rs, texts) -> bool:
    def run(step):
        docs = [Document(text=t, id=str(i)) for i, t in enumerate(texts)]
        out = [d.text for d in step.run(iter(docs))]
        return out, {k: (v.total, v.n) for k, v in step.stats.stats.items()}, step.stats.time_stats.n

    return run(py) == run(rs)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("corpus", nargs="*", type=Path)
    ap.add_argument("--n", type=int, default=2000)
    ap.add_argument("--fuzz", type=int, default=0)
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--all-configs", action="store_true", help="every config on real docs too (default: fuzz only)")
    a = ap.parse_args()
    rng = random.Random(a.seed)
    bad = 0
    for path in a.corpus:
        texts = read_corpus(path, a.n)
        bad += compare(path.name, texts, CONFIGS if a.all_configs else CONFIGS[:1])
        garbled = mojibake_docs(texts[: max(len(texts) // 4, 1)], rng)
        bad += compare(path.name + "+mojibake", garbled, CONFIGS if a.all_configs else CONFIGS[:2])
    if a.fuzz:
        docs = fuzz_docs(a.fuzz, rng)
        docs += [d[: len(d) // 2] + "\ud83d" + d[len(d) // 2 :] for d in docs[:20]]  # lone surrogates: Python
        bad += compare("fuzz", docs, CONFIGS)
    print("TOTAL mismatches:", bad)
    return int(bad > 0)


if __name__ == "__main__":
    sys.exit(main())
