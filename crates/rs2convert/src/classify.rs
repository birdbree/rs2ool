//! A small decision tree that tells a sequence's [`Role`] from its
//! [`Motion`], learned from sequences labelled by eye, to set beside the
//! hand-tuned rules of [`Motion::role`].
//!
//! It's written here rather than taken from a crate because it's tiny: a
//! binary tree over the eleven motion measures, grown greedily on weighted
//! Gini impurity, depth-limited and with a minimum leaf weight. Both keep
//! it small enough to read, which matters more than accuracy with a few
//! dozen labels. Growing it is deterministic: ties go to the first feature
//! and the lowest threshold, and a leaf's role to the earliest in
//! [`ROLE_ORDER`].
//!
//! ```text
//! features  end_height drop returns peak duration lift rise base_drop spread hold stretch
//! split     feature < at  →  below, else above
//! ```

use std::collections::BTreeMap;
use std::fmt::Write;

use serde::{Deserialize, Serialize};

use crate::infer::{Motion, ROLE_ORDER, Role};

/// How many measures the tree splits on.
pub const FEATURE_COUNT: usize = 14;

/// The measures the tree splits on, in [`features`] order. New ones go at
/// the end, so a saved tree's feature numbers keep their meaning.
pub const FEATURES: [&str; FEATURE_COUNT] = [
    "end_height",
    "drop",
    "returns",
    "peak",
    "duration",
    "lift",
    "rise",
    "base_drop",
    "spread",
    "hold",
    "stretch",
    "end_visible",
    "min_visible",
    "start_visible",
];

/// A motion as the tree sees it, in [`FEATURES`] order.
pub fn features(m: &Motion) -> [f64; FEATURE_COUNT] {
    [
        m.end_height,
        m.drop,
        m.returns,
        m.peak,
        f64::from(m.duration),
        m.lift,
        m.rise,
        m.base_drop,
        m.spread,
        f64::from(m.hold),
        m.stretch,
        m.end_visible,
        m.min_visible,
        m.start_visible,
    ]
}

/// A labelled sequence to learn from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// Its motion, from [`features`].
    pub x: [f64; FEATURE_COUNT],
    /// The role it really has.
    pub role: Role,
    /// How much it counts; 1 for a label checked by eye.
    pub weight: f64,
}

/// How big the tree may grow.
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
            max_depth: 3,
            min_leaf: 2.0,
        }
    }
}

/// A learned tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tree {
    /// Every motion that gets here has this role.
    Leaf {
        /// The role.
        role: Role,
    },
    /// Motions whose feature is below `at` go one way, the rest the other.
    Split {
        /// An index into [`FEATURES`].
        feature: usize,
        /// The threshold.
        at: f64,
        /// Where `x[feature] < at` goes.
        below: Box<Tree>,
        /// Where the rest go.
        above: Box<Tree>,
    },
}

/// Weighted count per role.
fn counts<'a>(samples: impl Iterator<Item = &'a Sample>) -> BTreeMap<Role, f64> {
    let mut c = BTreeMap::new();
    for s in samples {
        *c.entry(s.role).or_default() += s.weight;
    }
    c
}

/// Gini impurity times total weight, so two sides add up.
fn impurity(c: &BTreeMap<Role, f64>) -> f64 {
    let total: f64 = c.values().sum();
    if total <= 0.0 {
        return 0.0;
    }
    total - c.values().map(|w| w * w).sum::<f64>() / total
}

/// The heaviest role, ties to the earliest in [`ROLE_ORDER`];
/// [`Role::Other`] when there's nothing to go on.
fn majority(c: &BTreeMap<Role, f64>) -> Role {
    if c.values().sum::<f64>() <= 0.0 {
        return Role::Other;
    }
    ROLE_ORDER
        .iter()
        .copied()
        .fold(None::<(Role, f64)>, |best, r| {
            let w = c.get(&r).copied().unwrap_or(0.0);
            match best {
                Some((_, bw)) if bw >= w => best,
                _ => Some((r, w)),
            }
        })
        .map_or(Role::Other, |(r, _)| r)
}

impl Tree {
    /// Grows a tree on `samples`. With no samples it's a single leaf of
    /// [`Role::Other`].
    pub fn train(samples: &[Sample], p: Params) -> Self {
        Self::grow(&samples.iter().collect::<Vec<_>>(), p, 0)
    }

    fn grow(samples: &[&Sample], p: Params, depth: usize) -> Self {
        let all = counts(samples.iter().copied());
        let leaf = Tree::Leaf {
            role: majority(&all),
        };
        let total: f64 = all.values().sum();
        if depth >= p.max_depth || all.len() <= 1 || total < 2.0 * p.min_leaf {
            return leaf;
        }
        let here = impurity(&all);
        // (gain, feature, threshold), best so far.
        let mut best: Option<(f64, usize, f64)> = None;
        for f in 0..FEATURES.len() {
            let mut sorted = samples.to_vec();
            sorted.sort_by(|a, b| a.x[f].total_cmp(&b.x[f]));
            let mut below: BTreeMap<Role, f64> = BTreeMap::new();
            let mut below_w = 0.0;
            for i in 0..sorted.len() - 1 {
                *below.entry(sorted[i].role).or_default() += sorted[i].weight;
                below_w += sorted[i].weight;
                let (v, next) = (sorted[i].x[f], sorted[i + 1].x[f]);
                if v == next || below_w < p.min_leaf || total - below_w < p.min_leaf {
                    continue;
                }
                let above: BTreeMap<Role, f64> = all
                    .iter()
                    .map(|(r, w)| (*r, w - below.get(r).copied().unwrap_or(0.0)))
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
        let (lo, hi): (Vec<&Sample>, Vec<&Sample>) =
            samples.iter().partition(|s| s.x[feature] < at);
        Tree::Split {
            feature,
            at,
            below: Box::new(Self::grow(&lo, p, depth + 1)),
            above: Box::new(Self::grow(&hi, p, depth + 1)),
        }
    }

    /// The role it gives a motion's features.
    pub fn predict(&self, x: &[f64; FEATURE_COUNT]) -> Role {
        match self {
            Tree::Leaf { role } => *role,
            Tree::Split {
                feature,
                at,
                below,
                above,
            } => {
                if x[*feature] < *at {
                    below.predict(x)
                } else {
                    above.predict(x)
                }
            }
        }
    }

    /// The role it gives a motion.
    pub fn role(&self, m: &Motion) -> Role {
        self.predict(&features(m))
    }

    /// The tree as indented rules, one line per node.
    pub fn describe(&self) -> String {
        let mut out = String::new();
        self.describe_into(&mut out, 0);
        out
    }

    fn describe_into(&self, out: &mut String, depth: usize) {
        let pad = "  ".repeat(depth);
        match self {
            Tree::Leaf { role } => {
                let _ = writeln!(out, "{pad}{}", role.name());
            }
            Tree::Split {
                feature,
                at,
                below,
                above,
            } => {
                let _ = writeln!(out, "{pad}{} < {at:.3}:", FEATURES[*feature]);
                below.describe_into(out, depth + 1);
                let _ = writeln!(out, "{pad}else:");
                above.describe_into(out, depth + 1);
            }
        }
    }
}

/// A global tree, and trees for the skeleton families with enough labels
/// of their own, as `rs2ool cache classify --save` keeps them. Family
/// numbers are [`crate::infer::SequenceIndex`] families, which depend on
/// the cache, so the cache's CRC comes along.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoleTrees {
    /// The cache the trees were learned on
    /// ([`crate::source_cache_crc`]).
    pub cache_crc: u32,
    /// Grown on every label.
    pub global: Tree,
    /// Grown on one family's labels, by family.
    #[serde(default)]
    pub families: BTreeMap<usize, Tree>,
}

/// The trees the tools come with, learned on the maintainer's cache with
/// `rs2ool cache classify --save` (see `docs/conversion/inference.md`).
pub const BUILT_IN: &str = include_str!("../data/role-tree.json");

impl RoleTrees {
    /// [`BUILT_IN`], parsed.
    pub fn built_in() -> Self {
        serde_json::from_str(BUILT_IN).expect("the built-in role trees parse")
    }

    /// These trees for the cache whose CRC is `crc`: as they are on the
    /// cache they were learned on, else the global tree alone, since the
    /// family numbers don't carry over to another cache but the measures
    /// the global tree splits on do.
    pub fn for_cache(mut self, crc: u32) -> Self {
        if self.cache_crc != crc {
            self.families.clear();
        }
        self
    }

    /// The tree for an NPC on skeleton family `family`: its own if it has
    /// one, else the global one.
    pub fn for_family(&self, family: Option<usize>) -> &Tree {
        family
            .and_then(|f| self.families.get(&f))
            .unwrap_or(&self.global)
    }
}

/// Each sample's role from a tree grown on all the others: an honest
/// estimate of how the tree does on sequences it hasn't seen.
pub fn leave_one_out(samples: &[Sample], p: Params) -> Vec<Role> {
    (0..samples.len())
        .map(|i| {
            let rest: Vec<Sample> = samples
                .iter()
                .enumerate()
                .filter(|&(j, _)| j != i)
                .map(|(_, s)| *s)
                .collect();
            Tree::train(&rest, p).predict(&samples[i].x)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(drop: f64, hold: f64, role: Role) -> Sample {
        let mut x = [0.0; FEATURE_COUNT];
        x[1] = drop;
        x[9] = hold;
        Sample {
            x,
            role,
            weight: 1.0,
        }
    }

    #[test]
    fn learns_a_two_feature_rule() {
        // Deaths drop and hold; downs drop without holding; actions don't
        // drop.
        let mut s = Vec::new();
        for i in 0..4 {
            let j = f64::from(i) * 0.01;
            s.push(sample(0.5 + j, 80.0 + j, Role::Death));
            s.push(sample(0.5 + j, 5.0 + j, Role::Down));
            s.push(sample(0.05 + j, 5.0 + j, Role::Action));
        }
        let t = Tree::train(&s, Params::default());
        for x in &s {
            assert_eq!(t.predict(&x.x), x.role);
        }
        assert_eq!(t.predict(&sample(0.6, 100.0, Role::Other).x), Role::Death);
        assert!(t.describe().contains("drop < "));
        // Unseen, each still lands right: the rule generalises.
        assert!(
            leave_one_out(&s, Params::default())
                .iter()
                .zip(&s)
                .all(|(p, x)| *p == x.role)
        );
        // It round-trips through JSON.
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(serde_json::from_str::<Tree>(&json).unwrap(), t);
    }

    #[test]
    fn family_trees_fall_back_to_the_global_one() {
        let leaf = |role| Tree::Leaf { role };
        let trees = RoleTrees {
            cache_crc: 7,
            global: leaf(Role::Action),
            families: [(3, leaf(Role::Death))].into(),
        };
        assert_eq!(trees.for_family(Some(3)), &leaf(Role::Death));
        assert_eq!(trees.for_family(Some(4)), &leaf(Role::Action));
        assert_eq!(trees.for_family(None), &leaf(Role::Action));
        let json = serde_json::to_string(&trees).unwrap();
        assert_eq!(serde_json::from_str::<RoleTrees>(&json).unwrap(), trees);
    }

    #[test]
    fn limits_hold() {
        let s: Vec<Sample> = (0..6)
            .map(|i| sample(f64::from(i), 0.0, ROLE_ORDER[i as usize]))
            .collect();
        fn depth(t: &Tree) -> usize {
            match t {
                Tree::Leaf { .. } => 0,
                Tree::Split { below, above, .. } => 1 + depth(below).max(depth(above)),
            }
        }
        let p = Params {
            max_depth: 2,
            min_leaf: 1.0,
        };
        assert!(depth(&Tree::train(&s, p)) <= 2);
        // A minimum leaf of 3 allows one split of six samples, no more.
        let p = Params {
            max_depth: 5,
            min_leaf: 3.0,
        };
        assert_eq!(depth(&Tree::train(&s, p)), 1);
        assert_eq!(
            Tree::train(&[], Params::default()),
            Tree::Leaf { role: Role::Other }
        );
        // The same samples always grow the same tree.
        assert_eq!(Tree::train(&s, p), Tree::train(&s, p));
    }

    #[test]
    fn the_built_in_trees_load_and_keep_to_their_cache() {
        let t = RoleTrees::built_in();
        // Every split names a measure there is.
        fn features_ok(t: &Tree) -> bool {
            match t {
                Tree::Leaf { .. } => true,
                Tree::Split {
                    feature,
                    below,
                    above,
                    ..
                } => *feature < FEATURE_COUNT && features_ok(below) && features_ok(above),
            }
        }
        assert!(features_ok(&t.global));
        assert!(t.families.values().all(features_ok));
        assert!(!t.families.is_empty());
        // On their own cache they're whole; on another, the global tree
        // alone.
        let crc = t.cache_crc;
        assert_eq!(t.clone().for_cache(crc), t);
        let other = t.clone().for_cache(crc.wrapping_add(1));
        assert!(other.families.is_empty());
        assert_eq!(other.global, t.global);
        let family = *t.families.keys().next().unwrap();
        assert_eq!(other.for_family(Some(family)), &t.global);
    }
}
