//! Stage 3's output pass: walk nodes in sorted order and emit each input file's `.remove`/`.clusters`/`.sizes`.

/// A node of datatrove's `union_set` with its resolved root.
pub(super) struct Node {
    pub key: u64,
    pub id: u32,
    pub root: u32,
}

/// The bytes datatrove writes for one input file; empty buffers mean the file is not created.
#[derive(Debug, Default, PartialEq)]
pub struct FileOutput {
    pub file: u32,
    pub remove: Vec<u8>,
    pub clusters: Vec<u8>,
    pub sizes: Vec<u8>,
}

/// datatrove `MetricStats` built by the same sequence of `update(x)` calls (Welford, f64 as in Python).
#[derive(Debug, Clone, PartialEq)]
pub struct Metric {
    pub label: &'static str,
    pub unit: Option<&'static str>,
    pub total: u64,
    pub n: u64,
    pub mean: f64,
    pub min: u64,
    pub max: u64,
    pub running_variance: f64,
}

impl Metric {
    fn update(&mut self, x: u64) {
        self.total += x;
        self.n += 1;
        self.min = self.min.min(x);
        self.max = self.max.max(x);
        let delta = x as f64 - self.mean;
        self.mean += delta / self.n as f64;
        if self.n > 1 {
            self.running_variance += delta * (x as f64 - self.mean);
        }
    }
}

const DUPLICATES: usize = 0;
const TO_REMOVE: usize = 1;
const CLUSTERS: usize = 2;
const CLUSTER_SIZE: usize = 3;

pub struct Outputs {
    nodes: Vec<Node>,
    size: Vec<u32>,
    cluster_id: Vec<u32>,
    next_cluster: u32,
    save_cluster_size: bool,
    pos: usize,
    metrics: [Metric; 4],
    /// Metric indices in order of first update: the key order of datatrove's stats dict.
    order: Vec<usize>,
}

/// The root of a single-node set reached only through `(x, x)` pairs has no `set_size` entry; datatrove raises
/// `KeyError` on it.
#[derive(Debug, PartialEq)]
pub struct MissingSize(pub u64);

impl Outputs {
    pub(super) fn new(nodes: Vec<Node>, size: Vec<u32>, save_cluster_id: bool, save_cluster_size: bool) -> Self {
        let metric = |label, unit| Metric {
            label,
            unit,
            total: 0,
            n: 0,
            mean: 0.0,
            min: u64::MAX,
            max: 0,
            running_variance: 0.0,
        };
        Self {
            cluster_id: if save_cluster_id { vec![u32::MAX; size.len()] } else { Vec::new() },
            nodes,
            size,
            next_cluster: 0,
            save_cluster_size,
            pos: 0,
            metrics: [
                metric("duplicates", None),
                metric("to_remove", None),
                metric("clusters", None),
                metric("cluster_size", Some("cluster")),
            ],
            order: Vec::new(),
        }
    }

    fn stat(&mut self, i: usize, x: u64) {
        if self.metrics[i].n == 0 {
            self.order.push(i);
        }
        self.metrics[i].update(x);
    }

    fn set_size(&self, key: u64, root: u32) -> Result<u32, MissingSize> {
        match self.size[root as usize] {
            0 => Err(MissingSize(key)),
            s => Ok(s),
        }
    }

    /// The next input file's outputs, in ascending file order.
    pub fn next_file(&mut self) -> Result<Option<FileOutput>, MissingSize> {
        let Some(first) = self.nodes.get(self.pos) else { return Ok(None) };
        let file = (first.key >> 32) as u32;
        let mut out = FileOutput { file, ..Default::default() };
        while let Some(node) = self.nodes.get(self.pos) {
            if (node.key >> 32) as u32 != file {
                break;
            }
            let (key, id, root) = (node.key, node.id, node.root);
            let doc = (key as u32).to_le_bytes();
            self.stat(DUPLICATES, 1);
            if id != root {
                out.remove.extend_from_slice(&doc);
                self.stat(TO_REMOVE, 1);
            }
            if !self.cluster_id.is_empty() {
                if self.cluster_id[root as usize] == u32::MAX {
                    self.cluster_id[root as usize] = self.next_cluster;
                    self.next_cluster += 1;
                    self.stat(CLUSTERS, 1);
                }
                out.clusters.extend_from_slice(&doc);
                out.clusters.extend_from_slice(&self.cluster_id[root as usize].to_le_bytes());
            }
            if id == root {
                let s = self.set_size(key, root)?;
                self.stat(CLUSTER_SIZE, s as u64);
            }
            if self.save_cluster_size {
                let s = self.set_size(key, root)?;
                out.sizes.extend_from_slice(&doc);
                out.sizes.extend_from_slice(&s.to_le_bytes());
            }
            self.pos += 1;
        }
        Ok(Some(out))
    }

    /// Metrics that datatrove would have created, in its stats-dict order.
    pub fn metrics(&self) -> Vec<Metric> {
        self.order.iter().map(|&i| self.metrics[i].clone()).collect()
    }
}
