//! pythainlp `newmm.segment(text, safe_mode=True)` on char offsets; yields token end offsets.

use super::tcc::valid_positions;
use super::trie::Trie;
use crate::pyunicode::py;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

const MAX_GRAPH_SIZE: usize = 50;
const SCAN_BEGIN: usize = 100;
const SCAN_END: usize = 140;

/// Token boundaries (exclusive char ends, ascending) of `segment(text, safe_mode=True)`.
pub fn segment(text: &[char], dict: &Trie, ends: &mut Vec<usize>) {
    ends.clear();
    if text.len() < SCAN_END {
        onecut(text, 0, dict, ends);
        return;
    }
    let mut start = 0;
    while text.len() - start >= SCAN_END {
        let cut = start + find_cut_pos(&text[start..], dict);
        onecut(&text[start..cut], start, dict, ends);
        start = cut;
    }
    if start < text.len() {
        onecut(&text[start..], start, dict, ends);
    }
}

fn find_cut_pos(text: &[char], dict: &Trie) -> usize {
    let sample = &text[SCAN_BEGIN..SCAN_END];
    if let Some(i) = sample.iter().rposition(|&c| c == ' ') {
        return SCAN_BEGIN + i + 1;
    }
    let mut ends = Vec::new();
    onecut(sample, 0, dict, &mut ends);
    let (mut max_idx, mut max_len, mut prev) = (0, 0, 0);
    for (i, &e) in ends.iter().enumerate() {
        if e - prev >= max_len {
            max_len = e - prev;
            max_idx = i;
        }
        prev = e;
    }
    SCAN_BEGIN + if max_idx == 0 { 0 } else { ends[max_idx - 1] }
}

fn onecut(text: &[char], offset: usize, dict: &Trie, out: &mut Vec<usize>) {
    let s: String = text.iter().collect();
    let mut valid = Vec::new();
    valid_positions(&s, &mut valid);
    let n = text.len();
    let mut graph: FxHashMap<usize, Vec<usize>> = FxHashMap::default();
    let mut graph_size = 0;
    let mut heap = BinaryHeap::from([Reverse(0usize)]);
    let mut in_heap = FxHashSet::from_iter([0usize]);
    let mut end_pos = 0;
    let mut words = Vec::new();
    while heap.peek().is_some_and(|Reverse(p)| *p < n) {
        let Reverse(begin) = heap.pop().expect("non-empty");
        in_heap.remove(&begin);
        dict.prefixes(text, begin, &mut words);
        for &len in &words {
            let cand = begin + len;
            if valid[cand] {
                graph.entry(begin).or_default().push(cand);
                graph_size += 1;
                if in_heap.insert(cand) {
                    heap.push(Reverse(cand));
                }
                if graph_size > MAX_GRAPH_SIZE {
                    break;
                }
            }
        }
        match heap.len() {
            1 => {
                let goal = heap.peek().expect("one").0;
                for pos in bfs_path(&graph, end_pos, goal).into_iter().skip(1) {
                    out.push(offset + pos);
                    end_pos = pos;
                }
                graph_size = 0;
                graph.clear();
            }
            0 => {
                end_pos = find_skip_end(text, begin, &valid, dict);
                graph_size = 0;
                graph.clear();
                out.push(offset + end_pos);
                in_heap.insert(end_pos);
                heap.push(Reverse(end_pos));
            }
            _ => {}
        }
    }
}

/// The first path `_bfs_paths_graph` yields.
fn bfs_path(graph: &FxHashMap<usize, Vec<usize>>, start: usize, goal: usize) -> Vec<usize> {
    let mut visited = FxHashSet::from_iter([start]);
    let mut queue = VecDeque::from([(start, vec![start])]);
    while let Some((vertex, path)) = queue.pop_front() {
        for &pos in graph.get(&vertex).map(Vec::as_slice).unwrap_or(&[]) {
            if pos == goal {
                let mut p = path;
                p.push(pos);
                return p;
            } else if visited.insert(pos) {
                let mut p = path.clone();
                p.push(pos);
                queue.push_back((pos, p));
            }
        }
    }
    panic!("newmm: no path from {start} to {goal}")
}

fn find_skip_end(text: &[char], begin: usize, valid: &[bool], dict: &Trie) -> usize {
    if let Some(end) = nonthai_match(text, begin) {
        return end;
    }
    let mut words = Vec::new();
    for pos in begin + 1..text.len() {
        if valid[pos] {
            dict.prefixes(text, pos, &mut words);
            let long = |&len: &usize| valid[pos + len] && !two_thai_consonants(&text[pos..pos + len]);
            if words.iter().any(long) || nonthai_match(text, pos).is_some() {
                return pos;
            }
        }
    }
    text.len()
}

/// `_PAT_THAI_TWOCHARS.match(word)`: `[ก-ฮ]{,2}$`.
fn two_thai_consonants(word: &[char]) -> bool {
    word.len() <= 2 && word.iter().all(|c| ('ก'..='ฮ').contains(c))
}

/// End of `_PAT_NONTHAI.match(text, pos)`:
/// `[-a-zA-Z]+|\d+([,\.]\d+)*|[ \t]+|\r?\n|[^฀-๿ \t\r\n]+`.
fn nonthai_match(text: &[char], pos: usize) -> Option<usize> {
    let run = |from: usize, f: &dyn Fn(char) -> bool| from + text[from..].iter().take_while(|&&c| f(c)).count();
    let d = |c: char| py().re_d.contains(c);
    let c = *text.get(pos)?;
    if c == '-' || c.is_ascii_alphabetic() {
        return Some(run(pos, &|c| c == '-' || c.is_ascii_alphabetic()));
    }
    if d(c) {
        let mut end = run(pos, &d);
        while matches!(text.get(end), Some(',' | '.')) && text.get(end + 1).is_some_and(|&c| d(c)) {
            end = run(end + 1, &d);
        }
        return Some(end);
    }
    if c == ' ' || c == '\t' {
        return Some(run(pos, &|c| c == ' ' || c == '\t'));
    }
    if c == '\n' {
        return Some(pos + 1);
    }
    if c == '\r' {
        return (text.get(pos + 1) == Some(&'\n')).then_some(pos + 2);
    }
    let other = |c: char| !matches!(c, '\u{E00}'..='\u{E7F}' | ' ' | '\t' | '\r' | '\n');
    other(c).then(|| run(pos, &other))
}
