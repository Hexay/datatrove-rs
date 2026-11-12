//! Chinese and Japanese for `Words`: datatrove's `SpaCyTokenizer("zh")` (jieba) and `SpaCyTokenizer("ja")`
//! (SudachiPy, one spaCy doc per 40,000-byte chunk). Their data comes from the installed Python packages, so it is
//! registered at runtime (`load_jieba`, `load_sudachi`) before `Words::new("zh" | "ja")`.

use crate::pystr::char_len;
use cjk_tokenizer_rs::japanese::{self, Sudachi};
use cjk_tokenizer_rs::jieba::Jieba;
use cjk_tokenizer_rs::Span;
use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

pub const LANGUAGES: [&str; 2] = ["zh", "ja"];

static JIEBA: OnceLock<Jieba> = OnceLock::new();
static SUDACHI: OnceLock<Sudachi> = OnceLock::new();

thread_local! {
    static JA_STATE: RefCell<Option<japanese::State<'static>>> = const { RefCell::new(None) };
    static ZH_WORDS: RefCell<Vec<Span>> = const { RefCell::new(Vec::new()) };
    static FAILED: Cell<bool> = const { Cell::new(false) };
}

/// Registers jieba's dictionary; later calls keep the first.
pub fn load_jieba(jieba: Jieba) {
    let _ = JIEBA.set(jieba);
}

/// Registers the Sudachi dictionary; later calls keep the first.
pub fn load_sudachi(sudachi: Sudachi) {
    let _ = SUDACHI.set(sudachi);
}

/// True if a tokenization on this thread hit input datatrove's own tokenizer raises on, since the last call.
/// The doc's results are then meaningless and the caller must run datatrove's Python code instead.
pub fn take_failure() -> bool {
    FAILED.replace(false)
}

#[derive(Clone, Copy)]
pub enum Cjk {
    Zh(&'static Jieba),
    Ja(&'static Sudachi),
}

impl Cjk {
    /// None when `lang` is not zh/ja, Err when its data was not registered.
    pub fn new(lang: &str) -> Option<Result<Self, String>> {
        let missing = || format!("{lang} tokenizer data not loaded");
        match lang {
            "zh" => Some(JIEBA.get().map(Cjk::Zh).ok_or_else(missing)),
            "ja" => Some(SUDACHI.get().map(Cjk::Ja).ok_or_else(missing)),
            _ => None,
        }
    }

    /// Doc tokens into `tokens`; `docs` gets (token end, byte start) per spaCy doc when there are several.
    /// Returns false (and flags the failure) where datatrove raises.
    pub fn tokenize(self, text: &str, tokens: &mut Vec<Span>, docs: &mut Vec<(usize, u32)>) -> bool {
        tokens.clear();
        docs.clear();
        let ok = match self {
            Cjk::Zh(j) => {
                ZH_WORDS.with_borrow_mut(|w| j.tokenize(text, 0, w, tokens));
                true
            }
            Cjk::Ja(s) => JA_STATE.with_borrow_mut(|st| {
                s.tokenize(text, st.get_or_insert_with(|| s.state()), tokens, docs).is_ok()
            }),
        };
        if !ok {
            tokens.clear();
            docs.clear();
            FAILED.set(true);
        }
        ok
    }
}

/// datatrove's `span_tokenize` sentence ends over several docs, in chars: each doc's spans are offset by the
/// previous span's end rather than the chunk start, which drifts when a chunk ends in a token's trailing space.
/// `sentence_starts(tokens)` gives the token indices starting a sentence within one doc.
pub fn chunked_sentence_ends(
    text: &str,
    tokens: &[Span],
    docs: &[(usize, u32)],
    mut sentence_starts: impl FnMut(&[Span]) -> Vec<usize>,
) -> Vec<usize> {
    let mut ends = Vec::new();
    let mut first = 0;
    for &(last, byte_start) in docs {
        let toks = &tokens[first..last];
        let starts = sentence_starts(toks);
        let start = ends.last().copied().unwrap_or(0);
        for k in 0..starts.len() {
            let end_tok = if k + 1 == starts.len() { toks.len() - 1 } else { starts[k + 1] - 1 };
            ends.push(start + char_len(&text[byte_start as usize..toks[end_tok].1 as usize]));
        }
        first = last;
    }
    ends
}

/// `[text[a:b] for a, b in zip([0] + ends[:-1], ends[:-1] + [len(text)])]` for strictly ascending char offsets.
pub fn split_at_chars<'a>(text: &'a str, ends: &[usize]) -> Vec<&'a str> {
    let mut offs = text.char_indices().map(|(i, _)| i).chain(std::iter::once(text.len())).enumerate();
    let mut out = Vec::with_capacity(ends.len().max(1));
    let mut prev = 0;
    for &e in &ends[..ends.len().saturating_sub(1)] {
        let b = offs.by_ref().find(|&(k, _)| k == e).map_or(text.len(), |(_, b)| b);
        out.push(&text[prev..b]);
        prev = b;
    }
    out.push(&text[prev..]);
    out
}
