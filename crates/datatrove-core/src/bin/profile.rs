//! profile <docs.bin> [reps] [stage-substring] — per-stage timing of the Rust core with datatrove's default configs.
//! docs.bin: UTF-8 docs joined by '\0' (see crates/spacy-tokenizer/src/bin/bench.rs).
//! With a stage filter, only matching stages run, so `perf stat -e instructions:u` measures one stage under load.

use datatrove_core::filters::c4::{C4Config, C4};
use datatrove_core::filters::fineweb::{FineWeb, FineWebConfig};
use datatrove_core::filters::gopher_quality::{GopherQuality, GopherQualityConfig};
use datatrove_core::filters::gopher_repetition::{GopherRepetition, GopherRepetitionConfig};
use datatrove_core::minhash::{signature, HashConfig, HashFn, MinHasher};
use datatrove_core::simplify::{NormConfig, Simplifier};
use datatrove_core::words::Words;
use spacy_tokenizer_rs::{english, Backend, State};
use std::time::Instant;

static ONLY: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();

fn time(name: &str, docs: &[&str], reps: usize, mut f: impl FnMut(&str)) {
    if ONLY.get().and_then(Option::as_deref).is_some_and(|o| !name.contains(o)) {
        return;
    }
    docs.iter().take(50).for_each(|d| f(d));
    let mut best = f64::MAX;
    for _ in 0..reps {
        let t = Instant::now();
        docs.iter().for_each(|d| f(d));
        best = best.min(t.elapsed().as_secs_f64());
    }
    println!("{name:34} {:8.0} docs/s {:7.1} us/doc", docs.len() as f64 / best, best / docs.len() as f64 * 1e6);
}

/// One pass over distinct docs, alternating cache sizes per 1000-doc block so machine load hits all equally.
fn stream_cache_sizes(docs: &[&str]) {
    let tok = english(Backend::Split);
    let sizes = [0usize, 10_000, 100_000, 1 << 20];
    let mut states: Vec<State> = sizes.iter().map(|&s| State::new(s)).collect();
    let mut totals = vec![0f64; sizes.len()];
    for block in docs.chunks(1000) {
        for (st, total) in states.iter_mut().zip(&mut totals) {
            let t = Instant::now();
            block.iter().for_each(|d| tok.tokenize(d, st));
            *total += t.elapsed().as_secs_f64();
        }
    }
    for (s, total) in sizes.iter().zip(totals) {
        println!("stream, chunk cache {s:>8}: {:8.0} docs/s {:7.1} us/doc", docs.len() as f64 / total, total / docs.len() as f64 * 1e6);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let raw = std::fs::read_to_string(&args[1]).expect("docs.bin");
    let docs: Vec<&str> = raw.split('\0').collect();
    if args.get(2).is_some_and(|a| a == "--stream-cache") {
        return stream_cache_sizes(&docs);
    }
    let reps = args.get(2).map_or(5, |r| r.parse().unwrap());
    ONLY.set(args.get(3).cloned()).unwrap();

    let tok = english(Backend::Split);
    for (name, cache) in [("tokenizer, no chunk cache", 0), ("tokenizer, cache 10k", 10_000), ("tokenizer, cache 1M", 1 << 20)] {
        let mut st = State::new(cache);
        time(name, &docs, reps, |d| tok.tokenize(d, &mut st));
    }
    let mut st = State::new(1 << 20);
    time("tokenizer, cache 1M, no matcher", &docs, reps, |d| tok.tokenize_without_matcher(d, &mut st));
    let space = &datatrove_core::pyunicode::py().isspace;
    let mut chunks = 0usize;
    time("floor: isspace scan per char", &docs, reps, |d| {
        chunks += d.chars().filter(|&c| space.contains(c)).count();
    });
    let mut map: rustc_hash::FxHashMap<Box<str>, Box<[u32]>> = Default::default();
    for d in &docs {
        for w in d.split(|c| space.contains(c)) {
            map.entry(w.into()).or_insert_with(|| vec![w.len() as u32].into());
        }
    }
    let mut spans: Vec<(u32, u32)> = Vec::new();
    time("floor: scan + 1 map lookup/chunk", &docs, reps, |d| {
        spans.clear();
        for w in d.split(|c| space.contains(c)) {
            if let Some(l) = map.get(w) {
                spans.push((0, l[0]));
            }
        }
    });
    std::hint::black_box(chunks);
    let mut words = Words::default();
    time("tokenize (split_into_words)", &docs, reps, |d| words.words(d, &mut Vec::new()));
    time("sentence_count (whole doc)", &docs, reps, |d| {
        words.sentence_count(d);
    });

    let top = vec![(2, 0.2), (3, 0.18), (4, 0.16)];
    let dup = vec![(5, 0.15), (6, 0.14), (7, 0.13), (8, 0.12), (9, 0.11), (10, 0.10)];
    let never = |v: &[(usize, f64)]| v.iter().map(|&(n, _)| (n, 1e9)).collect::<Vec<_>>();
    let rep_cfg = |top: Vec<(usize, f64)>, dup: Vec<(usize, f64)>, frac: f64| GopherRepetitionConfig {
        dup_line_frac: Some(frac),
        dup_para_frac: Some(frac),
        dup_line_char_frac: Some(frac),
        dup_para_char_frac: Some(frac),
        top_n_grams: top,
        dup_n_grams: dup,
    };
    for (name, cfg) in [
        ("GopherRepetition", rep_cfg(top.clone(), dup.clone(), 0.3)),
        ("  rep: no drops, lines+paras only", rep_cfg(vec![], vec![], 1e9)),
        ("  rep: no drops, + top n-grams", rep_cfg(never(&top), vec![], 1e9)),
        ("  rep: no drops, + dup n-grams", rep_cfg(vec![], never(&dup), 1e9)),
        ("  rep: no drops, all", rep_cfg(never(&top), never(&dup), 1e9)),
        ("  rep: default top n-grams only", rep_cfg(top.clone(), vec![], 1e9)),
        ("  rep: default dup n-grams only", rep_cfg(vec![], dup.clone(), 1e9)),
    ] {
        let mut rep = GopherRepetition::new(cfg);
        time(name, &docs, reps, |d| {
            let _ = rep.filter(d);
        });
    }

    let mut qual = GopherQuality::new(GopherQualityConfig {
        min_doc_words: Some(50.0),
        max_doc_words: Some(100_000.0),
        min_avg_word_length: Some(3.0),
        max_avg_word_length: Some(10.0),
        max_symbol_word_ratio: Some(0.1),
        max_bullet_lines_ratio: Some(0.9),
        max_ellipsis_lines_ratio: Some(0.3),
        max_non_alpha_words_ratio: Some(0.8),
        min_stop_words: Some(2.0),
        stop_words: ["the", "be", "to", "of", "and", "that", "have", "with"].map(String::from).to_vec(),
    });
    time("GopherQuality", &docs, reps, |d| {
        let _ = qual.filter(d);
    });

    let mut c4 = C4::new(C4Config {
        split_paragraph: true,
        remove_citations: true,
        filter_no_terminal_punct: true,
        min_num_sentences: 5,
        min_words_per_line: 3,
        max_word_length: 1000,
        filter_lorem_ipsum: true,
        filter_javascript: true,
        filter_curly_bracket: true,
        filter_policy: true,
    });
    time("C4Quality", &docs, reps, |d| {
        c4.filter(d);
    });

    let mut fw = FineWeb::new(FineWebConfig {
        line_punct_thr: 0.12,
        line_punct_exclude_zero: false,
        stop_chars: [".", "!", "?", "\u{2026}"].map(String::from).to_vec(),
        short_line_thr: 0.67,
        short_line_length: 30,
        char_duplicates_ratio: 0.01,
        new_line_ratio: 0.3,
    });
    time("FineWebQuality (4 stop chars)", &docs, reps, |d| {
        let _ = fw.filter(d);
    });

    let mut simp = Simplifier::new(NormConfig::default());
    time("simplify_text", &docs, reps, |d| {
        simp.simplify(d);
    });
    let mut mh = MinHasher::new(5, NormConfig::default(), HashConfig { hash_fn: HashFn::Xxhash, precision: 64 });
    let mut sh = Vec::new();
    time("MinHash shingles", &docs, reps, |d| mh.shingles(d, &mut sh));
    let a: Vec<u64> = (1..=112u64).map(|i| i.wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 3).collect();
    let b: Vec<u64> = (1..=112u64).map(|i| i.wrapping_mul(0xc2b2_ae3d_27d4_eb4f) >> 3).collect();
    let mut sig = Vec::new();
    time("MinHash shingles + signature", &docs, reps, |d| {
        mh.shingles(d, &mut sh);
        signature(&sh, &a, &b, 64, &mut sig);
    });
}
