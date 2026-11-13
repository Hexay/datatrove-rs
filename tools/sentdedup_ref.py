"""Sentence dedup fingerprints: stages 1 -> 2 -> 3 over a corpus split into `RANKS` ranks, stage 3 again on
synthetic duplicate files, and `remove_dup_sentences` on random duplicate indices (some out of order, where
datatrove raises). Stage 2 is always datatrove's `SentenceFindDedups`, so equal signature files give equal input
to stage 3 on both sides."""

import os
import random
import tempfile
from concurrent.futures import ProcessPoolExecutor

import numpy as np
from datatrove.data import Document
from datatrove.pipeline.dedup import sentence_dedup as dts
from datatrove.utils.word_tokenizers import SpaCyTokenizer, load_word_tokenizer

from parity_ref import RecordingWriter, digest, settle_is_alpha_state, stats_of

RANKS = 4
_DUP_DTYPE = [("doc", "<u4"), ("sent", "<u2")]


def shards(texts):
    size = -(-len(texts) // RANKS)
    return [texts[i * size : (i + 1) * size] for i in range(RANKS)]


def _docs(texts):
    return (Document(text=t, id=str(i)) for i, t in enumerate(texts))


def _files(folder):
    out = {}
    for root, _, names in os.walk(folder):
        for n in names:
            with open(os.path.join(root, n), "rb") as fh:
                out[os.path.relpath(os.path.join(root, n), folder)] = digest(fh.read())
    return out


def _raised(fn):
    """None, or the type name of what `fn()` raised (a raise ends `run()` in datatrove too)."""
    try:
        fn()
    except Exception as e:  # noqa: BLE001 — the exception type is part of the behavior under test
        return type(e).__name__


def _stage1(args):
    cls, case, language, folder, rank, texts = args
    settle_is_alpha_state(language)
    step = cls(folder, finder_workers=case["finder_workers"], config=case["config"], language=language)
    raised = _raised(lambda: step.run(_docs(texts), rank=rank))
    return stats_of(step), raised


def _stage3(args):
    cls, case, language, folder, rank, texts = args
    settle_is_alpha_state(language)
    step = cls(folder, config=case["config"], exclusion_writer=RecordingWriter(), language=language)
    out = []
    raised = _raised(lambda: out.extend((d.id, digest(d.text)) for d in step.run(_docs(texts), rank=rank)))
    return out, stats_of(step), step.exclusion_writer.log, raised


def span_safe(texts, language):
    """The texts datatrove's `span_tokenize` doesn't raise on. Only Thai and Indic (`simple_span_tokenize`) can."""
    tok = load_word_tokenizer(language)
    if isinstance(tok, SpaCyTokenizer):
        return texts
    return [t for t in texts if _raised(lambda: tok.span_tokenize(t)) is None]


def random_dups(rng):
    """Sorted sentence indices for one doc; 10% get a repeated index, which datatrove raises on."""
    hi = rng.choice([3, 8, 40, 300])
    du = sorted(rng.sample(range(hi), min(hi, rng.choice([0, 1, 1, 2, 3, 6]))))
    if du and rng.random() < 0.1:
        du.insert(rng.randrange(len(du)), rng.choice(du))
    return np.array(du, dtype="<u2")


def _remove(args):
    cls, case, language, rank, texts = args
    settle_is_alpha_state(language)
    rng = random.Random(rank)
    with tempfile.TemporaryDirectory() as tmp:
        step = cls(tmp, config=case["config"], language=language)
        out = []
        for t in texts:
            try:
                a, b = step.remove_dup_sentences(Document(text=t, id="0"), random_dups(rng))
                out.append((digest(a), digest(b)))
            except Exception as e:  # noqa: BLE001 — the exception type is part of the behavior under test
                out.append(("raise", type(e).__name__))
        return out, stats_of(step)


def write_synthetic_dups(folder, parts):
    """Random (doc, sent) duplicates for half the docs of each rank, spread over two bucket folders."""
    rng = random.Random(1)
    for rank, texts in enumerate(parts):
        pairs = [(d, s) for d in range(len(texts)) if rng.random() < 0.5 for s in random_dups(rng)]
        pairs = sorted(set(pairs))
        rng.shuffle(pairs)
        for b in range(2):
            os.makedirs(os.path.join(folder, f"{b:04d}"), exist_ok=True)
            np.array(pairs[b::2], dtype=_DUP_DTYPE).tofile(os.path.join(folder, f"{b:04d}", f"{rank:05d}.c4_dup"))


def _map(fn, jobs, workers):
    if workers <= 1:
        return [fn(j) for j in jobs]
    with ProcessPoolExecutor(min(workers, len(jobs))) as pool:
        return list(pool.map(fn, jobs))


def sentdedup_results(sig_cls, filter_cls, case, texts, language, workers=1):
    parts = shards(texts)
    with tempfile.TemporaryDirectory() as tmp:
        sigs, dups, synth = (os.path.join(tmp, d) for d in ("sigs", "dups", "synth"))
        sig_stats = _map(_stage1, [(sig_cls, case, language, sigs, r, p) for r, p in enumerate(parts)], workers)
        fw = case["finder_workers"]
        for r in range(fw):
            dts.SentenceFindDedups(sigs, dups, config=case["config"]).run(rank=r, world_size=fw)
        real = _map(_stage3, [(filter_cls, case, language, dups, r, p) for r, p in enumerate(parts)], workers)
        write_synthetic_dups(synth, parts)
        synthetic = _map(_stage3, [(filter_cls, case, language, synth, r, p) for r, p in enumerate(parts)], workers)
        removed = _map(_remove, [(filter_cls, case, language, r, p) for r, p in enumerate(parts)], workers)
        return {
            "sig_files": _files(sigs),
            "sig_stats": sig_stats,
            "dup_files": _files(dups),
            "real": real,
            "synthetic": synthetic,
            "remove": removed,
        }


def signature_diffs(sig_a, sig_b, case, texts, language, limit=20):
    """Indices of docs whose `get_hashes` differ between two stage 1 classes (for reports)."""
    with tempfile.TemporaryDirectory() as tmp:
        a, b = (cls(tmp, config=case["config"], language=language) for cls in (sig_a, sig_b))
        out = []
        for i, t in enumerate(texts):
            d = Document(text=t, id="0")
            if a.get_hashes(d, 0) != b.get_hashes(d, 0):
                out.append(i)
                if len(out) >= limit:
                    break
        return out
