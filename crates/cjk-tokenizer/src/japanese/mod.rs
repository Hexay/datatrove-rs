//! datatrove's `SpaCyTokenizer("ja")`: `chunk_text_on_bytes(text, 40000)`, then per chunk SudachiPy split mode A
//! with `sudachipy.Dictionary()`'s defaults and `datatrove.utils.japanese_tokenizer`'s token alignment.

mod chunk;

pub use chunk::chunks;

use crate::{align, py_str_isspace, Span};
use std::path::Path;
use sudachi::analysis::mlist::MorphemeList;
use sudachi::analysis::stateful_tokenizer::StatefulTokenizer;
use sudachi::analysis::Mode;
use sudachi::config::ConfigBuilder;
use sudachi::dic::dictionary::JapaneseDictionary;

/// datatrove's chunk size for Japanese.
pub const CHUNK_BYTES: usize = 40_000;

/// The sudachi.rs version compiled in; SudachiPy must be the same release.
pub const SUDACHI_VERSION: &str = "0.7.0";

pub struct Sudachi {
    dict: JapaneseDictionary,
}

/// Per-thread analyzer over a shared dictionary.
pub struct State<'a> {
    tok: StatefulTokenizer<&'a JapaneseDictionary>,
    morphemes: MorphemeList<&'a JapaneseDictionary>,
    words: Vec<(Span, bool)>,
}

impl Sudachi {
    /// What `sudachipy.Dictionary()` loads: `sudachipy._DEFAULT_SETTINGFILE`, resources from
    /// `sudachipy._DEFAULT_RESOURCEDIR` and the system dictionary `sudachipy._find_dict_path("core")`.
    pub fn new(settings: &Path, resource_dir: &Path, system_dict: &Path) -> Result<Self, String> {
        let mut cfg =
            ConfigBuilder::from_file(settings).map_err(|e| e.to_string())?.push_resolver_root(resource_dir).build();
        cfg.system_dict = Some(system_dict.to_owned());
        let dict = JapaneseDictionary::from_cfg(&cfg).map_err(|e| e.to_string())?;
        Ok(Self { dict })
    }

    pub fn state(&self) -> State<'_> {
        State {
            tok: StatefulTokenizer::new(&self.dict, Mode::A),
            morphemes: MorphemeList::empty(&self.dict),
            words: Vec::new(),
        }
    }

    /// Doc tokens of `text` as absolute byte spans; `docs` gets each chunk's (token end index, byte start), since
    /// datatrove makes one spaCy doc per chunk. Err where datatrove would raise.
    pub fn tokenize(
        &self,
        text: &str,
        st: &mut State,
        out: &mut Vec<Span>,
        docs: &mut Vec<(usize, u32)>,
    ) -> Result<(), String> {
        out.clear();
        docs.clear();
        for (a, b) in chunks(text, CHUNK_BYTES) {
            self.tokenize_chunk(&text[a..b], a as u32, st, out)?;
            docs.push((out.len(), a as u32));
        }
        Ok(())
    }

    fn tokenize_chunk(&self, chunk: &str, base: u32, st: &mut State, out: &mut Vec<Span>) -> Result<(), String> {
        st.tok.reset().push_str(chunk);
        st.tok.do_tokenize().map_err(|e| e.to_string())?;
        st.morphemes.collect_results(&mut st.tok).map_err(|e| e.to_string())?;
        st.words.clear();
        for m in st.morphemes.iter() {
            let (a, b) = (m.begin(), m.end());
            if chunk.get(a..b) != Some(&*m.surface()) {
                return Err(format!("surface {:?} is not text[{a}..{b}]", &*m.surface()));
            }
            if a < b {
                st.words.push(((a as u32, b as u32), is_blank_tag(m.part_of_speech())));
            }
        }
        let s = |&((a, b), _): &(Span, bool)| &chunk[a as usize..b as usize];
        // `_get_dtokens` drops a whitespace "空白" token that follows another one.
        let blank = |w: &(Span, bool)| w.1 && py_str_isspace(s(w));
        let words: Vec<&str> = (0..st.words.len())
            .filter(|&i| i == 0 || !blank(&st.words[i]) || !blank(&st.words[i - 1]))
            .map(|i| s(&st.words[i]))
            .collect();
        if !align::same_non_space(chunk, words.iter().copied()) {
            return Err("E194: tokens don't match the text".into());
        }
        if words.is_empty() {
            return Ok(());
        }
        // `get_dtokens_and_spaces` takes a word's trailing space when the next token is " ", not the next char.
        align::align(chunk, &words, base, |i, _, _| words.get(i + 1) == Some(&" "), out)
    }
}

/// datatrove's tag `"-".join(pos[:4] without "*")` is "空白".
fn is_blank_tag(pos: &[String]) -> bool {
    let mut it = pos.iter().take(4).filter(|p| *p != "*");
    it.next().is_some_and(|p| p == "空白") && it.next().is_none()
}
