//! A growable read buffer handing out byte ranges, which stay valid until the next `compact`.

use std::io::{self, Read};
use std::ops::Range;

const CHUNK: usize = 1 << 18;
const COMPACT_AT: usize = 16 << 20;

pub struct Stream<R> {
    src: R,
    buf: Vec<u8>,
    pos: usize,
    eof: bool,
}

impl<R: Read> Stream<R> {
    pub fn new(src: R) -> Self {
        Self { src, buf: Vec::new(), pos: 0, eof: false }
    }

    pub fn bytes(&self, r: Range<usize>) -> &[u8] {
        &self.buf[r]
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn rewind(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// Drops consumed bytes once there are many; invalidates every range handed out so far.
    pub fn compact(&mut self) {
        if self.pos >= COMPACT_AT && self.pos * 2 >= self.buf.len() {
            self.buf.drain(..self.pos);
            self.pos = 0;
        }
    }

    /// Reads more input; false at EOF.
    fn fill(&mut self, want: usize) -> io::Result<bool> {
        if self.eof {
            return Ok(false);
        }
        let start = self.buf.len();
        self.buf.resize(start + want.max(CHUNK), 0);
        let n = loop {
            match self.src.read(&mut self.buf[start..]) {
                Ok(n) => break n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    self.buf.truncate(start);
                    return Err(e);
                }
            }
        };
        self.buf.truncate(start + n);
        self.eof = n == 0;
        Ok(n > 0)
    }

    /// Next line including its `\n`, or up to EOF; empty at EOF.
    pub fn readline(&mut self) -> io::Result<Range<usize>> {
        let start = self.pos;
        let mut scanned = start;
        loop {
            if let Some(i) = memchr::memchr(b'\n', &self.buf[scanned..]) {
                self.pos = scanned + i + 1;
                return Ok(start..self.pos);
            }
            scanned = self.buf.len();
            if !self.fill(0)? {
                self.pos = scanned;
                return Ok(start..scanned);
            }
        }
    }

    /// Buffers at least `n` unread bytes, or everything up to EOF.
    pub fn ensure(&mut self, n: usize) -> io::Result<()> {
        while self.buf.len() - self.pos < n {
            let missing = n - (self.buf.len() - self.pos);
            if !self.fill(missing)? {
                break;
            }
        }
        Ok(())
    }

    /// Up to `n` bytes; fewer only at EOF.
    pub fn take(&mut self, n: usize) -> io::Result<Range<usize>> {
        self.ensure(n)?;
        let start = self.pos;
        self.pos = (start + n).min(self.buf.len());
        Ok(start..self.pos)
    }
}
