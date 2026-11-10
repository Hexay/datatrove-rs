//! `"".join(words)` with per-word byte offsets, shared by the top and dup n-gram checks.

/// Zero bytes kept after the joined text so `head` can always load 16 bytes.
const PAD: usize = 16;

#[derive(Default)]
pub(super) struct Joined {
    bytes: Vec<u8>,
    byte_off: Vec<u32>,
}

impl Joined {
    pub(super) fn build(&mut self, words: &[&str]) {
        let Self { bytes, byte_off } = self;
        bytes.clear();
        byte_off.clear();
        byte_off.push(0);
        let mut end = 0;
        byte_off.extend(words.iter().map(|w| {
            end += w.len() as u32;
            end
        }));
        bytes.reserve(end as usize + PAD);
        for w in words {
            bytes.extend_from_slice(w.as_bytes());
        }
        bytes.resize(bytes.len() + PAD, 0);
    }

    pub(super) fn n_words(&self) -> usize {
        self.byte_off.len() - 1
    }

    /// Byte offset of each word, then of the end.
    pub(super) fn offsets(&self) -> &[u32] {
        &self.byte_off
    }

    /// Byte offset of word i in the joined text (i = n_words gives its end).
    pub(super) fn start(&self, i: usize) -> usize {
        self.byte_off[i] as usize
    }

    /// `"".join(words[i:i+n])` as bytes.
    pub(super) fn span(&self, i: usize, n: usize) -> &[u8] {
        &self.bytes[self.start(i)..self.start(i + n)]
    }

    /// Python `len("".join(words[i:i+n]))`: code points are the UTF-8 bytes that are not continuation bytes.
    pub(super) fn chars(&self, i: usize, n: usize) -> usize {
        self.span(i, n).iter().filter(|&&b| b & 0xc0 != 0x80).count()
    }

    /// 16 bytes from byte offset `a`, zero past the end of the joined text.
    pub(super) fn head(&self, a: usize) -> u128 {
        u128::from_le_bytes(self.bytes[a..a + 16].try_into().unwrap())
    }

    /// The first min(len, 16) bytes from byte offset `a`, zero-padded.
    fn key16(&self, a: usize, len: usize) -> u128 {
        self.head(a) & MASKS[len.min(16)]
    }

    /// Key of word i, equal for equal words; injective over words shorter than 16 bytes (their zero-padded
    /// bytes, with the length in the top byte).
    pub(super) fn word_key(&self, i: usize) -> u128 {
        let (a, len) = (self.start(i), self.start(i + 1) - self.start(i));
        self.key16(a, len.min(15)) | (len.min(255) as u128) << 120
    }

    /// Exact key of the `len` < 16 bytes at byte offset `a`: zero-padded, with the length in the top byte.
    pub(super) fn short_key(&self, a: usize, len: usize) -> u128 {
        debug_assert!(len < 16);
        self.key16(a, len) | (len as u128) << 120
    }

    /// Hash of the `len` bytes at byte offset `a`, equal for equal byte strings (it reads at most 16 of them).
    pub(super) fn word_hash(&self, a: usize, len: usize) -> u64 {
        let k = self.key16(a, len);
        ((k as u64) ^ (len as u64).rotate_right(8)).wrapping_mul(0xff51_afd7_ed55_8ccd)
            ^ ((k >> 64) as u64).wrapping_mul(0xc4ce_b9fe_1a85_ec53)
    }
}

/// MASKS[l] keeps the low l bytes.
const MASKS: [u128; 17] = {
    let mut m = [0; 17];
    let mut l = 1;
    while l <= 16 {
        m[l] = u128::MAX >> (128 - 8 * l);
        l += 1;
    }
    m
};
