import numpy as np
from datatrove.pipeline.dedup import minhash as dt
from datatrove.utils.logging import logger
from datatrove.utils.stats import MetricStats
from tqdm import tqdm

from datatrove_rs import _core
from datatrove_rs._base import RustBacked

_READ_CHUNK = 1 << 24


class MinhashDedupSignature(RustBacked, dt.MinhashDedupSignature):
    """Stage 1 only; the signature files it writes are consumed unchanged by datatrove's stages 2-4."""

    def _build_rust(self):
        a, b = self.parameters
        n = self.config.norm_config
        h = self.config.hash_config
        norm = (
            n.lowercase,
            n.norm_whitespace,
            n.remove_punctuation,
            n.norm_unicode_diacritics,
            n.norm_numbers,
            n.norm_weekdays,
            n.norm_monthnames,
        )
        return _core.MinHasher(
            self.config.n_grams, norm, h.hash_fc, h.precision, a.astype("<u8").tobytes(), b.astype("<u8").tobytes()
        )

    def get_shingles(self, text: str) -> np.ndarray:
        rs = self.rust_for(text)
        if not rs:
            return super().get_shingles(text)
        return np.frombuffer(rs.shingles(text), dtype="<u8").reshape((-1, 1))

    def get_signature(self, shingles: np.ndarray) -> list[list[int]]:
        rs = self.rust()
        if not rs:
            return super().get_signature(shingles)
        return rs.signature(np.ascontiguousarray(shingles, dtype="<u8").tobytes(), self.config.num_buckets)


class MinhashDedupCluster(dt.MinhashDedupCluster):
    """Stage 3 with the union-find and output pass in Rust (`crates/datatrove-core/src/cluster`); same output files
    and stats as datatrove. Python still lists, reads and writes the files, so any fsspec folder works."""

    def run(self, data=None, _: int = 0, world_size: int = 1):
        dup_files = self.input_folder.list_files(glob_pattern="*.dups")
        assert (len(dup_files) % self.config.num_buckets) == 0, (
            "Number of .dups files should be divisible by number of buckets"
        )
        assert world_size == 1, "World size must be 1 for clustering"
        if dup_files and self.lines_to_buffer != -1 and self.lines_to_buffer < 1:
            raise ValueError("lines_to_buffer must be >= 1 or -1 (for unlimited)")
        rs = _core.MinhashCluster(self.ignore_index_matches)
        with self.track_time():
            logger.info("Loading dup files...")
            for dup_file in tqdm(dup_files, desc="Reading dup files"):
                with self.input_folder.open(dup_file, "rb") as dupf:
                    while chunk := dupf.read(_READ_CHUNK):
                        rs.feed(chunk)
                rs.end_file()
            logger.info("Finished reading dup files.")
            rs.finish(self.save_cluster_id, self.save_cluster_size)
            with self.output_folder.get_output_file_manager(mode="wb") as output_mg:
                while (out := rs.next_file()) is not None:
                    file, *parts = out
                    for ext, part in zip(("remove", "clusters", "sizes"), parts):
                        if part:
                            output_mg.write(f"{file:06d}.{ext}", part)
            self._add_stats(rs.metrics())

    def _add_stats(self, metrics):
        """Install the Rust-accumulated stats; equal to datatrove's per-node `stat_update` calls on a fresh step."""
        for label, unit, total, n, mean, mn, mx, var in metrics:
            m = MetricStats(total=total, n=n, mean=mean, min=mn, max=mx, _running_variance=var, unit=unit or "doc")
            prev = self.stats.stats.get(label)
            self.stats[label] = m if prev is None or prev.n == 0 else prev + m
