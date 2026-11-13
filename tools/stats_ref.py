"""Stats steps parity helpers: synthetic URLs/metadata, case-heavy fuzz docs, and everything one `run()` produces.

`run_tasks` runs a step class over docs split into tasks (rank = task index), then datatrove's `StatsMerger`, and
returns each task's yielded docs (id, text, metadata items in order), step stats, raised exception, and every file
written (per-task JSON and merged `metric.json`) as bytes.
"""

import os
import random
import tempfile
import unicodedata
from functools import lru_cache

from datatrove.data import Document
from datatrove.pipeline.stats import StatsMerger
from parity_ref import stats_of

SUFFIXES = ["com", "org", "co.uk", "de", "com.au", "jp", "co.jp", "kawasaki.jp", "ck", "blogspot.com", "github.io",
            "s3.amazonaws.com", "xn--p1ai", "рф", "fr", "io", "gov.uk", "info", "notatld"]
ODD_URLS = ["http://192.168.0.1/x", "http://[::1]:8080/", "http://localhost/a", "https://user:pw@Example.COM:443/p",
            "www.no-scheme.co.uk/path", "mailto:someone@example.org", "http:///nohost", "https://bücher.de/ä",
            "https://例え.テスト/", "HTTPS://WWW.UPPER.ORG", "ftp://files.example.com.", "", "https://a.b.c.d.e.blogspot.com"]


def make_urls(n: int, seed: int = 0) -> list[str]:
    """Few distinct hosts (so fqdn/suffix keys repeat and tie), multi-label and private suffixes, odd URLs."""
    rng = random.Random(seed)
    hosts = [
        ".".join([*rng.sample(["www", "blog", "m", "news", "a1"], rng.randint(0, 2)), f"site{i}", rng.choice(SUFFIXES)])
        for i in range(40)
    ]
    return [rng.choice(ODD_URLS) if rng.random() < 0.05 else f"https://{rng.choice(hosts)}/p{i}" for i in range(n)]


def make_metadata(n: int, seed: int = 0, url: bool = True, token_counts: bool = True) -> list[dict]:
    rng = random.Random(seed)
    out = []
    for i, u in enumerate(make_urls(n, seed)):
        meta = {"url": u} if url else {}
        if token_counts and rng.random() < 0.3:
            meta["token_count"] = rng.choice([0, 1, rng.randint(2, 5000)])
        if rng.random() < 0.05:
            meta["fqdn"] = rng.choice(["preset.example.com", ""])
        out.append(meta)
    return out


@lru_cache(maxsize=1)
def _case_pool() -> list[str]:
    chars = [chr(c) for c in range(0x110000) if not 0xD800 <= c <= 0xDFFF]
    picks = [str.isupper, str.islower, lambda c: c.istitle() and not c.isupper(), str.isdigit, str.isspace,
             lambda c: unicodedata.category(c) in ("Mn", "Nl", "So", "Lo", "Pd", "Po")]
    rng = random.Random(7)
    pools = [[c for c in chars if p(c)] for p in picks]
    return [c for pool in pools for c in rng.sample(pool, min(60, len(pool)))]


EDGE_TEXTS = ["", " ", "\n", "\n\n\n", "IS_ALPHA test IS_ALPHA.", "lone \ud800 surrogate", "...", "....…...", "ǅungla ǅ",
              "ΣΑΣ σας Σ", "Ⅻ ⅻ ²³ ① ٣", "A. B. C. D.", "- bullet\n* star\n• dot\n  - indented\n", "x\r\ny\x0bz\x1c "]


def case_fuzz(n: int, seed: int = 0) -> list[str]:
    """Words of cased, titlecase, digit, space and mark chars mixed with ASCII, for the per-char/word predicates."""
    rng = random.Random(seed)
    pool = _case_pool() + list("aAzZ09.,!?-*•'\"…") + [" ", "\n", "\n\n"]
    docs = list(EDGE_TEXTS)
    while len(docs) < n:
        words = ["".join(rng.choice(pool) for _ in range(rng.randint(1, 8))) for _ in range(rng.randint(1, 60))]
        docs.append("".join(w + rng.choice([" ", " ", "\n", ". ", "\n\n", "... "]) for w in words))
    return docs


def make_docs(texts, metadata) -> list[Document]:
    return [Document(text=t, id=str(i), metadata=dict(m)) for i, (t, m) in enumerate(zip(texts, metadata))]


def _files(root: str) -> dict[str, bytes]:
    out = {}
    for d, _, names in os.walk(root):
        for name in names:
            with open(os.path.join(d, name), "rb") as f:
                out[os.path.relpath(os.path.join(d, name), root)] = f.read()
    return out


def run_tasks(cls, kwargs, texts, metadata, tasks: int = 1) -> dict:
    """Everything `cls(**kwargs).run` produces over `tasks` contiguous splits, plus the merged stats files."""
    docs = make_docs(texts, metadata)
    bounds = [len(docs) * k // tasks for k in range(tasks + 1)]
    out = {"tasks": [], "error": None}
    with tempfile.TemporaryDirectory() as tmp:
        partial, merged = os.path.join(tmp, "partial"), os.path.join(tmp, "merged")
        for rank in range(tasks):
            step = cls(partial, **kwargs)
            ran = []
            try:
                for doc in step.run(iter(docs[bounds[rank]:bounds[rank + 1]]), rank=rank):
                    ran.append((doc.id, doc.text, repr(list(doc.metadata.items()))))
            except Exception as e:  # noqa: BLE001 - the raised type is part of what's compared
                out["error"] = type(e).__name__
                return out
            out["tasks"].append((ran, stats_of(step)))
        if os.path.isdir(partial):
            kw = {"top_k_config": kwargs["top_k_config"]} if "top_k_config" in kwargs else {}
            for _ in StatsMerger(partial, merged, **kw).run(None):
                pass
        out["files"] = {f"partial/{k}": v for k, v in _files(partial).items()}
        out["files"] |= {f"merged/{k}": v for k, v in _files(merged).items()}
    return out


def diff(ref: dict, got: dict) -> list[str]:
    """Human-readable differences between two `run_tasks` results (empty when identical)."""
    if ref["error"] or got["error"]:
        return [] if ref["error"] == got["error"] else [f"raised {ref['error']} vs {got['error']}"]
    out = []
    for rank, ((r_docs, r_stats), (g_docs, g_stats)) in enumerate(zip(ref["tasks"], got["tasks"])):
        bad = [a for a, b in zip(r_docs, g_docs) if a != b]
        if bad or len(r_docs) != len(g_docs):
            out.append(f"task {rank}: {len(bad)} docs differ, first: {bad[:1]!r:.600}")
        if r_stats != g_stats:
            out.append(f"task {rank}: step stats {r_stats} vs {g_stats}")
    if ref["files"].keys() != got["files"].keys():
        out.append(f"files: {sorted(ref['files'].keys() ^ got['files'].keys())[:10]}")
    for name in sorted(ref["files"].keys() & got["files"].keys()):
        if ref["files"][name] != got["files"][name]:
            out.append(f"{name}: {ref['files'][name][:300]!r} vs {got['files'][name][:300]!r}")
    return out
