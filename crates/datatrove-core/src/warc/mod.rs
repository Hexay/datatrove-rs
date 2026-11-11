//! datatrove's `WarcReader.read_file` record loop (warcio `ArchiveIterator` over fsspec's gzip stream, then
//! `process_record`), for well-formed WARC. Anything warcio could treat differently is handed back to Python:
//! a single record as `Item::Raw`, or the rest of the file as `Item::Fallback`. See docs/parity.md "WarcReader".

mod headers;
mod stream;
#[cfg(test)]
mod tests;

use flate2::bufread::MultiGzDecoder;
use headers::{is_blank_line, is_warc_status, lines, parse_length, Headers};
use std::io::{BufReader, Read};
use std::ops::Range;
use stream::Stream;

/// A record is returned only once this much input after it decompressed cleanly (or EOF was reached). Python's
/// gzip and warcio read ahead (< 1 MB), so on a truncated or corrupt gzip datatrove may stop or raise while handling
/// an earlier record; with the lookahead, such damage makes `Item::Fallback` before any record datatrove would not
/// have yielded, and datatrove's own loop then stops or raises exactly where it would have.
const LOOKAHEAD: usize = 4 << 20;

pub struct Doc<'a> {
    /// Record number in the file, counting every record (`enumerate(ArchiveIterator(f))`).
    pub index: usize,
    pub id: Option<&'a str>,
    pub url: &'a str,
    pub date: &'a str,
    /// Payload, not yet decoded: UTF-8 decoding stays with CPython so failures take datatrove's charset path.
    pub body: &'a [u8],
    /// The whole record, for `Item::Raw` when `body` isn't UTF-8.
    pub raw: &'a [u8],
}

pub enum Item<'a> {
    Doc(Doc<'a>),
    /// A selected record datatrove may treat specially: run its own `process_record` on these record bytes.
    Raw {
        index: usize,
        raw: &'a [u8],
    },
    /// From record `index` on, the file must be read by datatrove itself.
    Fallback {
        index: usize,
    },
    End,
}

enum Parsed {
    Skip,
    Doc { headers: Headers, body: Range<usize> },
    Raw,
}

pub struct WarcRecords<R> {
    stream: Stream<R>,
    index: usize,
    done: bool,
    headers: Option<Headers>,
    record: Range<usize>,
    body: Range<usize>,
}

pub type GzipRecords<R> = WarcRecords<MultiGzDecoder<BufReader<R>>>;

/// Records of a gzip WARC, decompressed like Python's `gzip.GzipFile` (members concatenated); a gzip error makes
/// `Item::Fallback`, since Python may then fail or recover differently.
pub fn gzip_records<R: Read>(src: R) -> GzipRecords<R> {
    WarcRecords::new(MultiGzDecoder::new(BufReader::with_capacity(1 << 20, src)))
}

impl<R: Read> WarcRecords<R> {
    /// `src` yields the decompressed WARC bytes.
    pub fn new(src: R) -> Self {
        Self { stream: Stream::new(src), index: 0, done: false, headers: None, record: 0..0, body: 0..0 }
    }

    pub fn next_item(&mut self) -> Item<'_> {
        if self.done {
            return Item::End;
        }
        loop {
            let index = self.index;
            match self.next_record() {
                Err(()) => {
                    self.done = true;
                    return Item::Fallback { index };
                }
                Ok(None) => {
                    self.done = true;
                    return Item::End;
                }
                Ok(Some(Parsed::Skip)) => continue,
                Ok(Some(Parsed::Raw)) => {
                    return Item::Raw { index, raw: self.stream.bytes(self.record.clone()) };
                }
                Ok(Some(Parsed::Doc { headers, body })) => {
                    self.headers = Some(headers);
                    self.body = body;
                    let h = self.headers.as_ref().unwrap();
                    return Item::Doc(Doc {
                        index,
                        id: h.get("WARC-Record-ID"),
                        url: h.get("WARC-Target-URI").unwrap(),
                        date: h.get("WARC-Date").unwrap(),
                        body: self.stream.bytes(self.body.clone()),
                        raw: self.stream.bytes(self.record.clone()),
                    });
                }
            }
        }
    }

    /// One record: Ok(None) at EOF, Err when datatrove must take over from this record.
    fn next_record(&mut self) -> Result<Option<Parsed>, ()> {
        let s = &mut self.stream;
        s.compact();
        let status = s.readline().map_err(drop)?;
        if status.is_empty() {
            return Ok(None);
        }
        if !is_warc_status(s.bytes(status.clone())) {
            return Err(());
        }
        let mut header_lines = Vec::new();
        loop {
            let line = s.readline().map_err(drop)?;
            if is_blank_line(s.bytes(line.clone())) {
                break;
            }
            header_lines.push(line);
        }
        let headers = Headers::parse(header_lines.into_iter().map(|r| s.bytes(r))).ok_or(())?;
        let uri = headers.get("WARC-Target-URI");
        if uri.is_some_and(|u| (u.starts_with('<') && u.ends_with('>')) || u.contains(' ')) {
            return Err(()); // warcio rewrites the URI and logs a warning
        }
        let length = headers.get("Content-Length").and_then(parse_length).ok_or(())?;
        let rec_type = headers.get("WARC-Type");
        let http_record = matches!(rec_type, Some("response" | "request" | "revisit"));
        if http_record && length > 0 && uri.is_none() {
            return Err(()); // warcio raises on `uri.startswith`
        }
        let block = s.take(length).map_err(drop)?;
        if block.len() < length {
            return Err(());
        }
        let record_end = s.pos();
        skip_blank_lines(s)?;
        s.ensure(LOOKAHEAD).map_err(drop)?;
        self.index += 1;
        self.record = status.start..record_end;
        Ok(Some(classify(headers, s.bytes(block.clone()), block.start)))
    }
}

/// `ArchiveIterator._consume_blanklines`, minus its warning path (a non-blank first line), which goes to Python.
fn skip_blank_lines<R: Read>(s: &mut Stream<R>) -> Result<(), ()> {
    let mut first = true;
    loop {
        let line = s.readline().map_err(drop)?;
        if line.is_empty() {
            return Ok(());
        }
        if !s.bytes(line.clone()).trim_ascii_end().is_empty() {
            if first {
                return Err(());
            }
            s.rewind(line.start); // the next record's status line
            return Ok(());
        }
        first = false;
    }
}

/// `process_record`'s selection, plus warcio's HTTP header parsing and `content_stream()` for response records.
fn classify(headers: Headers, block: &[u8], offset: usize) -> Parsed {
    let rec_type = headers.get("WARC-Type");
    let conversion = rec_type == Some("conversion");
    if rec_type != Some("response") && !conversion {
        return Parsed::Skip;
    }
    match headers.get("WARC-Identified-Payload-Type") {
        None => return Parsed::Raw, // datatrove sniffs the payload with libmagic
        Some(m) if m != "text/html" && m != "application/xhtml+xml" && !(conversion && m == "text/plain") => {
            return Parsed::Skip;
        }
        Some(_) => {}
    }
    let is_http = |u: &str| u.starts_with("http:") || u.starts_with("https:");
    let mut start = 0;
    if !conversion && !block.is_empty() && headers.get("WARC-Target-URI").is_some_and(is_http) {
        match http_body_start(block) {
            Some(n) => start = n,
            None => return Parsed::Raw,
        }
    }
    let truthy = |name| headers.get(name).is_some_and(|v| !v.is_empty());
    if !truthy("WARC-Target-URI") || !truthy("WARC-Date") {
        return Parsed::Raw; // datatrove falls back to ARC header names
    }
    Parsed::Doc { headers, body: offset + start..offset + block.len() }
}

/// Offset of the payload after the HTTP status line and headers; None when `content_stream()` would decode it.
fn http_body_start(block: &[u8]) -> Option<usize> {
    let mut it = lines(block);
    let status = it.next()?;
    let mut consumed = status.len();
    if is_blank_line(status) {
        return Some(consumed);
    }
    let mut header_lines = Vec::new();
    for line in it {
        consumed += line.len();
        if is_blank_line(line) {
            break;
        }
        header_lines.push(line);
    }
    let headers = Headers::parse(header_lines)?;
    let encoded = headers.get("content-encoding").is_some_and(|v| !v.is_empty());
    (!encoded && headers.get("transfer-encoding").is_none()).then_some(consumed)
}
