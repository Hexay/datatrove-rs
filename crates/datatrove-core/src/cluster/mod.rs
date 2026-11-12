//! MinhashDedupCluster (MinHash stage 3): union-find over `.dups` pairs and the per-file outputs.
//!
//! Reproduces datatrove's decisions exactly: union by size, the larger set's root wins, ties go to the root of the
//! pair's first node, and the index SENTINEL node always becomes the root. Nodes are `file << 32 | doc`, so sorting
//! the keys gives datatrove's `sorted(union_set)` order. Path compression changes no root, so any `find` will do.

mod output;
#[cfg(test)]
mod tests;

pub use output::{FileOutput, Metric, MissingSize, Outputs};

use output::Node;

use hashbrown::HashMap;
use rustc_hash::FxBuildHasher;

pub const SENTINEL: u64 = u64::MAX;
const PAIR: usize = 16;

#[derive(Debug, PartialEq)]
pub enum ClusterError {
    /// A `.dups` file whose length is not a multiple of 16 (Python's `struct.error`).
    TrailingBytes(usize),
}

pub struct UnionFind {
    index: HashMap<u64, u32, FxBuildHasher>,
    parent: Vec<u32>,
    /// datatrove's `set_size`: 0 = no entry (`set_size.get(root, 1)` reads it as 1).
    size: Vec<u32>,
    sentinel: Option<u32>,
    ignore_index_matches: bool,
    carry: Vec<u8>,
}

impl UnionFind {
    pub fn new(ignore_index_matches: bool) -> Self {
        Self {
            index: HashMap::with_hasher(FxBuildHasher),
            parent: Vec::new(),
            size: Vec::new(),
            sentinel: None,
            ignore_index_matches,
            carry: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.parent.len()
    }

    pub fn is_empty(&self) -> bool {
        self.parent.is_empty()
    }

    /// Feed the next bytes of a `.dups` file (`<4I` records); records may straddle calls.
    pub fn feed(&mut self, mut bytes: &[u8]) {
        if !self.carry.is_empty() {
            let need = (PAIR - self.carry.len()).min(bytes.len());
            self.carry.extend_from_slice(&bytes[..need]);
            bytes = &bytes[need..];
            if self.carry.len() < PAIR {
                return;
            }
            let rec: [u8; PAIR] = self.carry[..].try_into().unwrap();
            self.carry.clear();
            self.pair(&rec);
        }
        let (recs, rest) = bytes.as_chunks::<PAIR>();
        for rec in recs {
            self.pair(rec);
        }
        self.carry.extend_from_slice(rest);
    }

    /// Call at the end of each file.
    pub fn end_file(&mut self) -> Result<(), ClusterError> {
        let n = self.carry.len();
        self.carry.clear();
        if n == 0 {
            Ok(())
        } else {
            Err(ClusterError::TrailingBytes(n))
        }
    }

    fn pair(&mut self, rec: &[u8; PAIR]) {
        let u = |i: usize| u32::from_le_bytes(rec[i..i + 4].try_into().unwrap()) as u64;
        let (a, b) = (u(0) << 32 | u(4), u(8) << 32 | u(12));
        if self.ignore_index_matches && a == SENTINEL {
            return;
        }
        self.union(a, b);
    }

    fn node(&mut self, key: u64) -> u32 {
        let next = self.parent.len() as u32;
        let id = *self.index.entry(key).or_insert(next);
        if id == next {
            self.parent.push(next);
            self.size.push(0);
            if key == SENTINEL {
                self.sentinel = Some(id);
            }
        }
        id
    }

    fn find(&mut self, mut x: u32) -> u32 {
        while self.parent[x as usize] != x {
            let gp = self.parent[self.parent[x as usize] as usize];
            self.parent[x as usize] = gp;
            x = gp;
        }
        x
    }

    pub fn union(&mut self, a: u64, b: u64) {
        let (na, nb) = (self.node(a), self.node(b));
        let (mut ra, mut rb) = (self.find(na), self.find(nb));
        if ra == rb {
            return;
        }
        let get = |s: u32| if s == 0 { 1 } else { s };
        let (sa, sb) = (get(self.size[ra as usize]), get(self.size[rb as usize]));
        if Some(rb) == self.sentinel || (Some(ra) != self.sentinel && sa < sb) {
            std::mem::swap(&mut ra, &mut rb);
        }
        self.parent[rb as usize] = ra;
        self.size[ra as usize] = sa + sb;
        self.size[rb as usize] = 0;
    }

    /// Resolve every node's root and sort nodes; frees the hash index first.
    pub fn finish(mut self, save_cluster_id: bool, save_cluster_size: bool) -> Outputs {
        let index = std::mem::take(&mut self.index);
        let mut nodes = Vec::with_capacity(index.len());
        for (key, id) in index {
            nodes.push(Node { key, id, root: self.find(id) });
        }
        nodes.sort_unstable_by_key(|n| n.key);
        drop(std::mem::take(&mut self.parent));
        Outputs::new(nodes, self.size, save_cluster_id, save_cluster_size)
    }
}
