//! datatrove's `SpaCyTokenizer(lang)`: `spacy.blank(lang)` + the `sentencizer` pipe, for every language with
//! embedded rules (`spacy_tokenizer_rs::languages`).

use crate::pystr::{char_len, strip};
use crate::pyunicode::py;
pub use spacy_tokenizer_rs::languages;
use spacy_tokenizer_rs::specials::Span;
use spacy_tokenizer_rs::{tokenizer, Backend, State, Tokenizer};
use std::cell::RefCell;
use std::sync::OnceLock;

/// Chunk-cache capacity. The cache never changes output (spike parity ran with and without it); see
/// `profile --stream-cache` for the size trade-off.
const CHUNK_CACHE: usize = 100_000;

type LastDoc = Option<(usize, Box<str>, Vec<Span>)>;

thread_local! {
    /// One tokenizer state per thread and language, so every filter shares the chunk cache: FineWeb
    /// re-tokenizing C4's rewritten text then hits the chunks C4 just tokenized.
    static STATES: RefCell<Vec<Option<State>>> = const { RefCell::new(Vec::new()) };
    static LAST_DOC: RefCell<LastDoc> = const { RefCell::new(None) };
}

type Slot = (&'static str, OnceLock<Result<Tokenizer, String>>);

/// (index, tokenizer) for `lang`, compiled on first use and shared by all threads.
fn shared_tokenizer(lang: &str) -> Result<(usize, &'static Tokenizer), String> {
    static SLOTS: OnceLock<Vec<Slot>> = OnceLock::new();
    let slots = SLOTS.get_or_init(|| languages().map(|l| (l, OnceLock::new())).collect());
    let id = slots.iter().position(|(l, _)| *l == lang).ok_or_else(|| format!("no Rust tokenizer for {lang:?}"))?;
    let tok = slots[id].1.get_or_init(|| tokenizer(lang, Backend::Split).expect("listed language"));
    tok.as_ref().map(|t| (id, t)).map_err(|e| format!("{lang} rules: {e}"))
}

/// Per-caller token buffers; tokenizer state is per thread.
pub struct Words {
    id: usize,
    tok: &'static Tokenizer,
    tokens: Vec<Span>,
    starts: Vec<usize>,
}

impl Default for Words {
    fn default() -> Self {
        Self::new("en").expect("embedded English rules")
    }
}

impl Words {
    /// Words for spaCy language code `lang`; Err when no rules are embedded for it.
    pub fn new(lang: &str) -> Result<Self, String> {
        let (id, tok) = shared_tokenizer(lang)?;
        Ok(Self { id, tok, tokens: Vec::new(), starts: Vec::new() })
    }

    /// `split_into_words(text)`: token texts, stripped, empties dropped.
    pub fn words<'a>(&mut self, text: &'a str, out: &mut Vec<&'a str>) {
        out.clear();
        self.tokenize_shared(text);
        out.extend(self.tokens.iter().map(|&(a, b)| strip(&text[a as usize..b as usize])).filter(|w| !w.is_empty()));
    }

    /// `len(split_into_sentences(text))`.
    pub fn sentence_count(&mut self, text: &str) -> usize {
        self.tokenize(text);
        self.find_sentence_starts(text);
        self.starts.len().max(1)
    }

    /// `split_into_sentences(text)`: consecutive pieces covering `text`, cut at each sentence's `end_char`.
    /// With no sentences (empty text) datatrove's zip still yields one piece, the whole text.
    pub fn sentences<'a>(&mut self, text: &'a str) -> Vec<&'a str> {
        self.tokenize(text);
        self.find_sentence_starts(text);
        let tokens = &self.tokens;
        let n = self.starts.len();
        if n == 0 {
            return vec![text];
        }
        let mut out = Vec::with_capacity(n);
        let mut a = 0;
        for k in 0..n {
            let b = if k + 1 == n { text.len() } else { tokens[self.starts[k + 1] - 1].1 as usize };
            out.push(&text[a..b]);
            a = b;
        }
        out
    }

    /// Tokenizes `text` into `self.tokens` with the thread's shared state (buffers are swapped, not copied).
    fn tokenize(&mut self, text: &str) {
        STATES.with_borrow_mut(|states| {
            if states.len() <= self.id {
                states.resize_with(self.id + 1, || None);
            }
            let st = states[self.id].get_or_insert_with(|| State::new(CHUNK_CACHE));
            std::mem::swap(&mut st.tokens, &mut self.tokens);
            self.tok.tokenize(text, st);
            std::mem::swap(&mut st.tokens, &mut self.tokens);
        });
    }

    /// Tokenizes via a per-thread cache of the last whole-doc tokenization, so consecutive filters on the same
    /// doc (datatrove chains them per doc) tokenize it once, as datatrove's `lru_cache` on `split_into_parts` does.
    fn tokenize_shared(&mut self, text: &str) {
        let hit = LAST_DOC.with_borrow(|last| match last {
            Some((id, t, spans)) if *id == self.id && **t == *text => {
                self.tokens.clone_from(spans);
                true
            }
            _ => false,
        });
        if !hit {
            self.tokenize(text);
            LAST_DOC.set(Some((self.id, text.into(), self.tokens.clone())));
        }
    }

    /// Token indices where `Sentencizer.predict` starts a sentence.
    fn find_sentence_starts(&mut self, text: &str) {
        let t = py();
        self.starts.clear();
        if self.tokens.is_empty() {
            return;
        }
        self.starts.push(0);
        let mut seen_period = false;
        for (i, &(a, b)) in self.tokens.iter().enumerate() {
            let s = &text[a as usize..b as usize];
            let in_punct = t.sentencizer_punct.contains(s);
            if seen_period && !in_punct && !s.chars().all(|c| t.punct.contains(c)) {
                self.starts.push(i);
                seen_period = false;
            } else if in_punct {
                seen_period = true;
            }
        }
    }
}

/// `len(sep.join(words))` for a separator of `sep_chars` code points.
pub fn joined_len(words: &[&str], sep_chars: usize) -> usize {
    words.iter().map(|w| char_len(w)).sum::<usize>() + sep_chars * words.len().saturating_sub(1)
}
