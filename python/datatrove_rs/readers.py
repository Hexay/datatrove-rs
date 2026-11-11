"""datatrove's WarcReader with gzip decompression, WARC framing and record selection in Rust.

Rust yields only the records `process_record` would keep. A record datatrove might treat specially (no payload type,
non-UTF-8 payload, HTTP content/transfer encoding, missing URL or date) comes back as raw bytes and goes through
datatrove's own `process_record`; anything that could make warcio or Python's gzip behave differently (malformed
framing, gzip errors) hands the rest of the file to datatrove's loop. See docs/parity.md "WarcReader".
"""

import io

from datatrove.pipeline.readers import warc as dt

from datatrove_rs import _core


class WarcReader(dt.WarcReader):
    def read_file(self, filepath: str):
        if not self._is_gzip(filepath):
            yield from super().read_file(filepath)
            return
        with self.data_folder.open(filepath, "rb", compression=None) as f:
            for item in _core.WarcRecords(f):
                if type(item) is int:
                    yield from self._read_with_warcio(filepath, start=item)
                    return
                with self.track_time():
                    if len(item) == 2:
                        data = _process_raw(item[1])
                        if not data:
                            continue
                    else:
                        data = {"text": item[1], "id": item[2], "url": item[3], "date": item[4]}
                    document = self.get_document_from_dict(data, filepath, item[0])
                    if not document:
                        continue
                yield document

    def _is_gzip(self, filepath: str) -> bool:
        """Whether datatrove would read `filepath` through fsspec's gzip, the only stream the Rust loop reproduces."""
        from fsspec.utils import infer_compression

        return self.compression == "gzip" or (self.compression == "infer" and infer_compression(filepath) == "gzip")

    def _read_with_warcio(self, filepath: str, start: int):
        """datatrove's `read_file` from record `start` on."""
        from warcio.archiveiterator import ArchiveIterator

        with self.data_folder.open(filepath, "rb", compression=self.compression) as f:
            for ri, record in enumerate(ArchiveIterator(f)):
                if ri < start:
                    continue
                with self.track_time():
                    extracted_data = dt.process_record(record)
                    if not extracted_data:
                        continue
                    document = self.get_document_from_dict(extracted_data, filepath, ri)
                    if not document:
                        continue
                yield document


def _process_raw(raw: bytes) -> dict | None:
    from warcio.archiveiterator import ArchiveIterator

    return dt.process_record(next(iter(ArchiveIterator(io.BytesIO(raw)))))
