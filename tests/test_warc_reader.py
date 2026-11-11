"""datatrove_rs.readers.WarcReader against datatrove's on hand-built WARC files, including malformed ones."""

import gzip
import hashlib
import os

import pytest

pytest.importorskip("warcio")
pytest.importorskip("cchardet")
pytest.importorskip("magic")

from datatrove.pipeline.readers import WarcReader as DtWarcReader  # noqa: E402

from datatrove_rs.readers import WarcReader  # noqa: E402

HTML = b"<html><body><p>Hello world</p></body></html>"
FINEWEB_WARC = os.environ.get("DATATROVE_RS_WARC", "")


def record(rec_type="response", uri="https://example.com/a", payload_type="text/html", body=HTML,
           http=b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n", extra=(), date="2024-02-20T21:10:55Z",
           trailer=b"\r\n\r\n", length=None, rid="<urn:uuid:1>"):
    block = (http if rec_type == "response" else b"") + body
    headers = [("WARC-Type", rec_type), ("WARC-Record-ID", rid), ("WARC-Target-URI", uri), ("WARC-Date", date),
               ("WARC-Identified-Payload-Type", payload_type), *extra,
               ("Content-Length", str(len(block)) if length is None else length)]
    head = b"".join(f"{k}: {v}\r\n".encode() for k, v in headers if v is not None)
    return b"WARC/1.0\r\n" + head + b"\r\n" + block + trailer


def members(*records):
    return b"".join(gzip.compress(r) for r in records)


GOOD = [
    record(rec_type="warcinfo", uri=None, payload_type=None, body=b"software: test\r\n"),
    record(rec_type="request", payload_type=None, body=b"GET /a HTTP/1.1\r\nHost: example.com\r\n\r\n"),
    record(),
    record(rec_type="metadata", payload_type=None, body=b"fetchTimeMs: 12\r\n"),
    record(uri="https://example.com/img", payload_type="image/png", body=b"\x89PNG\r\n"),
    record(uri="https://example.com/x", payload_type="application/xhtml+xml", rid="<urn:uuid:2>"),
    record(uri="https://example.com/latin1", body="<p>caf\xe9 cr\xe8me br\xfbl\xe9e d\xe9j\xe0 vu</p>".encode("latin-1")),
    record(uri="https://example.com/nomime", payload_type=None),
    record(uri="https://example.com/nomime-pdf", payload_type=None, body=b"%PDF-1.4 binary"),
    record(uri="https://example.com/empty", body=b""),
    record(uri="https://example.com/nohttp", http=b"\r\n"),
    record(uri="https://example.com/chunked",
           http=b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n", body=b"5\r\n<p>hi\r\n0\r\n\r\n"),
    record(uri="https://example.com/gz", http=b"HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\n\r\n",
           body=gzip.compress(HTML)),
    record(uri="https://example.com/fold", http=b"HTTP/1.1 200 OK\r\nX-A: 1\r\n  folded\r\n\tmore\r\nbad line\r\n\r\n"),
    record(uri="dns:example.com", body=b"example.com. 300 IN A 1.2.3.4"),
    record(rec_type="conversion", uri="https://example.com/wet", payload_type="text/plain", body=b"plain text"),
    record(rec_type="conversion", uri="https://example.com/wet2", payload_type="text/html", body=b"<p>x</p>"),
    record(rec_type="revisit", payload_type="text/html", body=b""),
    record(uri="https://example.com/caps", extra=[("warc-identified-payload-type", "image/jpeg")]),
    record(uri="https://example.com/été", rid="<urn:uuid:é>"),
    record(uri="https://example.com/a b"),
    record(uri="<https://example.com/angle>"),
    record(uri="https://example.com/sp", extra=[("X-Weird", " nbsp")], body=b"\xef\xbb\xbf<p>bom</p>"),
    record(uri="https://example.com/surrogate", body=b"<p>\xed\xa0\x80</p>"),
    record(trailer=b"\r\n"),
    record(trailer=b"\r\n\r\n\r\n\r\n"),
]


def write(tmp_path, name, data):
    (tmp_path / name).write_bytes(data)
    return name


def read(cls, folder, name, **kwargs):
    reader = cls(str(folder), glob_pattern=name, default_metadata={"dump": "test"}, **kwargs)
    docs, error = [], None
    try:
        for d in reader.run():
            text = hashlib.sha1(d.text.encode("utf-8", "surrogatepass")).hexdigest() if len(d.text) > 4096 else d.text
            docs.append((d.id, text, d.media, d.metadata))
    except Exception as e:  # noqa: BLE001 - both readers must fail the same way
        error = type(e)
    stats = {k: v.total for k, v in reader.stats.stats.items()}
    return docs, error, stats


def assert_same(tmp_path, name, **kwargs):
    ours, theirs = read(WarcReader, tmp_path, name, **kwargs), read(DtWarcReader, tmp_path, name, **kwargs)
    assert ours == theirs
    return ours


def test_well_formed_members(tmp_path):
    docs, error, _ = assert_same(tmp_path, write(tmp_path, "a.warc.gz", members(*GOOD)))
    assert error is None and len(docs) > 10


def test_limit_and_skip(tmp_path):
    name = write(tmp_path, "a.warc.gz", members(*GOOD))
    assert_same(tmp_path, name, limit=3)
    assert_same(tmp_path, name, skip=2, limit=4)


def test_single_gzip_member_and_padding(tmp_path):
    assert_same(tmp_path, write(tmp_path, "one.warc.gz", gzip.compress(b"".join(GOOD))))
    assert_same(tmp_path, write(tmp_path, "pad.warc.gz", members(*GOOD[:4]) + b"\0" * 8 + members(*GOOD[4:])))


@pytest.mark.parametrize(
    "bad",
    [
        record(trailer=b"junk\r\n\r\n"),
        record(uri="https://example.com/nodate", date=None),
        record(uri="https://example.com/emptydate", date=""),
        record(length="12x"),
        record(length="99999"),
        record(length="-3"),
        record(rec_type="response", uri=None),
        b"\r\n" + record(),
        b"HTTP/1.1 200 OK\r\n\r\n",
        record()[:-20],
        record().replace(b"WARC-Type:", b"WARC-Type\xe2\x84\xaa:"),
        record().replace(b"WARC/1.0", b"warc/1.0"),
        record().replace(b"WARC/1.0", b"WARC/2.0"),
    ],
)
def test_malformed_records_fall_back(tmp_path, bad):
    assert_same(tmp_path, write(tmp_path, "bad.warc.gz", members(GOOD[2], bad, *GOOD[5:9])))


def _big(n=4000):
    """~10 MB of members, more than the reader's lookahead, so damage far from the start is exercised too."""
    filler = HTML * 50
    return [record(uri=f"https://example.com/{i}", rid=f"<urn:uuid:{i}>", body=filler + str(i).encode())
            for i in range(n)]


@pytest.mark.parametrize("where", [0.1, 0.5, 0.97])
def test_corrupt_gzip(tmp_path, where):
    data = members(*GOOD, *_big())
    cut = int(len(data) * where)
    flipped = bytearray(data)
    flipped[cut] ^= 0xFF
    assert_same(tmp_path, write(tmp_path, "corrupt.warc.gz", bytes(flipped)))
    assert_same(tmp_path, write(tmp_path, "trunc.warc.gz", data[:cut]))
    assert_same(tmp_path, write(tmp_path, "garbage.warc.gz", data + b"garbage"))


def test_uncompressed_and_explicit_compression(tmp_path):
    assert_same(tmp_path, write(tmp_path, "plain.warc", b"".join(GOOD)))
    assert_same(tmp_path, write(tmp_path, "nosuffix", members(*GOOD)), compression="gzip")
    assert_same(tmp_path, write(tmp_path, "raw.warc.gz", members(*GOOD)), compression=None)


@pytest.mark.skipif(not os.path.isfile(FINEWEB_WARC), reason="set DATATROVE_RS_WARC to a Common Crawl .warc.gz")
def test_common_crawl_file():
    folder, name = os.path.split(FINEWEB_WARC)
    docs, error, _ = assert_same(folder, name, limit=int(os.environ.get("DATATROVE_RS_WARC_LIMIT", "2000")))
    assert error is None and docs
