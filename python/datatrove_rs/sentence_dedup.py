"""Sentence dedup stages 1 and 3 with sentence splitting, `simplify_text`, hashing, sentence removal and the keep
decision's word/sentence counts in Rust (`crates/datatrove-core/src/sentdedup`). Stage 2 (`SentenceFindDedups`)
is datatrove's: it only merges sorted files. Files, docs and stats are datatrove's; see docs/parity.md."""

import contextlib
import dataclasses
import hashlib
import inspect
from concurrent.futures import ThreadPoolExecutor
from functools import lru_cache

import numpy as np
from datatrove.pipeline.dedup import sentence_dedup as dt
from datatrove.utils.logging import logger
from datatrove.utils.typeshelper import ExtensionHelperSD, StatHints
from tqdm import tqdm

from datatrove_rs import _core
from datatrove_rs._base import _NOT_APPLICABLE, FALLBACK, RustBacked, norm_fields
# Fingerprint of the datatrove code these classes reproduce (`_reproduced`): datatrove 0.10.1 up to main of 2026-11.
VERIFIED_SOURCE = "278a91e614dc465eaada99c8ece9b73a"


def _reproduced():
    return (
        dt.SentDedupConfig,
        dt.SentenceDedupSignature.get_hashes,
        dt.SentenceDedupSignature.save_hashes,
        dt.SentenceDedupSignature.run,
        dt.SentenceDedupFilter.remove_dup_sentences,
        dt.SentenceDedupFilter.run,
    )


@lru_cache(maxsize=1)
def datatrove_verified() -> bool:
    """True when datatrove's sentence dedup code is the one the Rust path was checked against; else it runs."""
    try:
        src = "".join(inspect.getsource(obj) for obj in _reproduced())
    except (OSError, TypeError):
        return False
    return hashlib.blake2b(src.encode(), digest_size=16).hexdigest() == VERIFIED_SOURCE


def _plain_int(x, lo: int) -> bool:
    return type(x) is int and lo <= x < 1 << 62


class SentenceDedupSignature(RustBacked, dt.SentenceDedupSignature):
    def _build_rust(self, language):
        c, h = self.config, self.config.hash_config
        if not datatrove_verified() or not _plain_int(c.n_sentences, 0):
            return _NOT_APPLICABLE
        norm = norm_fields(c.norm_config)
        rs = _core.SentDedupSigner(c.n_sentences, bool(c.split_sentences), norm, h.hash_fc, h.precision, language)
        return rs if rs.supported else _NOT_APPLICABLE

    def get_hashes(self, doc, doc_idx):
        rs = self.rust_for(doc.text)
        if rs:
            with contextlib.suppress(*FALLBACK):
                pairs = rs.hashes(doc.text)
                if pairs is not None:
                    return [(h, doc_idx, s) for h, s in pairs]
        return super().get_hashes(doc, doc_idx)

    def run(self, data, rank: int = 0, world_size: int = 1):
        rs = self.rust()
        cls = type(self)
        if not rs or cls.get_hashes is not SentenceDedupSignature.get_hashes or cls.save_hashes is not _DT_SAVE:
            return super().run(data, rank, world_size)
        rs.clear()
        for doc_idx, doc in enumerate(data):
            with self.stats.time_stats:
                self.stat_update(StatHints.total)
                self._add_hashes(rs, doc, doc_idx)
        records = rs.records()
        if records is None:
            self.save_hashes(rank, rs.signatures())
        else:
            self._save_sorted(rank, np.frombuffer(records, dtype=self._dtype()))
        rs.clear()

    def _add_hashes(self, rs, doc, doc_idx):
        if self.rust_for(doc.text):
            with contextlib.suppress(*FALLBACK):
                if rs.add(doc.text, doc_idx):
                    return
        rs.extend(dt.SentenceDedupSignature.get_hashes(self, doc, doc_idx))

    def _dtype(self):
        return np.dtype([("hash", self.config.hash_config.np_descr), ("doc", "<u4"), ("sent", "<u2")])

    def _save_sorted(self, rank, signatures):
        """`save_hashes` after its `np.array(...)` and `sort`, which Rust already did (`sorted_records`)."""
        hashes_per_worker = self.config.hash_config.max // self.finder_workers
        left_idx = 0
        for hash_i in range(self.finder_workers):
            with self.output_folder.open(
                f"{hash_i:04d}/{rank:05d}{ExtensionHelperSD.stage_1_signature}", mode="wb"
            ) as f:
                right_hash = (
                    (hash_i + 1) * hashes_per_worker
                    if hash_i != self.finder_workers - 1
                    else self.config.hash_config.max
                )
                right_idx = left_idx + signatures["hash"][left_idx:].searchsorted(right_hash, side="right")
                if right_idx > left_idx:
                    f.write(signatures[left_idx:right_idx].tobytes())
                left_idx = right_idx
                if right_idx >= len(signatures):
                    break


_DT_SAVE = dt.SentenceDedupSignature.save_hashes


class SentenceDedupFilter(RustBacked, dt.SentenceDedupFilter):
    def _build_rust(self, language):
        c = self.config
        if not datatrove_verified() or not _plain_int(c.n_sentences, -(1 << 62)):
            return _NOT_APPLICABLE
        if type(c.min_words_to_remove_span) not in (int, float):
            return _NOT_APPLICABLE
        split = bool(c.split_sentences)
        rs = _core.SentDedupFilter(c.n_sentences, split, float(c.min_words_to_remove_span), language)
        return rs if rs.supported else _NOT_APPLICABLE

    def remove_dup_sentences(self, doc, du_lines):
        rs = self.rust_for(doc.text)
        out = None
        if rs and isinstance(du_lines, np.ndarray) and du_lines.dtype == np.dtype("<u2"):
            with contextlib.suppress(*FALLBACK):
                out = rs.remove(doc.text, np.ascontiguousarray(du_lines).tobytes())
        if out is None:
            return super().remove_dup_sentences(doc, du_lines)
        text, original_formatted, n_sentences, n_kept = out
        if n_kept < n_sentences:
            self.stat_update("removed_sentences", value=n_sentences - n_kept)
        self.stat_update("original_sentences", value=n_sentences)
        return text, original_formatted

    def _enough_words(self, text) -> bool:
        m = self.config.min_doc_words
        if m <= 0:
            return True
        if rs := self.rust_for(text):
            with contextlib.suppress(*FALLBACK):
                return rs.word_count(text) >= m
        return len(self.tokenizer.word_tokenize(text)) >= m

    def _enough_sentences(self, text) -> bool:
        m = self.config.min_num_sentences
        if m <= 0:
            return True
        if rs := self.rust_for(text):
            with contextlib.suppress(*FALLBACK):
                n, exact = rs.sentence_count(text)
                if exact or n >= m:
                    return n >= m
        return len(dt.split_into_sentences(text, self.language)) >= m

    def _load_duplicates(self, rank):
        """datatrove's `run()` preamble: the rank's (doc, sent) duplicates, sorted, and each doc's first row."""
        folders = self.data_folder.list_files(include_directories=True, recursive=False)
        files = [
            f
            for f in [f"{folder}/{rank:05d}{ExtensionHelperSD.stage_2_duplicates}" for folder in folders]
            if self.data_folder.exists(f)
        ]
        logger.info(f"Loading duplicate indexes from {len(files)} results files.")
        all_dups = np.array([], dtype=[("doc", "<u4"), ("sent", "<u2")])
        if files:
            with ThreadPoolExecutor() as pool:
                all_dups = np.concatenate(
                    list(tqdm(pool.map(self.read_duplicates, self.data_folder.open_files(files)), total=len(files))),
                    axis=0,
                )
            all_dups.sort()
        _, doc_starts = np.unique(all_dups["doc"], return_index=True)
        logger.info("Loaded duplicate indexes.")
        return all_dups, doc_starts

    def run(self, data, rank: int = 0, world_size: int = 1):
        """datatrove's `run()` with the keep decision's counts from Rust (`_enough_words`, `_enough_sentences`)."""
        if not self.rust():
            yield from super().run(data, rank, world_size)
            return
        all_dups, doc_starts = self._load_duplicates(rank)
        dups_doc_i = 0
        with self.exclusion_writer if self.exclusion_writer else contextlib.nullcontext() as writer:
            for doc_idx, doc in enumerate(data):
                self.stat_update(StatHints.total)
                with self.stats.time_stats:
                    if dups_doc_i >= len(doc_starts) or all_dups["doc"][doc_starts[dups_doc_i]] > doc_idx:
                        filtered_text, original_formatted = doc.text, None
                    else:
                        sents_span_l, sents_span_r = (
                            doc_starts[dups_doc_i],
                            doc_starts[dups_doc_i + 1] if dups_doc_i + 1 < len(doc_starts) else None,
                        )
                        filtered_text, original_formatted = self.remove_dup_sentences(
                            doc, all_dups["sent"][sents_span_l:sents_span_r]
                        )
                        dups_doc_i += 1

                if (
                    filtered_text == doc.text
                    or (self._enough_words(filtered_text) and self._enough_sentences(filtered_text))
                ) and filtered_text:
                    self.update_doc_stats(doc)
                    if not filtered_text == doc.text and writer:
                        writer.write(dataclasses.replace(doc, text=original_formatted), rank=rank)
                    doc.text = filtered_text
                    yield doc
                elif writer:
                    doc.text = original_formatted
                    writer.write(doc, rank=rank)
