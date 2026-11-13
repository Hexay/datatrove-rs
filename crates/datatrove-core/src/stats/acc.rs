//! `BaseStats.run`'s bookkeeping: per group, per stat name, a `MetricStatsDict` of key -> `MetricStats`, each
//! built only by `counters[stat][key] += value`.

use super::pyfloat::histogram_key;
use super::Num;
use rustc_hash::FxHashMap;

/// `MetricStats.total`: Python int arithmetic stays exact until a float is added.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Total {
    Int(i128),
    Float(f64),
}

impl Total {
    /// `float(total)`: `as` rounds to nearest-even like `int.__float__`.
    fn f(self) -> f64 {
        match self {
            Total::Int(t) => t as f64,
            Total::Float(t) => t,
        }
    }
}

/// A `MetricStats` that only ever had `+= value` applied; its unit is then always "task".
#[derive(Debug, Clone, Copy)]
pub struct Row {
    pub total: Total,
    pub n: u64,
    pub mean: f64,
    pub min: Num,
    pub max: Num,
    pub running_variance: f64,
}

impl Default for Row {
    fn default() -> Self {
        let inf = f64::INFINITY;
        Self {
            total: Total::Int(0),
            n: 0,
            mean: 0.0,
            min: Num::Float(inf),
            max: Num::Float(-inf),
            running_variance: 0.0,
        }
    }
}

impl Row {
    /// `self + MetricStats.from_dict(x)` (total=x, n=1, mean=x, min=x, max=x), in `MetricStats.__add__`'s
    /// operation order; Python's int*float and int+float convert the int first, exactly here (|ints| <= 2**53).
    pub fn add(&mut self, x: Num) {
        let (n0, xf) = (self.n as f64, x.f());
        let n = (self.n + 1) as f64;
        let mean = (n0 * self.mean + xf) / n;
        let delta = self.mean - xf;
        self.running_variance = (self.running_variance + 0.0) + delta * delta * n0 / n;
        self.mean = mean;
        self.n += 1;
        self.total = match (self.total, x) {
            (Total::Int(t), Num::Int(v)) => Total::Int(t + v as i128),
            (t, x) => Total::Float(t.f() + x.f()),
        };
        // max(a, b) / min(a, b) keep `a` unless `b` is strictly greater / smaller
        if x.gt(self.max) {
            self.max = x;
        }
        if self.min.gt(x) {
            self.min = x;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupKind {
    Summary,
    Histogram,
    Fqdn,
    Suffix,
}

impl GroupKind {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "summary" => Self::Summary,
            "histogram" => Self::Histogram,
            "fqdn" => Self::Fqdn,
            "suffix" => Self::Suffix,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Summary => "summary",
            Self::Histogram => "histogram",
            Self::Fqdn => "fqdn",
            Self::Suffix => "suffix",
        }
    }
}

/// One `MetricStatsDict`, keys in insertion order.
#[derive(Debug, Default)]
pub struct Table {
    pub name: String,
    index: FxHashMap<Box<str>, usize>,
    pub rows: Vec<(Box<str>, Row)>,
}

impl Table {
    fn add(&mut self, key: &str, x: Num) {
        let i = match self.index.get(key) {
            Some(&i) => i,
            None => {
                self.rows.push((key.into(), Row::default()));
                self.index.insert(key.into(), self.rows.len() - 1);
                self.rows.len() - 1
            }
        };
        self.rows[i].1.add(x);
    }
}

/// A group's `defaultdict(MetricStatsDict)`, tables in insertion order.
#[derive(Debug)]
pub struct Group {
    pub kind: GroupKind,
    pub tables: Vec<Table>,
    /// Per stat id: the table of the stat itself, `<stat>__chars`, `<stat>__tokens`.
    slots: Vec<[Option<usize>; 3]>,
}

impl Group {
    fn table(&mut self, stat: usize, name: &str, suffix: usize) -> &mut Table {
        if self.slots.len() <= stat {
            self.slots.resize(stat + 1, [None; 3]);
        }
        let i = *self.slots[stat][suffix].get_or_insert_with(|| {
            let name = match suffix {
                0 => name.to_owned(),
                1 => format!("{name}__chars"),
                _ => format!("{name}__tokens"),
            };
            self.tables.push(Table { name, ..Default::default() });
            self.tables.len() - 1
        });
        &mut self.tables[i]
    }
}

/// What `get_kv` reads from the doc besides the value.
pub struct DocKeys<'a> {
    pub fqdn: Option<&'a str>,
    pub suffix: Option<&'a str>,
    /// `len(doc.text)`.
    pub chars: i64,
    /// `doc.metadata["token_count"]` when present.
    pub tokens: Option<Num>,
}

pub struct Accumulator {
    stat_ids: FxHashMap<Box<str>, usize>,
    pub groups: Vec<Group>,
    digits: usize,
    key: String,
}

impl Accumulator {
    /// `groups` in `self.groups` order without duplicates; `digits` is `histogram_round_digits`.
    pub fn new(groups: &[GroupKind], digits: usize) -> Self {
        let groups = groups.iter().map(|&kind| Group { kind, tables: Vec::new(), slots: Vec::new() }).collect();
        Self { stat_ids: FxHashMap::default(), groups, digits, key: String::new() }
    }

    /// True when every group can key this doc (fqdn/suffix present for their groups).
    pub fn accepts(&self, doc: &DocKeys) -> bool {
        self.groups.iter().all(|g| match g.kind {
            GroupKind::Fqdn => doc.fqdn.is_some(),
            GroupKind::Suffix => doc.suffix.is_some(),
            _ => true,
        })
    }

    /// The `for group ...: for stat, value in doc_stats.items()` loop of `BaseStats.run` for one doc.
    /// Panics unless `accepts(doc)`.
    pub fn add(&mut self, stats: &[(&str, Num)], doc: &DocKeys) {
        let ids: Vec<usize> = stats.iter().map(|(name, _)| self.stat_id(name)).collect();
        for g in &mut self.groups {
            for (&id, &(name, value)) in ids.iter().zip(stats) {
                match g.kind {
                    GroupKind::Summary => g.table(id, name, 0).add("summary", value),
                    GroupKind::Fqdn => g.table(id, name, 0).add(doc.fqdn.expect("accepts"), value),
                    GroupKind::Suffix => g.table(id, name, 0).add(doc.suffix.expect("accepts"), value),
                    GroupKind::Histogram => {
                        histogram_key(value, self.digits, &mut self.key);
                        g.table(id, name, 0).add(&self.key, Num::Int(1));
                        g.table(id, name, 1).add(&self.key, Num::Int(doc.chars));
                        if let Some(tokens) = doc.tokens {
                            g.table(id, name, 2).add(&self.key, tokens);
                        }
                    }
                }
            }
        }
    }

    fn stat_id(&mut self, name: &str) -> usize {
        if let Some(&id) = self.stat_ids.get(name) {
            return id;
        }
        let id = self.stat_ids.len();
        self.stat_ids.insert(name.into(), id);
        id
    }
}
