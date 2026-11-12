//! cjkdump <docs.bin> <out.bin> zh <dict.txt> <hmm.json>
//! cjkdump <docs.bin> <out.bin> ja <settings.json> <resource_dir> <system.dic>
//! Per doc: u32 ntok (u32::MAX where datatrove raises), then (u32 idx, u32 len) in chars, the format of
//! `tools/cjk_ref.py dump` and spacy-tokenizer's `compare.py`.

use cjk_tokenizer_rs::japanese::Sudachi;
use cjk_tokenizer_rs::jieba::{Hmm, Jieba};
use cjk_tokenizer_rs::Span;
use std::io::Write;
use std::path::Path;
use std::time::Instant;

fn write_doc(out: &mut impl Write, text: &str, spans: Option<&[Span]>) {
    let Some(spans) = spans else {
        out.write_all(&u32::MAX.to_le_bytes()).unwrap();
        return;
    };
    out.write_all(&(spans.len() as u32).to_le_bytes()).unwrap();
    let (mut byte, mut chars) = (0usize, 0u32);
    for &(a, b) in spans {
        chars += text[byte..a as usize].chars().count() as u32;
        let len = text[a as usize..b as usize].chars().count() as u32;
        out.write_all(&chars.to_le_bytes()).unwrap();
        out.write_all(&len.to_le_bytes()).unwrap();
        chars += len;
        byte = b as usize;
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read_to_string(&a[1]).expect("docs.bin");
    let mut out = std::io::BufWriter::new(std::fs::File::create(&a[2]).expect("out"));
    let docs: Vec<&str> = data.split('\0').collect();
    let (mut tokens, mut words, mut chunks) = (Vec::new(), Vec::new(), Vec::new());
    let t = Instant::now();
    let mut results: Vec<Option<Vec<Span>>> = Vec::with_capacity(docs.len());
    match a[3].as_str() {
        "zh" => {
            let dict = std::fs::read_to_string(&a[4]).expect("dict.txt");
            let (freq, total) = Jieba::parse_dict(&dict).unwrap();
            let hmm = Hmm::from_json(&std::fs::read_to_string(&a[5]).expect("hmm.json")).unwrap();
            let jieba = Jieba::new(freq, total, hmm).unwrap();
            eprintln!("loaded in {:.2}s", t.elapsed().as_secs_f64());
            let t = Instant::now();
            for d in &docs {
                tokens.clear();
                jieba.tokenize(d, 0, &mut words, &mut tokens);
                results.push(Some(tokens.clone()));
            }
            eprintln!("{} docs in {:.2}s", docs.len(), t.elapsed().as_secs_f64());
        }
        "ja" => {
            let sudachi = Sudachi::new(Path::new(&a[4]), Path::new(&a[5]), Path::new(&a[6])).unwrap();
            let mut st = sudachi.state();
            eprintln!("loaded in {:.2}s", t.elapsed().as_secs_f64());
            let t = Instant::now();
            for d in &docs {
                let ok = sudachi.tokenize(d, &mut st, &mut tokens, &mut chunks).is_ok();
                results.push(ok.then(|| tokens.clone()));
            }
            eprintln!("{} docs in {:.2}s", docs.len(), t.elapsed().as_secs_f64());
        }
        lang => panic!("unknown language {lang}"),
    }
    for (d, r) in docs.iter().zip(&results) {
        write_doc(&mut out, d, r.as_deref());
    }
}
