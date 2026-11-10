"""Result fingerprints of one pipeline step over a corpus, so the slow datatrove side can be computed in parallel
and cached across parity runs. Texts are compared by 16-byte digests to keep memory flat on large corpora."""

import hashlib
import os
import pickle
import tempfile
from concurrent.futures import ProcessPoolExecutor

from datatrove.data import Document
from datatrove.utils.word_tokenizers import load_word_tokenizer


def settle_is_alpha_state():
    """Both sides route "IS_ALPHA" docs to datatrove's shared spaCy tokenizer, whose behavior changes after its first
    "IS_ALPHA" token (see RustBacked.rust_for). Trigger that change up front so every process sees the same state."""
    load_word_tokenizer("eng").word_tokenize("IS_ALPHA")


def digest(s: str | bytes) -> bytes:
    return hashlib.blake2b(s.encode() if isinstance(s, str) else s, digest_size=16).digest()


def filter_outcome(step, text):
    doc = Document(text=text, id="0")
    try:
        r = step.filter(doc)
    except Exception as e:  # noqa: BLE001 — the exception type is part of the behavior under test
        return ("raise", type(e).__name__), None
    return (r if isinstance(r, tuple) else (bool(r), None)), (None if doc.text == text else digest(doc.text))


def make_step(cls, kwargs, **extra):
    """`cls(**kwargs, **extra)`; `batch_size` is set as an attribute since datatrove's filters don't take it."""
    kwargs = dict(kwargs)
    batch_size = kwargs.pop("batch_size", 1)
    step = cls(**kwargs, **extra)
    step.batch_size = batch_size
    return step


def _filter_chunk(args):
    cls, kwargs, texts = args
    settle_is_alpha_state()
    step = make_step(cls, kwargs)
    return [filter_outcome(step, t) for t in texts]


def _minhash_chunk(args):
    cls, config, texts = args
    settle_is_alpha_state()
    with tempfile.TemporaryDirectory() as tmp:
        step = cls(tmp, config=config)
        out = []
        for t in texts:
            s = step.get_shingles(t)
            out.append(digest(s.tobytes() + repr(s.shape).encode() + (repr(step.get_signature(s)).encode() if s.size else b"")))
        return out


def _map_chunks(fn, cls, arg, texts, workers):
    if workers <= 1:
        return fn((cls, arg, texts))
    size = max(1, len(texts) // (workers * 4))
    chunks = [(cls, arg, texts[i : i + size]) for i in range(0, len(texts), size)]
    with ProcessPoolExecutor(workers) as pool:
        return [r for part in pool.map(fn, chunks) for r in part]


_METRIC_FIELDS = ("total", "n", "mean", "min", "max", "_running_variance", "unit")


def stats_of(step):
    """Every stat field, repr'd so int vs float and -0.0 count; time stats only by count and unit (timings vary)."""
    metrics = [(k, repr([getattr(v, f) for f in _METRIC_FIELDS])) for k, v in step.stats.stats.items()]
    t = step.stats.time_stats
    return metrics + [("time_stats", t.n, t.unit)]


class RecordingWriter:
    """Stands in for an exclusion `DiskWriter`: records context entries/exits and every written doc."""

    def __init__(self):
        self.log = []

    def __enter__(self):
        self.log.append("enter")
        return self

    def __exit__(self, *exc):
        self.log.append("exit")

    def write(self, doc, rank):
        self.log.append((doc.id, digest(doc.text), rank, sorted(doc.metadata.items())))


def run_results(step, texts, rank=0):
    """`run()` output (ids, text digests), every stat and, if `step.exclusion_writer` records, what it wrote."""
    docs = (Document(text=t, id=str(i)) for i, t in enumerate(texts))
    ran = [(d.id, digest(d.text)) for d in step.run(docs, rank=rank)]
    return ran, stats_of(step), getattr(step.exclusion_writer, "log", None)


def filter_results(cls, kwargs, texts, workers=1, run_limit=None):
    """Per-doc (result, text digest if changed); then `run()` output, stats and excluded docs over the first
    `run_limit` docs that don't raise (`run()` accumulates stats in order, so it can't be split across workers)."""
    outcomes = _map_chunks(_filter_chunk, cls, kwargs, texts, workers)
    clean = [t for t, (r, _) in zip(texts, outcomes) if r[0] != "raise"][:run_limit]
    ran, stats, excluded = run_results(make_step(cls, kwargs, exclusion_writer=RecordingWriter()), clean, rank=3)
    return {"outcomes": outcomes, "run": ran, "stats": stats, "excluded": excluded}


def minhash_results(cls, config, texts, workers=1):
    """Per-doc shingle+signature digests; then the signature files `run()` writes, by path."""
    per_doc = _map_chunks(_minhash_chunk, cls, config, texts, workers)
    with tempfile.TemporaryDirectory() as tmp:
        cls(tmp, config=config).run((Document(text=t, id=str(i)) for i, t in enumerate(texts)), rank=0)
        files = {}
        for root, _, names in os.walk(tmp):
            for n in names:
                with open(os.path.join(root, n), "rb") as fh:
                    files[os.path.relpath(os.path.join(root, n), tmp)] = digest(fh.read())
    return {"outcomes": per_doc, "files": files}


def cached(cache_dir, key_parts, compute):
    if not cache_dir:
        return compute()
    key = hashlib.blake2b(repr(key_parts).encode(), digest_size=12).hexdigest()
    path = os.path.join(cache_dir, f"{key}.pkl")
    if os.path.exists(path):
        with open(path, "rb") as f:
            return pickle.load(f)
    result = compute()
    os.makedirs(cache_dir, exist_ok=True)
    with open(path, "wb") as f:
        pickle.dump(result, f)
    return result
