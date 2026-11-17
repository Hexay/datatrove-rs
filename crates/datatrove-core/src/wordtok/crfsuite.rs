//! crfsuite's CRF1d tagger (`crf1d_model.c`, `crf1d_tag.c`, `crf1d_context.c`), bit-exact: the binary model, state
//! scores summed in the same order, and its Viterbi with the same tie-breaks.

use rustc_hash::FxHashMap;

/// An attribute's state features as (label, weight).
type StateFeatures = Box<[(u32, f64)]>;

/// A `lCRF`/`FOMC` model file.
pub struct Model {
    pub labels: Vec<String>,
    /// `trans[i * L + j]`: the score of label i followed by label j.
    trans: Vec<f64>,
    /// Attribute string -> its state features, in the model's attribute-reference order.
    attrs: FxHashMap<Box<[u8]>, StateFeatures>,
}

struct Buf<'a>(&'a [u8]);

impl Buf<'_> {
    fn u32(&self, at: usize) -> Result<u32, String> {
        let b = self.0.get(at..at + 4).ok_or("crfsuite model: truncated")?;
        Ok(u32::from_le_bytes(b.try_into().expect("4 bytes")))
    }

    fn at(&self, at: usize) -> Result<usize, String> {
        self.u32(at).map(|v| v as usize)
    }

    fn chunk(&self, at: usize, id: &[u8; 4]) -> Result<usize, String> {
        if self.0.get(at..at + 4) != Some(id) {
            return Err(format!("crfsuite model: no {} chunk", String::from_utf8_lossy(id)));
        }
        self.at(at + 8)
    }

    /// (label, weight) of feature `fid` with its type and source.
    fn feature(&self, off: usize, fid: usize) -> Result<(u32, u32, u32, f64), String> {
        let p = off + 12 + 20 * fid;
        let w = self.0.get(p + 12..p + 20).ok_or("crfsuite model: truncated")?;
        Ok((self.u32(p)?, self.u32(p + 4)?, self.u32(p + 8)?, f64::from_le_bytes(w.try_into().expect("8 bytes"))))
    }

    /// The feature ids of reference list `i` in the `LFRF`/`AFRF` chunk at `off`.
    fn refs(&self, off: usize, i: usize) -> Result<Vec<usize>, String> {
        let list = self.at(off + 12 + 4 * i)?;
        (0..self.at(list)?).map(|r| self.at(list + 4 + 4 * r)).collect()
    }

    /// Id -> key of the CQDB chunk at `off`, from its backward array (keys are NUL-terminated).
    fn cqdb(&self, off: usize) -> Result<Vec<Box<[u8]>>, String> {
        self.chunk(off, b"CQDB")?;
        let (n, bwd) = (self.at(off + 16)?, self.at(off + 20)?);
        (0..n)
            .map(|i| {
                let rec = off + self.at(off + bwd + 4 * i)?;
                let size = self.at(rec + 4)?;
                let key = self.0.get(rec + 8..rec + 8 + size.saturating_sub(1)).ok_or("crfsuite model: truncated")?;
                Ok(key.into())
            })
            .collect()
    }
}

impl Model {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let b = Buf(bytes);
        if bytes.get(..4) != Some(b"lCRF") || bytes.get(8..12) != Some(b"FOMC") {
            return Err("not a crfsuite CRF1d model".into());
        }
        let [n_labels, n_attrs, feat, off_labels, off_attrs, label_refs, attr_refs] =
            [20, 24, 28, 32, 36, 40, 44].map(|at| b.at(at));
        let (n_labels, n_attrs, feat) = (n_labels?, n_attrs?, feat?);
        let n_feat = b.chunk(feat, b"FEAT")?;
        let feature = |fid: usize| if fid < n_feat { b.feature(feat, fid) } else { Err("bad feature id".into()) };
        let labels = b.cqdb(off_labels?)?;
        let keys = b.cqdb(off_attrs?)?;
        if labels.len() != n_labels || keys.len() != n_attrs {
            return Err("crfsuite model: dictionary sizes don't match the header".into());
        }
        let (label_refs, attr_refs) = (label_refs?, attr_refs?);
        b.chunk(label_refs, b"LFRF")?;
        b.chunk(attr_refs, b"AFRF")?;
        let mut trans = vec![0.0; n_labels * n_labels];
        for i in 0..n_labels {
            for fid in b.refs(label_refs, i)? {
                let (_, _, dst, w) = feature(fid)?;
                *trans.get_mut(i * n_labels + dst as usize).ok_or("bad label")? = w;
            }
        }
        let mut attrs = FxHashMap::default();
        for (a, key) in keys.into_iter().enumerate() {
            let feats: Result<Box<[(u32, f64)]>, String> =
                b.refs(attr_refs, a)?.into_iter().map(|fid| feature(fid).map(|(_, _, dst, w)| (dst, w))).collect();
            let feats = feats?;
            if feats.iter().any(|&(dst, _)| dst as usize >= n_labels) {
                return Err("bad label".into());
            }
            attrs.insert(key, feats);
        }
        let labels = labels.into_iter().map(|l| String::from_utf8_lossy(&l).into_owned()).collect();
        Ok(Self { labels, trans, attrs })
    }

    pub fn label(&self, name: &str) -> Option<u32> {
        self.labels.iter().position(|l| l == name).map(|i| i as u32)
    }

    /// Adds attribute `key`'s state weights to one item's scores (`crf1dt_state_score` with value 1.0, which
    /// pycrfsuite gives every string attribute). The key is looked up as a C string, so it ends at a NUL.
    pub fn add_state(&self, key: &str, scores: &mut [f64]) {
        let key = key.as_bytes();
        let key = key.iter().position(|&c| c == 0).map_or(key, |n| &key[..n]);
        for &(dst, w) in self.attrs.get(key).map_or(&[][..], |f| f) {
            scores[dst as usize] += w;
        }
    }

    /// `crf1dc_viterbi` over `state` (T items x L labels, row-major) with this model's transitions.
    pub fn viterbi(&self, state: &[f64]) -> Vec<u32> {
        viterbi(state, &self.trans, self.labels.len())
    }
}

/// `crf1dc_viterbi`: strict `<` keeps the first best label on ties.
pub fn viterbi(state: &[f64], trans: &[f64], l: usize) -> Vec<u32> {
    let n = state.len() / l;
    if n == 0 {
        return Vec::new();
    }
    let mut back = vec![0u32; n * l];
    let mut prev = state[..l].to_vec();
    let mut curr = vec![0.0; l];
    for t in 1..n {
        for j in 0..l {
            let (mut best, mut arg) = (-f64::MAX, 0);
            for (i, p) in prev.iter().enumerate() {
                let score = p + trans[i * l + j];
                if best < score {
                    best = score;
                    arg = i as u32;
                }
            }
            back[t * l + j] = arg;
            curr[j] = best + state[t * l + j];
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    let mut labels = vec![0u32; n];
    let mut best = -f64::MAX;
    for (i, &p) in prev.iter().enumerate() {
        if best < p {
            best = p;
            labels[n - 1] = i as u32;
        }
    }
    for t in (0..n - 1).rev() {
        labels[t] = back[(t + 1) * l + labels[t + 1] as usize];
    }
    labels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ties_keep_the_first_label() {
        let trans = [0.0, 0.0, 0.0, 0.0];
        assert_eq!(viterbi(&[1.0, 1.0, 2.0, 2.0], &trans, 2), [0, 0]);
        assert_eq!(viterbi(&[1.0, 1.0, 2.0, 3.0], &trans, 2), [0, 1]);
        // label 1 at t=1 is reached equally from both labels at t=0
        assert_eq!(viterbi(&[0.5, 0.5, 0.0, 1.0], &[0.0, 1.0, 0.0, 1.0], 2), [0, 1]);
        assert!(viterbi(&[], &trans, 2).is_empty());
    }

    #[test]
    fn rejects_other_files() {
        assert!(Model::parse(b"not a model").is_err());
        assert!(Model::parse(b"lCRF\0\0\0\0FOMC").is_err());
    }
}
