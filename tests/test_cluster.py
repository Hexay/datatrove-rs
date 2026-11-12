"""datatrove_rs MinhashDedupCluster against datatrove's: output files byte for byte, stats dict for dict."""

import random
import struct
from pathlib import Path

import pytest

pytest.importorskip("tokenizers")

from datatrove.pipeline.dedup import minhash as dt  # noqa: E402

from datatrove_rs.minhash import MinhashDedupCluster  # noqa: E402

SENTINEL = (1 << 32) - 1
BUCKETS = 3


def _write_dups(folder: Path, seed: int, files: int, docs: int, n_pairs: int, index_every: int = 0) -> None:
    rng = random.Random(seed)
    folder.mkdir(parents=True)
    for b in range(BUCKETS):
        buf = bytearray()
        for i in range(n_pairs):
            a = (SENTINEL, SENTINEL) if index_every and i % index_every == 0 else (rng.randrange(files), rng.randrange(docs))
            c = (rng.randrange(files), rng.randrange(docs))
            if a != c:
                buf += struct.pack("<4I", *a, *c)
        # chains, so union by size and the tie rule both matter
        for _ in range(n_pairs // 10):
            chain = [(rng.randrange(files), rng.randrange(docs)) for _ in range(rng.randrange(2, 6))]
            for x, y in zip(chain, chain[1:]):
                if x != y:
                    buf += struct.pack("<4I", *x, *y)
        (folder / f"{b:05d}_00.dups").write_bytes(bytes(buf))


def _run(cls, src: Path, out: Path, **kw):
    step = cls(input_folder=str(src), output_folder=str(out), config=dt.MinhashConfig(num_buckets=BUCKETS), **kw)
    step.run(None, 0, 1)
    files = {p.name: p.read_bytes() for p in sorted(out.iterdir())} if out.exists() else {}
    return files, step.stats.stats.to_dict()


FLAGS = [
    {},
    {"save_cluster_id": True},
    {"save_cluster_size": True},
    {"save_cluster_id": True, "save_cluster_size": True, "ignore_index_matches": True},
    {"save_cluster_id": True, "save_cluster_size": True},
]


@pytest.mark.parametrize("flags", FLAGS)
@pytest.mark.parametrize("shape", [(1, 3, 40, 300, 0), (2, 12, 2000, 3000, 7), (3, 1, 60, 500, 3)])
def test_cluster_matches_datatrove(tmp_path, flags, shape):
    seed, files, docs, n_pairs, index_every = shape
    _write_dups(tmp_path / "dups", seed, files, docs, n_pairs, index_every)
    expected = _run(dt.MinhashDedupCluster, tmp_path / "dups", tmp_path / "py", **flags)
    got = _run(MinhashDedupCluster, tmp_path / "dups", tmp_path / "rs", **flags)
    assert expected[0], "no output files"
    assert list(got[0]) == list(expected[0])
    assert got[0] == expected[0]
    assert list(got[1].items()) == list(expected[1].items())


def test_empty_input(tmp_path):
    for b in range(BUCKETS):
        (tmp_path / "dups").mkdir(exist_ok=True)
        (tmp_path / "dups" / f"{b:05d}_00.dups").write_bytes(b"")
    assert _run(MinhashDedupCluster, tmp_path / "dups", tmp_path / "rs") == ({}, {})


def test_truncated_file_raises_struct_error(tmp_path):
    _write_dups(tmp_path / "dups", 4, 2, 50, 20)
    with open(tmp_path / "dups" / "00001_00.dups", "ab") as f:
        f.write(b"\0" * 5)
    with pytest.raises(struct.error):
        _run(MinhashDedupCluster, tmp_path / "dups", tmp_path / "rs")
