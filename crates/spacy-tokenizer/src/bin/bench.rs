//! bench <docs.bin> [--backend split|fancy] [--cache N] [--reps R] [--limit N] [--strings] [--dump out.bin]
//! docs.bin: UTF-8 docs joined by '\0'. --dump writes, per doc, u32 ntok then (u32 idx, u32 len)
//! in chars — the same format as tools/spacy_ref.py dump.

use spacy_tokenizer_rs::{char_offsets, english, Backend, State};
use std::io::Write;
use std::time::Instant;

struct Args {
    path: String,
    backend: Backend,
    cache: usize,
    reps: usize,
    limit: usize,
    strings: bool,
    no_matcher: bool,
    dump: Option<String>,
}

fn parse_args() -> Args {
    let mut a = Args {
        path: String::new(),
        backend: Backend::Split,
        cache: 10_000,
        reps: 5,
        limit: usize::MAX,
        strings: false,
        no_matcher: false,
        dump: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(x) = it.next() {
        let mut val = || it.next().expect("missing value");
        match x.as_str() {
            "--backend" => a.backend = if val() == "fancy" { Backend::Fancy } else { Backend::Split },
            "--cache" => a.cache = val().parse().unwrap(),
            "--reps" => a.reps = val().parse().unwrap(),
            "--limit" => a.limit = val().parse().unwrap(),
            "--strings" => a.strings = true,
            "--no-matcher" => a.no_matcher = true,
            "--dump" => a.dump = Some(val()),
            _ => a.path = x,
        }
    }
    a
}

fn main() {
    let args = parse_args();
    let raw = std::fs::read_to_string(&args.path).expect("read docs");
    let docs: Vec<&str> = raw.split('\0').take(args.limit).collect();
    let mb = docs.iter().map(|d| d.len()).sum::<usize>() as f64 / 1e6;
    let t0 = Instant::now();
    let tok = english(args.backend);
    eprintln!(
        "load {:.0} ms, spaCy rules {}, {} matcher patterns, {} docs, {mb:.1} MB, backend {:?}, cache {}",
        t0.elapsed().as_secs_f64() * 1e3,
        tok.spacy_version,
        tok.matcher_patterns(),
        docs.len(),
        args.backend,
        args.cache
    );

    if let Some(out) = &args.dump {
        let mut st = State::new(args.cache);
        let mut w = std::io::BufWriter::new(std::fs::File::create(out).unwrap());
        let mut offs = Vec::new();
        for d in &docs {
            if args.no_matcher {
                tok.tokenize_without_matcher(d, &mut st);
            } else {
                tok.tokenize(d, &mut st);
            }
            char_offsets(d, &st.tokens, &mut offs);
            w.write_all(&(offs.len() as u32).to_le_bytes()).unwrap();
            for &(i, l) in &offs {
                w.write_all(&i.to_le_bytes()).unwrap();
                w.write_all(&l.to_le_bytes()).unwrap();
            }
        }
        return;
    }

    let mut times = Vec::new();
    let mut ntok = 0usize;
    for rep in 0..args.reps {
        // Fresh state per rep: rep 0 includes cache warm-up, like a cold spaCy process.
        let mut st = State::new(args.cache);
        let t = Instant::now();
        ntok = 0;
        for d in &docs {
            tok.tokenize(d, &mut st);
            if args.strings {
                let v: Vec<String> = st.tokens.iter().map(|&(a, b)| d[a as usize..b as usize].to_string()).collect();
                ntok += v.len();
            } else {
                ntok += st.tokens.len();
            }
        }
        let dt = t.elapsed().as_secs_f64();
        eprintln!("  rep {rep}: {dt:.3}s {:.0} docs/s {:.2} MB/s", docs.len() as f64 / dt, mb / dt);
        times.push(dt);
    }
    times.sort_by(f64::total_cmp);
    let (min, med) = (times[0], times[times.len() / 2]);
    println!(
        "rust backend={:?} cache={} strings={} docs={} tokens={ntok} median {:.0} docs/s {:.2} MB/s | best {:.0} docs/s {:.2} MB/s",
        args.backend,
        args.cache,
        args.strings,
        docs.len(),
        docs.len() as f64 / med,
        mb / med,
        docs.len() as f64 / min,
        mb / min
    );
}
