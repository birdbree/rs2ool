//! Links the cache doesn't store, inferred from structure: which sequences
//! could animate an NPC beyond the ones its definition names.
//!
//! A sequence can only animate a model whose vertex labels fit the frame
//! base of its frames. Frame packs with identical frame bases form a
//! family, and a base that's a prefix of another (the same skeleton with
//! fewer slots) is treated as compatible with it. An NPC's candidates are
//! the sequences compatible with the ones it names, that move at least
//! half of its model's labels, ranked by how close their ids are to the
//! named ones. Everything here is a guess with its evidence attached;
//! `docs/conversion/inference.md` has the measurements behind it.

use std::collections::{BTreeMap, BTreeSet};

use rs2cache::anim::FrameIndex;
use rs2cache::types::anim::{BaseTransform, kind};
use rs2cache::types::record::{Record, RecordType};
use rs2cache::{Cache, Records};

use crate::ConvertError;

/// The share of a model's labels a candidate must move.
pub const MIN_COVERAGE: f32 = 0.5;

/// A family group with more sequences than this is a shared pool (the
/// humanoid skeleton, for one): every model on it fits every sequence in
/// it, so id distance says little and candidates are flagged as such.
pub const SHARED_POOL: usize = 300;

/// The id distance at which the distance term of a score halves.
const DISTANCE_SCALE: f32 = 8.0;

/// What's known about one sequence's frames.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SeqInfo {
    /// The families of the packs its frames come from (almost always one).
    pub families: BTreeSet<usize>,
    /// The vertex labels its frames actually move (translate, rotate or
    /// scale by something other than nothing).
    pub moved: BTreeSet<u8>,
}

/// Every sequence's frame-base family and moved labels, built once per
/// cache.
#[derive(Debug, Clone, Default)]
pub struct SequenceIndex {
    /// Per sequence id.
    pub seqs: BTreeMap<u32, SeqInfo>,
    /// For each family, the families compatible with it (itself, and any
    /// whose base is a prefix of its base or has it as a prefix).
    pub compatible: Vec<Vec<usize>>,
}

/// One inferred candidate sequence, with its evidence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candidate {
    /// The sequence id.
    pub seq: u32,
    /// 0–1, higher is likelier: coverage times a term that halves every
    /// 8 ids of distance. Only comparable within one NPC's list.
    pub score: f32,
    /// The share of the model's labels the sequence moves.
    pub coverage: f32,
    /// Distance in id to the nearest named sequence.
    pub distance: u32,
    /// How many sequences share the named sequences' family group.
    pub pool: usize,
}

impl Candidate {
    /// Whether it comes from a shared pool (see [`SHARED_POOL`]), where the
    /// ranking means little.
    pub fn shared(&self) -> bool {
        self.pool > SHARED_POOL
    }
}

impl SequenceIndex {
    /// Builds the index: decodes every frame pack and sequence. Packs and
    /// sequences that won't decode are left out.
    pub fn build(
        cache: &Cache,
        records: &Records,
        index: &FrameIndex,
    ) -> Result<Self, ConvertError> {
        // Families by identical base, in pack order.
        let mut bases: Vec<Vec<BaseTransform>> = Vec::new();
        let mut family_of_pack = BTreeMap::new();
        let mut frames: BTreeMap<u16, (u32, usize)> = BTreeMap::new();
        let mut packs = BTreeMap::new();
        for &file in index.pack_of.values().collect::<BTreeSet<_>>() {
            let Ok(p) = cache.frame_pack(file) else {
                continue;
            };
            let family = match bases.iter().position(|b| *b == p.base) {
                Some(f) => f,
                None => {
                    bases.push(p.base.clone());
                    bases.len() - 1
                }
            };
            family_of_pack.insert(file, family);
            for (i, f) in p.frames.iter().enumerate() {
                frames.insert(f.id, (file, i));
            }
            packs.insert(file, p);
        }
        let prefix =
            |a: &[BaseTransform], b: &[BaseTransform]| a.len() <= b.len() && b[..a.len()] == *a;
        let compatible = bases
            .iter()
            .map(|a| {
                (0..bases.len())
                    .filter(|&j| prefix(a, &bases[j]) || prefix(&bases[j], a))
                    .collect()
            })
            .collect();
        let mut seqs = BTreeMap::new();
        for id in records.ids(RecordType::Seq)? {
            let Ok(Record::Seq(def)) = records.decode(RecordType::Seq, id) else {
                continue;
            };
            let mut info = SeqInfo::default();
            for sf in &def.frames {
                let Some(&(file, i)) = frames.get(&sf.frame) else {
                    continue;
                };
                info.families.insert(family_of_pack[&file]);
                let p = &packs[&file];
                for t in &p.frames[i].transforms {
                    let Some(b) = p.base.get(usize::from(t.transform)) else {
                        continue;
                    };
                    let moves = match b.kind {
                        kind::TRANSLATE | kind::ROTATE => (t.dx, t.dy, t.dz) != (0, 0, 0),
                        kind::SCALE => (t.dx, t.dy, t.dz) != (128, 128, 128),
                        _ => false,
                    };
                    if moves {
                        info.moved.extend(&b.labels);
                    }
                }
            }
            if !info.families.is_empty() {
                seqs.insert(id, info);
            }
        }
        Ok(Self { seqs, compatible })
    }

    /// Candidate sequences for a model with vertex labels `labels` that's
    /// known to play `named`, best first. The named sequences themselves
    /// aren't listed. Empty when none of `named` is in the index.
    pub fn candidates(&self, named: &[u32], labels: &BTreeSet<u8>) -> Vec<Candidate> {
        let anchors: Vec<u32> = named
            .iter()
            .copied()
            .filter(|s| self.seqs.contains_key(s))
            .collect();
        // Every family compatible with a named sequence's.
        let group: BTreeSet<usize> = anchors
            .iter()
            .flat_map(|s| &self.seqs[s].families)
            .flat_map(|&f| self.compatible[f].iter().copied())
            .collect();
        let in_group: Vec<u32> = self
            .seqs
            .iter()
            .filter(|(_, i)| !i.families.is_disjoint(&group))
            .map(|(&s, _)| s)
            .collect();
        let pool = in_group.len();
        let mut out: Vec<Candidate> = in_group
            .into_iter()
            .filter(|s| !anchors.contains(s))
            .filter_map(|seq| {
                let coverage = coverage(&self.seqs[&seq].moved, labels);
                if coverage < MIN_COVERAGE {
                    return None;
                }
                let distance = anchors.iter().map(|&a| a.abs_diff(seq)).min()?;
                let score = coverage * DISTANCE_SCALE / (DISTANCE_SCALE + distance as f32);
                Some(Candidate {
                    seq,
                    score,
                    coverage,
                    distance,
                    pool,
                })
            })
            .collect();
        out.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.seq.cmp(&b.seq)));
        out
    }
}

/// The share of `labels` that `moved` covers; 1 for a model without
/// labels.
fn coverage(moved: &BTreeSet<u8>, labels: &BTreeSet<u8>) -> f32 {
    if labels.is_empty() {
        return 1.0;
    }
    moved.intersection(labels).count() as f32 / labels.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(n: usize) -> Vec<BaseTransform> {
        (0..n)
            .map(|i| BaseTransform {
                kind: kind::ROTATE,
                labels: vec![i as u8],
            })
            .collect()
    }

    /// Three families: 0 and 1 compatible (1's base extends 0's), 2 apart.
    fn index() -> SequenceIndex {
        let bases = [
            base(3),
            base(5),
            vec![BaseTransform {
                kind: kind::TRANSLATE,
                labels: vec![9],
            }],
        ];
        let prefix =
            |a: &[BaseTransform], b: &[BaseTransform]| a.len() <= b.len() && b[..a.len()] == *a;
        let compatible = bases
            .iter()
            .map(|a| {
                (0..3)
                    .filter(|&j| prefix(a, &bases[j]) || prefix(&bases[j], a))
                    .collect()
            })
            .collect();
        let info = |f: usize, moved: &[u8]| SeqInfo {
            families: [f].into(),
            moved: moved.iter().copied().collect(),
        };
        SequenceIndex {
            seqs: [
                (100, info(0, &[0, 1, 2])),
                (101, info(0, &[0, 1])),
                (110, info(1, &[0, 1, 2, 3])),
                (102, info(0, &[0])),
                (500, info(0, &[0, 1, 2])),
                (103, info(2, &[9])),
            ]
            .into(),
            compatible,
        }
    }

    #[test]
    fn candidates_share_a_compatible_family_and_rank_by_distance() {
        let labels: BTreeSet<u8> = [0, 1, 2].into();
        let c = index().candidates(&[100], &labels);
        let seqs: Vec<u32> = c.iter().map(|c| c.seq).collect();
        // 102 moves one label of three: filtered out. 103 is another
        // family. 110's base extends the named one's, so it counts.
        assert_eq!(seqs, [101, 110, 500]);
        assert_eq!(c[0].distance, 1);
        assert!((c[0].coverage - 2.0 / 3.0).abs() < 1e-6);
        assert!(c[0].score > c[1].score && c[1].score > c[2].score);
        assert_eq!(c[0].pool, 5);
        assert!(!c[0].shared());
    }

    #[test]
    fn nothing_named_nothing_inferred() {
        assert!(index().candidates(&[999], &BTreeSet::new()).is_empty());
        assert!(index().candidates(&[], &BTreeSet::new()).is_empty());
    }
}
