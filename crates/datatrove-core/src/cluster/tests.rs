use super::*;
use std::collections::BTreeMap;

/// datatrove's dict-based union-find, transcribed literally.
fn reference(pairs: &[(u64, u64)]) -> (BTreeMap<u64, u64>, BTreeMap<u64, u32>) {
    let mut parent: BTreeMap<u64, u64> = BTreeMap::new();
    let mut size: BTreeMap<u64, u32> = BTreeMap::new();
    fn find(p: &mut BTreeMap<u64, u64>, x: u64) -> u64 {
        match p.get(&x) {
            Some(&y) if y != x => {
                let r = find(p, y);
                p.insert(x, r);
                r
            }
            _ => {
                p.insert(x, x);
                x
            }
        }
    }
    for &(a, b) in pairs {
        let (mut ra, mut rb) = (find(&mut parent, a), find(&mut parent, b));
        if ra == rb {
            continue;
        }
        let sa = *size.get(&ra).unwrap_or(&1);
        let sb = *size.get(&rb).unwrap_or(&1);
        if rb == SENTINEL || (ra != SENTINEL && sa < sb) {
            std::mem::swap(&mut ra, &mut rb);
        }
        parent.insert(rb, ra);
        size.insert(ra, sa + sb);
        size.remove(&rb);
    }
    let keys: Vec<u64> = parent.keys().copied().collect();
    let roots = keys.into_iter().map(|k| (k, find(&mut parent, k))).collect();
    (roots, size)
}

fn encode(pairs: &[(u64, u64)]) -> Vec<u8> {
    let mut out = Vec::new();
    for &(a, b) in pairs {
        for x in [a >> 32, a & 0xffff_ffff, b >> 32, b & 0xffff_ffff] {
            out.extend_from_slice(&(x as u32).to_le_bytes());
        }
    }
    out
}

fn le(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

/// Expected outputs from the reference, following datatrove's output loop.
fn expected(pairs: &[(u64, u64)]) -> Vec<FileOutput> {
    let (roots, size) = reference(pairs);
    let mut files: BTreeMap<u32, FileOutput> = BTreeMap::new();
    let mut ids: BTreeMap<u64, u32> = BTreeMap::new();
    for (&node, &root) in &roots {
        let file = (node >> 32) as u32;
        let doc = node as u32;
        let out = files.entry(file).or_insert_with(|| FileOutput { file, ..Default::default() });
        if node != root {
            out.remove.extend(le(&[doc]));
        }
        let next = ids.len() as u32;
        let ci = *ids.entry(root).or_insert(next);
        out.clusters.extend(le(&[doc, ci]));
        out.sizes.extend(le(&[doc, size[&root]]));
    }
    files.into_values().collect()
}

fn run(pairs: &[(u64, u64)], chunk: usize) -> (Vec<FileOutput>, Vec<Metric>) {
    let mut uf = UnionFind::new(false);
    for c in encode(pairs).chunks(chunk) {
        uf.feed(c);
    }
    uf.end_file().unwrap();
    let mut out = uf.finish(true, true);
    let mut files = Vec::new();
    while let Some(f) = out.next_file().unwrap() {
        files.push(f);
    }
    (files, out.metrics())
}

fn lcg_pairs(seed: u64, n: usize, files: u64, docs: u64, sentinel_every: usize) -> Vec<(u64, u64)> {
    let mut s = seed;
    let mut next = || {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        s >> 33
    };
    (0..n)
        .map(|i| {
            let mut node = || ((next() % files) << 32) | (next() % docs);
            let a = if sentinel_every > 0 && i % sentinel_every == 0 { SENTINEL } else { node() };
            (a, node())
        })
        .filter(|(a, b)| a != b)
        .collect()
}

#[test]
fn matches_reference_on_random_graphs() {
    for (seed, files, docs, sentinel) in [(1, 3, 40, 0), (2, 5, 200, 7), (3, 1, 30, 3), (4, 50, 5000, 0)] {
        let pairs = lcg_pairs(seed, 3000, files, docs, sentinel);
        for chunk in [7, 16, 4096] {
            assert_eq!(run(&pairs, chunk).0, expected(&pairs), "seed {seed} chunk {chunk}");
        }
    }
}

#[test]
fn ties_go_to_the_first_node_and_sentinel_always_wins() {
    let (a, b, c) = (1 << 32 | 5, 7, 2 << 32);
    let (files, _) = run(&[(a, b)], 16);
    assert_eq!(files[0].remove, le(&[7]));
    let (files, _) = run(&[(b, a), (c, SENTINEL)], 16);
    assert_eq!(files.iter().map(|f| f.file).collect::<Vec<_>>(), [0, 1, 2, u32::MAX]);
    assert_eq!(files[1].remove, le(&[5]));
    assert_eq!(files[2].remove, le(&[0]));
    assert!(files[3].remove.is_empty());
}

#[test]
fn metrics_follow_first_update_order() {
    let (_, m) = run(&[(1 << 32, 3), (1 << 32, 4)], 16);
    let labels: Vec<_> = m.iter().map(|m| m.label).collect();
    assert_eq!(labels, ["duplicates", "to_remove", "clusters", "cluster_size"]);
    assert_eq!((m[0].n, m[1].n, m[2].n, m[3].total, m[3].min), (3, 2, 1, 3, 3));
}

#[test]
fn errors_like_datatrove() {
    let mut uf = UnionFind::new(false);
    uf.feed(&[0; 20]);
    assert_eq!(uf.end_file(), Err(ClusterError::TrailingBytes(4)));
    let mut uf = UnionFind::new(false);
    uf.union(9, 9);
    assert_eq!(uf.finish(false, false).next_file(), Err(MissingSize(9)));
}

#[test]
fn ignore_index_matches_skips_sentinel_pairs() {
    let mut uf = UnionFind::new(true);
    uf.feed(&encode(&[(SENTINEL, 3), (4, 5)]));
    assert_eq!(uf.len(), 2);
}
