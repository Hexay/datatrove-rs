//! datatrove's `SpaCyTokenizer(lang)`: `spacy.blank(lang)` + the `sentencizer` pipe, for every language with
//! embedded rules (`spacy_tokenizer_rs::languages`), and Chinese/Japanese (`cjk_words`).

use crate::cjk_words::{self, Cjk};
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

#[derive(Clone, Copy)]
enum Engine {
    Spacy(&'static Tokenizer),
    Cjk(Cjk),
}

/// Per-caller token buffers; tokenizer state is per thread.
pub struct Words {
    id: usize,
    engine: Engine,
    tokens: Vec<Span>,
    starts: Vec<usize>,
    /// (token end, byte start) per spaCy doc when datatrove tokenizes the text in several chunks (Japanese).
    docs: Vec<(usize, u32)>,
}

impl Default for Words {
    fn default() -> Self {
        Self::new("en").expect("embedded English rules")
    }
}

impl Words {
    /// Words for spaCy language code `lang`; Err when no Rust tokenizer is available for it.
    pub fn new(lang: &str) -> Result<Self, String> {
        let (id, engine) = match Cjk::new(lang) {
            Some(cjk) => (usize::MAX - cjk_words::LANGUAGES.iter().position(|&l| l == lang).unwrap(), Engine::Cjk(cjk?)),
            None => {
                let (id, tok) = shared_tokenizer(lang)?;
                (id, Engine::Spacy(tok))
            }
        };
        Ok(Self { id, engine, tokens: Vec::new(), starts: Vec::new(), docs: Vec::new() })
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
        if self.docs.len() > 1 {
            return self.chunked_sentence_ends(text).len();
        }
        sentence_starts(text, &self.tokens, &mut self.starts);
        self.starts.len().max(1)
    }

    /// `split_into_sentences(text)`: consecutive pieces covering `text`, cut at each sentence's `end_char`.
    /// With no sentences (empty text) datatrove's zip still yields one piece, the whole text.
    pub fn sentences<'a>(&mut self, text: &'a str) -> Vec<&'a str> {
        self.tokenize(text);
        if self.docs.len() > 1 {
            return cjk_words::split_at_chars(text, &self.chunked_sentence_ends(text));
        }
        sentence_starts(text, &self.tokens, &mut self.starts);
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

    fn chunked_sentence_ends(&mut self, text: &str) -> Vec<usize> {
        let starts = &mut self.starts;
        cjk_words::chunked_sentence_ends(text, &self.tokens, &self.docs, |toks| {
            sentence_starts(text, toks, starts);
            starts.clone()
        })
    }

    /// Tokenizes `text` into `self.tokens` with the thread's shared state (buffers are swapped, not copied).
    /// False where datatrove's tokenizer raises (see `cjk_words::take_failure`).
    fn tokenize(&mut self, text: &str) -> bool {
        let tok = match self.engine {
            Engine::Spacy(tok) => tok,
            Engine::Cjk(cjk) => return cjk.tokenize(text, &mut self.tokens, &mut self.docs),
        };
        STATES.with_borrow_mut(|states| {
            if states.len() <= self.id {
                states.resize_with(self.id + 1, || None);
            }
            let st = states[self.id].get_or_insert_with(|| State::new(CHUNK_CACHE));
            std::mem::swap(&mut st.tokens, &mut self.tokens);
            tok.tokenize(text, st);
            std::mem::swap(&mut st.tokens, &mut self.tokens);
        });
        true
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
        if !hit && self.tokenize(text) {
            LAST_DOC.set(Some((self.id, text.into(), self.tokens.clone())));
        }
    }
}

/// Token indices where `Sentencizer.predict` starts a sentence in a doc of `tokens`.
fn sentence_starts(text: &str, tokens: &[Span], starts: &mut Vec<usize>) {
    let t = py();
    starts.clear();
    if tokens.is_empty() {
        return;
    }
    starts.push(0);
    let mut seen_period = false;
    for (i, &(a, b)) in tokens.iter().enumerate() {
        let s = &text[a as usize..b as usize];
        let in_punct = t.sentencizer_punct.contains(s);
        if seen_period && !in_punct && !s.chars().all(|c| t.punct.contains(c)) {
            starts.push(i);
            seen_period = false;
        } else if in_punct {
            seen_period = true;
        }
    }
}

/// `len(sep.join(words))` for a separator of `sep_chars` code points.
pub fn joined_len(words: &[&str], sep_chars: usize) -> usize {
    words.iter().map(|w| char_len(w)).sum::<usize>() + sep_chars * words.len().saturating_sub(1)
}
