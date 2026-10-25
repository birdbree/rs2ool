//! A small decision tree over plain feature vectors, for any label: grown
//! greedily on weighted Gini impurity, depth-limited, with a minimum
//! weight either side of every split.
//!
//! It's the same method as [`crate::classify`]'s role tree, which keeps its
//! own fixed motion measures and role labels; this one takes features as a
//! slice and labels of any ordered type, for structures, rooms and pairs of
//! structures ([`crate::purpose`]). Growing is deterministic: ties go to
//! the first feature and the lowest threshold, and a leaf's label to the
//! least label among the heaviest.

use std::collections::BTreeMap;
use std::fmt::Write;

use serde::{Deserialize, Serialize};

/// A labelled example to learn from.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample<L> {
    /// Its features, the same length for every sample.
    pub x: Vec<f64>,
    /// The label it really has.
    pub label: L,
    /// How much it counts.
    pub weight: f64,
}

/// How big a tree may grow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    /// Splits from the root to a leaf, at most.
    pub max_depth: usize,
    /// The least weight either side of a split may have.
    pub min_leaf: f64,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            max_depth: 6,
            min_leaf: 2.0,
        }
    }
}

/// A learned tree. In JSON a leaf is just its label and a split is
/// `[feature, at, below, above]`, so forests stay small.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Tree<L> {
    /// Everything that gets here has this label.
    Leaf(L),
    /// Examples whose feature is below `at` go one way, the rest the other.
    Split(
        /// An index into the features.
        usize,
        /// The threshold.
        f64,
        /// Where `x[feature] < at` goes.
        Box<Tree<L>>,
        /// Where the rest go.
        Box<Tree<L>>,
    ),
}

/// Weighted count per label.
fn counts<'a, L: Ord + Clone + 'a>(
    samples: impl Iterator<Item = &'a Sample<L>>,
) -> BTreeMap<L, f64> {
    let mut c = BTreeMap::new();
    for s in samples {
        *c.entry(s.label.clone()).or_default() += s.weight;
    }
    c
}

/// Gini impurity times total weight, so two sides add up.
fn impurity<L>(c: &BTreeMap<L, f64>) -> f64 {
    let total: f64 = c.values().sum();
    if total <= 0.0 {
        return 0.0;
    }
    total - c.values().map(|w| w * w).sum::<f64>() / total
}

/// The heaviest label, ties to the least.
fn majority<L: Ord + Clone>(c: &BTreeMap<L, f64>) -> Option<L> {
    c.iter()
        .fold(None::<(&L, f64)>, |best, (l, &w)| match best {
            Some((_, bw)) if bw >= w => best,
            _ => Some((l, w)),
        })
        .map(|(l, _)| l.clone())
}

impl<L: Ord + Clone> Tree<L> {
    /// Grows a tree on `samples`, a single leaf of `fallback` when there's
    /// nothing to learn from.
    pub fn train(samples: &[Sample<L>], p: Params, fallback: L) -> Self {
        let features = samples.first().map_or(0, |s| s.x.len());
        let mut all = |_: usize| (0..features).collect();
        Self::grow(
            &samples.iter().collect::<Vec<_>>(),
            p,
            0,
            &fallback,
            &mut all,
        )
    }

    /// Grows a tree trying, at each split, only the features `pick` hands
    /// out for that node.
    fn grow(
        samples: &[&Sample<L>],
        p: Params,
        depth: usize,
        fallback: &L,
        pick: &mut dyn FnMut(usize) -> Vec<usize>,
    ) -> Self {
        let all = counts(samples.iter().copied());
        let leaf = Tree::Leaf(majority(&all).unwrap_or_else(|| fallback.clone()));
        let total: f64 = all.values().sum();
        if depth >= p.max_depth || all.len() <= 1 || total < 2.0 * p.min_leaf {
            return leaf;
        }
        let features = samples.first().map_or(0, |s| s.x.len());
        let here = impurity(&all);
        // (gain, feature, threshold), best so far.
        let mut best: Option<(f64, usize, f64)> = None;
        for f in pick(features) {
            let mut sorted = samples.to_vec();
            sorted.sort_by(|a, b| a.x[f].total_cmp(&b.x[f]));
            let mut below: BTreeMap<L, f64> = BTreeMap::new();
            let mut below_w = 0.0;
            for i in 0..sorted.len() - 1 {
                *below.entry(sorted[i].label.clone()).or_default() += sorted[i].weight;
                below_w += sorted[i].weight;
                let (v, next) = (sorted[i].x[f], sorted[i + 1].x[f]);
                if v == next || below_w < p.min_leaf || total - below_w < p.min_leaf {
                    continue;
                }
                let above: BTreeMap<L, f64> = all
                    .iter()
                    .map(|(l, w)| (l.clone(), w - below.get(l).copied().unwrap_or(0.0)))
                    .collect();
                let gain = here - impurity(&below) - impurity(&above);
                if gain > 1e-9 && best.is_none_or(|(g, _, _)| gain > g + 1e-12) {
                    best = Some((gain, f, (v + next) / 2.0));
                }
            }
        }
        let Some((_, feature, at)) = best else {
            return leaf;
        };
        let (lo, hi): (Vec<&Sample<L>>, Vec<&Sample<L>>) =
            samples.iter().partition(|s| s.x[feature] < at);
        Tree::Split(
            feature,
            at,
            Box::new(Self::grow(&lo, p, depth + 1, fallback, pick)),
            Box::new(Self::grow(&hi, p, depth + 1, fallback, pick)),
        )
    }

    /// The label it gives `x`. A feature past the end of `x` counts as 0.
    pub fn predict(&self, x: &[f64]) -> &L {
        match self {
            Tree::Leaf(label) => label,
            Tree::Split(feature, at, below, above) => {
                if x.get(*feature).copied().unwrap_or(0.0) < *at {
                    below.predict(x)
                } else {
                    above.predict(x)
                }
            }
        }
    }

    /// The tree as indented rules, one line per node, with features named
    /// by `names` and labels by `label`.
    pub fn describe(&self, names: &[&str], label: &dyn Fn(&L) -> String) -> String {
        let mut out = String::new();
        self.describe_into(&mut out, 0, names, label);
        out
    }

    fn describe_into(
        &self,
        out: &mut String,
        depth: usize,
        names: &[&str],
        label: &dyn Fn(&L) -> String,
    ) {
        let pad = "  ".repeat(depth);
        match self {
            Tree::Leaf(l) => {
                let _ = writeln!(out, "{pad}{}", label(l));
            }
            Tree::Split(feature, at, below, above) => {
                let name = names.get(*feature).copied().unwrap_or("?");
                let _ = writeln!(out, "{pad}{name} < {at:.3}:");
                below.describe_into(out, depth + 1, names, label);
                let _ = writeln!(out, "{pad}else:");
                above.describe_into(out, depth + 1, names, label);
            }
        }
    }
}

/// A small, fixed pseudo-random sequence (xorshift), so forests grow the
/// same every time.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        // Never zero, which would stick.
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A number below `n`.
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// How a forest grows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ForestParams {
    /// Trees in it.
    pub trees: usize,
    /// Each tree's size.
    pub tree: Params,
    /// Features each split tries, picked at random; the square root of
    /// the number of features when `None`.
    pub features: Option<usize>,
}

/// Trees grown on resamples of the labels, each split trying a random few
/// features, that vote: steadier than any one tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Forest<L> {
    /// The trees.
    pub trees: Vec<Tree<L>>,
}

impl<L: Ord + Clone> Forest<L> {
    /// Grows a forest on `samples`. Tree `k` resamples the samples (as many
    /// as there are, with repeats) and picks its features from a sequence
    /// seeded by `k`, so the forest is the same every time.
    pub fn train(samples: &[Sample<L>], p: ForestParams, fallback: L) -> Self {
        let features = samples.first().map_or(0, |s| s.x.len());
        let per_split = p
            .features
            .unwrap_or_else(|| (features as f64).sqrt().ceil() as usize)
            .clamp(1, features.max(1));
        let trees = (0..p.trees)
            .map(|k| {
                let mut rng = Rng::new(k as u64 + 1);
                let picked: Vec<&Sample<L>> = (0..samples.len())
                    .map(|_| &samples[rng.below(samples.len())])
                    .collect();
                let mut pick = |n: usize| {
                    let mut all: Vec<usize> = (0..n).collect();
                    // A partial shuffle: the first `per_split` are a random
                    // pick.
                    for i in 0..per_split.min(n) {
                        let j = i + rng.below(n - i);
                        all.swap(i, j);
                    }
                    all.truncate(per_split);
                    all.sort_unstable();
                    all
                };
                Tree::grow(&picked, p.tree, 0, &fallback, &mut pick)
            })
            .collect();
        Self { trees }
    }

    /// The label most trees give `x`, ties to the least; `fallback` for an
    /// empty forest.
    pub fn predict(&self, x: &[f64], fallback: &L) -> L {
        let mut votes: BTreeMap<&L, usize> = BTreeMap::new();
        for t in &self.trees {
            *votes.entry(t.predict(x)).or_default() += 1;
        }
        votes
            .into_iter()
            .fold(None::<(&L, usize)>, |best, (l, n)| match best {
                Some((_, bn)) if bn >= n => best,
                _ => Some((l, n)),
            })
            .map_or_else(|| fallback.clone(), |(l, _)| l.clone())
    }
}

/// [`cross_validate`] for a forest.
pub fn cross_validate_forest<L: Ord + Clone>(
    samples: &[Sample<L>],
    group: &dyn Fn(usize) -> usize,
    folds: usize,
    p: ForestParams,
    fallback: &L,
) -> Vec<L> {
    let folds = folds.max(2);
    let fold = |i: usize| group(i) % folds;
    let mut out: Vec<Option<L>> = vec![None; samples.len()];
    for k in 0..folds {
        let rest: Vec<Sample<L>> = samples
            .iter()
            .enumerate()
            .filter(|&(i, _)| fold(i) != k)
            .map(|(_, s)| s.clone())
            .collect();
        let forest = Forest::train(&rest, p, fallback.clone());
        for (i, s) in samples.iter().enumerate().filter(|&(i, _)| fold(i) == k) {
            out[i] = Some(forest.predict(&s.x, fallback));
        }
    }
    out.into_iter()
        .map(|l| l.unwrap_or_else(|| fallback.clone()))
        .collect()
}

/// Each sample's label from a tree grown without it: the samples are split
/// into `folds` groups by `group` (samples that must be held out together,
/// such as the rooms of one structure, share a group), and each group is
/// predicted by a tree grown on the rest. An honest estimate of how a tree
/// does on what it hasn't seen.
pub fn cross_validate<L: Ord + Clone>(
    samples: &[Sample<L>],
    group: &dyn Fn(usize) -> usize,
    folds: usize,
    p: Params,
    fallback: &L,
) -> Vec<L> {
    let folds = folds.max(2);
    let fold = |i: usize| group(i) % folds;
    let mut out: Vec<Option<L>> = vec![None; samples.len()];
    for k in 0..folds {
        let rest: Vec<Sample<L>> = samples
            .iter()
            .enumerate()
            .filter(|&(i, _)| fold(i) != k)
            .map(|(_, s)| s.clone())
            .collect();
        let tree = Tree::train(&rest, p, fallback.clone());
        for (i, s) in samples.iter().enumerate().filter(|&(i, _)| fold(i) == k) {
            out[i] = Some(tree.predict(&s.x).clone());
        }
    }
    out.into_iter()
        .map(|l| l.unwrap_or_else(|| fallback.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(x: f64, y: f64, label: &'static str) -> Sample<&'static str> {
        Sample {
            x: vec![x, y],
            label,
            weight: 1.0,
        }
    }

    #[test]
    fn learns_a_two_feature_rule() {
        // "a" when x is small, else "b" when y is small, else "c".
        let mut samples = Vec::new();
        for i in 0..6 {
            let v = f64::from(i);
            samples.push(s(v, 10.0 + v, "a"));
            samples.push(s(20.0 + v, v, "b"));
            samples.push(s(20.0 + v, 20.0 + v, "c"));
        }
        let t = Tree::train(&samples, Params::default(), "?");
        assert_eq!(*t.predict(&[1.0, 50.0]), "a");
        assert_eq!(*t.predict(&[30.0, 1.0]), "b");
        assert_eq!(*t.predict(&[30.0, 30.0]), "c");
        let text = t.describe(&["x", "y"], &|l| l.to_string());
        assert!(text.contains("x < ") && text.contains("y < "), "{text}");
        // Held out, every sample still comes out right. Each group is one
        // of each label, so every fold has all three to learn from.
        let cv = cross_validate(&samples, &|i| i / 3, 3, Params::default(), &"?");
        assert_eq!(cv, samples.iter().map(|s| s.label).collect::<Vec<_>>());
    }

    #[test]
    fn forests_vote_and_grow_the_same_every_time() {
        let mut samples = Vec::new();
        for i in 0..20 {
            let v = f64::from(i);
            samples.push(s(v, 0.0, "low"));
            samples.push(s(100.0 + v, 0.0, "high"));
        }
        let p = ForestParams {
            trees: 9,
            tree: Params::default(),
            features: None,
        };
        let f = Forest::train(&samples, p, "?");
        assert_eq!(f.trees.len(), 9);
        assert_eq!(f.predict(&[5.0, 0.0], &"?"), "low");
        assert_eq!(f.predict(&[110.0, 0.0], &"?"), "high");
        assert_eq!(f, Forest::train(&samples, p, "?"));
        assert_eq!(Forest::<&str> { trees: vec![] }.predict(&[1.0], &"?"), "?");
    }

    #[test]
    fn trees_are_compact_in_json() {
        let t = Tree::Split(
            1,
            2.5,
            Box::new(Tree::Leaf(true)),
            Box::new(Tree::Leaf(false)),
        );
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, "[1,2.5,true,false]");
        assert_eq!(serde_json::from_str::<Tree<bool>>(&json).unwrap(), t);
    }

    #[test]
    fn limits_and_fallbacks_hold() {
        let samples = vec![s(0.0, 0.0, "a"), s(1.0, 0.0, "b")];
        // Too little weight to split either side.
        let t = Tree::train(&samples, Params::default(), "?");
        assert!(matches!(t, Tree::Leaf(_)));
        // Ties go to the least label.
        assert_eq!(*t.predict(&[5.0, 5.0]), "a");
        // Nothing to learn from: the fallback.
        assert_eq!(*Tree::train(&[], Params::default(), "?").predict(&[]), "?");
    }
}
