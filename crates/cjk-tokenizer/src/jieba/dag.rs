//! `get_DAG`, `calc` and `__cut_DAG`: the max-probability path over dictionary words, with runs of single chars
//! handed to the HMM when they don't form a known word.

use super::Jieba;
use crate::Span;

impl Jieba {
    /// `__cut_DAG(text[a..b])`, pushing absolute byte spans.
    pub(super) fn cut_dag(&self, text: &str, a: usize, b: usize, out: &mut Vec<Span>) {
        let s = &text[a..b];
        let pos: Vec<usize> = s.char_indices().map(|(i, _)| i).chain(std::iter::once(s.len())).collect();
        let n = pos.len() - 1;
        let mut route = vec![(0f64, 0usize); n + 1];
        for idx in (0..n).rev() {
            // Python's max over (logprob, x) tuples: ties go to the larger x, which comes later here.
            let mut best = (f64::NEG_INFINITY, 0usize);
            let mut consider = |x: usize, f: u64| {
                let p = (f as f64).ln() - self.log_total + route[x + 1].0;
                if p >= best.0 {
                    best = (p, x);
                }
            };
            let mut any = false;
            for i in idx..n {
                match self.freq(&s[pos[idx]..pos[i + 1]]) {
                    None => break,
                    Some(0) => {}
                    Some(f) => {
                        any = true;
                        consider(i, f);
                    }
                }
            }
            if !any {
                consider(idx, 1);
            }
            route[idx] = best;
        }

        let span = |x: usize, y: usize| ((a + pos[x]) as u32, (a + pos[y]) as u32);
        let mut buf: Option<usize> = None;
        let mut x = 0;
        while x < n {
            let y = route[x].1 + 1;
            if y - x == 1 {
                buf.get_or_insert(x);
            } else {
                if let Some(bs) = buf.take() {
                    self.flush_buf(text, span(bs, x), out);
                }
                out.push(span(x, y));
            }
            x = y;
        }
        if let Some(bs) = buf {
            self.flush_buf(text, span(bs, n), out);
        }
    }

    /// A run of single-char words: one char as is, an unknown word through the HMM, a known word char by char.
    fn flush_buf(&self, text: &str, (a, b): Span, out: &mut Vec<Span>) {
        let buf = &text[a as usize..b as usize];
        let mut chars = buf.char_indices();
        if chars.nth(1).is_none() {
            out.push((a, b));
        } else if self.freq(buf).unwrap_or(0) == 0 {
            self.hmm.cut(buf, a, out);
        } else {
            out.extend(buf.char_indices().map(|(i, c)| (a + i as u32, a + (i + c.len_utf8()) as u32)));
        }
    }
}
