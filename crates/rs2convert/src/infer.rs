//! Links the cache doesn't store, inferred from structure: which sequences
//! could animate an NPC beyond the ones its definition names, and what
//! each one looks like it does.
//!
//! A sequence can only animate a model whose vertex labels fit the frame
//! base of its frames. Frame packs with identical frame bases form a
//! family, and a base that's a prefix of another (the same skeleton with
//! fewer slots) is treated as compatible with it. An NPC's candidates are
//! the sequences compatible with the ones it names, that move at least
//! half of its model's labels, ranked by how close their ids are to the
//! named ones. A candidate's role (idle, action, death) comes from posing
//! the NPC's model through it. Everything here is a guess with its
//! evidence attached;
//! `docs/conversion/inference.md` has the measurements behind it.

use std::collections::{BTreeMap, BTreeSet};

use rs2cache::anim::FrameIndex;
use rs2cache::types::anim::{BaseTransform, kind};
use rs2cache::types::record::{Record, RecordType};
use rs2cache::{Cache, Records};

use crate::ConvertError;
use crate::creature::Creature;

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
    /// What it looks like it does, once worked out with [`with_roles`].
    pub role: Option<Role>,
    /// The motion the role was worked out from.
    pub motion: Option<Motion>,
}

/// What a sequence looks like it does to a model, from its motion alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Barely moves: standing about.
    Idle,
    /// Moves and comes back to where it started: an attack, a block or a
    /// gait.
    Action,
    /// Goes down and stays down: the model's top drops by at least 30% of
    /// its height, it ends at most 75% as tall with its lowest point where
    /// it was, and the last frame is held (at least 50 cycles, 1 s). Falling
    /// over and then lying still looks the same, so some of these are
    /// falls rather than deaths.
    Death,
    /// Goes down the same way but doesn't stay: falling over, kneeling,
    /// sitting, floating down.
    Down,
    /// The whole body moves down, lowest point included (by at least 30%
    /// of its height): a fall, or a jump down into a gap.
    Drop,
    /// Ends with no height at all (under 5%): shrinking away, sinking out
    /// of sight, a teleport.
    Vanish,
    /// None of those.
    Other,
}

/// The order roles are grouped in: the most useful first.
pub const ROLE_ORDER: [Role; 7] = [
    Role::Death,
    Role::Action,
    Role::Down,
    Role::Drop,
    Role::Vanish,
    Role::Idle,
    Role::Other,
];

impl Role {
    /// Its place in [`ROLE_ORDER`].
    pub fn order(self) -> usize {
        ROLE_ORDER
            .iter()
            .position(|&r| r == self)
            .unwrap_or(ROLE_ORDER.len())
    }

    /// The role's name, lowercase.
    pub fn name(self) -> &'static str {
        match self {
            Role::Idle => "idle",
            Role::Action => "action",
            Role::Death => "death",
            Role::Down => "down",
            Role::Drop => "drop",
            Role::Vanish => "vanish",
            Role::Other => "other",
        }
    }
}

/// How a sequence moves a model, each distance as a share of the model's
/// height in the sequence's first frame.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Motion {
    /// The last frame's height.
    pub end_height: f64,
    /// How far the top has dropped between the first frame and the last
    /// (negative: risen).
    pub drop: f64,
    /// Mean vertex distance between the last frame and the first.
    pub returns: f64,
    /// The largest mean vertex distance from the first frame.
    pub peak: f64,
    /// How long one run of the sequence lasts, in client cycles (20 ms):
    /// each frame's sequence delay, or the frame's own when that's 0.
    pub duration: u32,
    /// How far the model's lowest point rises above where it started, at
    /// most: feet leaving the ground.
    pub lift: f64,
    /// How far the model's top rises above where it started, at most.
    pub rise: f64,
    /// How much lower the model's lowest point ends than it started
    /// (negative: higher).
    pub base_drop: f64,
    /// The model's widest horizontal extent at the end over at the start.
    pub spread: f64,
    /// How long the last frame is held, in cycles.
    pub hold: u32,
}

/// How far down (as a share of height) counts as going down.
const LOWERED: f64 = 0.3;
/// An end height below which the model has gone.
const VANISHED: f64 = 0.05;
/// A last frame held at least this long (cycles, 1 s) stays down. The
/// sequences that go down without staying hold their last frame for well
/// under half a second.
const HELD: u32 = 50;
/// Peak motion below which a sequence counts as idle.
const IDLE_PEAK: f64 = 0.06;
/// How far from the first frame an action may end.
const ACTION_RETURN: f64 = 0.15;

impl Motion {
    /// Measures sequence `s` of creature `c` (an index into its
    /// sequences), posed with the reference deformation. `None` when a
    /// frame won't pose or the model is empty or flat.
    pub fn measure(c: &Creature, s: usize) -> Option<Self> {
        let n = c.sequences.get(s)?.def.frames.len();
        let frames: Vec<Vec<[i32; 3]>> = (0..n).map(|i| c.posed(s, i)).collect::<Option<_>>()?;
        let (first, last) = (frames.first()?, frames.last()?);
        // y is down: the top is the smallest y.
        let span = |v: &[[i32; 3]]| {
            v.iter().fold((i32::MAX, i32::MIN), |(lo, hi), p| {
                (lo.min(p[1]), hi.max(p[1]))
            })
        };
        // Measured against the first frame, not the rest pose: some models
        // are animated in a pose quite unlike their rest pose.
        let (top, bottom) = span(first);
        let height = f64::from(bottom.saturating_sub(top));
        if first.is_empty() || height <= 0.0 {
            return None;
        }
        let mean_distance = |a: &[[i32; 3]], b: &[[i32; 3]]| {
            let total: f64 = a
                .iter()
                .zip(b)
                .map(|(p, q)| {
                    let d = [0, 1, 2].map(|k| f64::from(p[k] - q[k]));
                    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
                })
                .sum();
            total / a.len().max(1) as f64
        };
        let (end_top, end_bottom) = span(last);
        let delays = c.delays(s);
        // The widest horizontal extent, x or z.
        let extent = |v: &[[i32; 3]]| {
            let along = |a: usize| {
                let (lo, hi) = v.iter().fold((i32::MAX, i32::MIN), |(lo, hi), p| {
                    (lo.min(p[a]), hi.max(p[a]))
                });
                f64::from(hi.saturating_sub(lo))
            };
            along(0).max(along(2))
        };
        // y is down, so rising is a smaller y.
        let most_risen = |f: fn((i32, i32)) -> i32, from: i32| {
            frames
                .iter()
                .map(|v| f64::from(from - f(span(v))))
                .fold(0.0, f64::max)
                / height
        };
        Some(Self {
            duration: delays.iter().sum(),
            lift: most_risen(|(_, b)| b, bottom),
            rise: most_risen(|(t, _)| t, top),
            base_drop: f64::from(end_bottom - bottom) / height,
            spread: extent(last) / extent(first).max(1.0),
            hold: delays.last().copied().unwrap_or(0),
            end_height: f64::from(end_bottom - end_top) / height,
            drop: f64::from(end_top - top) / height,
            returns: mean_distance(last, first) / height,
            peak: frames
                .iter()
                .map(|f| mean_distance(f, first))
                .fold(0.0, f64::max)
                / height,
        })
    }

    /// The role this motion suggests.
    pub fn role(&self) -> Role {
        let lowered = self.drop >= LOWERED && self.end_height <= 0.75;
        if self.end_height < VANISHED {
            Role::Vanish
        } else if self.base_drop >= LOWERED {
            Role::Drop
        } else if lowered && self.hold >= HELD {
            Role::Death
        } else if lowered {
            Role::Down
        } else if self.peak < IDLE_PEAK {
            Role::Idle
        } else if self.returns <= ACTION_RETURN {
            Role::Action
        } else {
            Role::Other
        }
    }
}

/// Fills in each candidate's role by posing creature `c` through it.
/// Candidates whose sequence `c` doesn't have, or won't pose, keep `None`.
pub fn with_roles(c: &Creature, candidates: &mut [Candidate]) {
    let at: BTreeMap<u32, usize> = c
        .sequences
        .iter()
        .enumerate()
        .map(|(i, s)| (s.def.id, i))
        .collect();
    for cand in candidates {
        cand.motion = at.get(&cand.seq).and_then(|&s| Motion::measure(c, s));
        cand.role = cand.motion.map(|m| m.role());
    }
}

/// NPC `id`'s inferred sequences, given `named`, its creature with only
/// the sequences it names. Returns the creature with every candidate
/// added after the named ones, and the candidates with their roles, in
/// the order [`sort_by_role`] gives (the creature's sequences follow the
/// same order). With nothing to infer, or if the candidates won't load,
/// it's `named` and no candidates.
///
/// There's no cap: a creature has a few dozen candidates at most, and a
/// humanoid on the shared pool about 1200, which pose in about 0.1 s and
/// convert in a few seconds in a release build.
pub fn npc_candidates(
    seqs: &SequenceIndex,
    records: &Records,
    frames: &FrameIndex,
    id: u32,
    named: Creature,
) -> (Creature, Vec<Candidate>) {
    let names: Vec<u32> = named.sequences.iter().map(|s| s.def.id).collect();
    let labels: BTreeSet<u8> = named
        .model
        .vertex_labels
        .iter()
        .flatten()
        .copied()
        .collect();
    let mut best = seqs.candidates(&names, &labels);
    let extra: Vec<u16> = best
        .iter()
        .filter_map(|c| u16::try_from(c.seq).ok())
        .collect();
    if extra.is_empty() {
        return (named, Vec::new());
    }
    let Ok(mut c) = crate::creature::npc(records, frames, id, &extra) else {
        return (named, Vec::new());
    };
    with_roles(&c, &mut best);
    // A candidate that didn't load or won't pose on this model can't
    // play, so it's left out.
    best.retain(|b| b.role.is_some());
    sort_by_role(&mut best);
    let keep: BTreeSet<u32> = names
        .iter()
        .chain(best.iter().map(|b| &b.seq))
        .copied()
        .collect();
    c.sequences.retain(|s| keep.contains(&s.def.id));
    // The creature's sequences in the same order: named first, then the
    // candidates as sorted.
    let rank: BTreeMap<u32, usize> = best
        .iter()
        .enumerate()
        .map(|(i, b)| (b.seq, i + 1))
        .collect();
    c.sequences
        .sort_by_key(|s| rank.get(&s.def.id).copied().unwrap_or(0));
    (c, best)
}

/// The ids of NPC `id`'s inferred sequences (see [`npc_candidates`]), to
/// convert alongside the ones it names. Builds a [`SequenceIndex`] each
/// call, which decodes every frame pack, so a caller converting many NPCs
/// should build one and use [`npc_candidates`].
pub fn npc_extras(records: &Records, id: u32) -> Result<Vec<u16>, ConvertError> {
    let frames = records.cache().frame_index()?;
    let seqs = SequenceIndex::build(records.cache(), records, &frames)?;
    let named = crate::creature::npc(records, &frames, id, &[])?;
    let (_, picked) = npc_candidates(&seqs, records, &frames, id, named);
    Ok(picked
        .iter()
        .filter_map(|c| u16::try_from(c.seq).ok())
        .collect())
}

/// Actions at most this long (cycles, 1.2 s) come first in their group
/// on a shared pool: attacks are brief.
pub const SHORT_ACTION: u32 = 60;

/// Orders candidates with roles (see [`with_roles`]) into groups by role:
/// deaths, actions, idles, others, then any without a role.
///
/// Within a group, candidates on their own family keep score order, where
/// the score works. On a shared pool, where it doesn't, deaths go most
/// collapsed first, actions short ones first (see [`SHORT_ACTION`]) and
/// then the most motion first, and the rest by score.
pub fn sort_by_role(candidates: &mut [Candidate]) {
    let group = |c: &Candidate| c.role.map_or(ROLE_ORDER.len(), |r| r.order());
    candidates.sort_by(|a, b| {
        let within = match (a.shared(), a.role, a.motion, b.motion) {
            (true, Some(Role::Death), Some(ma), Some(mb)) => mb.drop.total_cmp(&ma.drop),
            (true, Some(Role::Action), Some(ma), Some(mb)) => {
                let short = |m: &Motion| m.duration <= SHORT_ACTION;
                short(&mb)
                    .cmp(&short(&ma))
                    .then(mb.peak.total_cmp(&ma.peak))
            }
            _ => b.score.total_cmp(&a.score),
        };
        group(a).cmp(&group(b)).then(within).then(a.seq.cmp(&b.seq))
    });
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
                    role: None,
                    motion: None,
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
    fn roles_from_motion() {
        let m = |end_height, drop, returns, peak| Motion {
            end_height,
            drop,
            returns,
            peak,
            duration: 20,
            ..Motion::default()
        };
        let held = |m: Motion| Motion { hold: 20000, ..m };
        // Goes down and stays down: death. Without the hold: down.
        assert_eq!(held(m(0.5, 0.56, 0.3, 0.42)).role(), Role::Death);
        assert_eq!(m(0.5, 0.56, 0.3, 0.42).role(), Role::Down);
        // The whole body going down is a drop, held or not.
        let falls = Motion {
            base_drop: 1.15,
            ..held(m(0.71, 1.45, 1.6, 1.6))
        };
        assert_eq!(falls.role(), Role::Drop);
        // Nothing left at the end: vanish, whatever else happened.
        assert_eq!(held(m(0.0, 0.7, 0.4, 0.4)).role(), Role::Vanish);
        assert_eq!(m(1.0, 0.0, 0.01, 0.02).role(), Role::Idle);
        assert_eq!(m(1.0, 0.0, 0.05, 0.15).role(), Role::Action);
        assert_eq!(m(0.97, 0.03, 0.3, 0.3).role(), Role::Other);
        // Crouching low but not collapsed isn't death.
        assert_eq!(m(0.8, 0.2, 0.05, 0.2).role(), Role::Action);
    }

    #[test]
    fn sorting_by_role_groups_and_orders() {
        let cand = |seq, score, drop, peak, duration, role, pool| Candidate {
            seq,
            score,
            coverage: 1.0,
            distance: 0,
            pool,
            role: Some(role),
            motion: Some(Motion {
                end_height: 0.5,
                drop,
                returns: 0.0,
                peak,
                duration,
                ..Motion::default()
            }),
        };
        let shared =
            |seq, drop, peak, duration, role| cand(seq, 0.0, drop, peak, duration, role, 1000);
        let mut all = vec![
            shared(6, 0.0, 0.01, 20, Role::Idle),
            shared(1, 0.4, 0.3, 20, Role::Death),
            shared(4, 0.0, 0.1, 20, Role::Action),
            shared(5, 0.0, 0.5, 200, Role::Action),
            shared(7, 0.0, 0.4, 20, Role::Other),
            shared(2, 0.6, 0.3, 20, Role::Death),
            shared(8, 0.0, 0.2, 20, Role::Action),
        ];
        sort_by_role(&mut all);
        let order: Vec<u32> = all.iter().map(|c| c.seq).collect();
        // Deaths most collapsed first; short actions before the long one,
        // most motion first; then idle, then other.
        assert_eq!(order, [2, 1, 8, 4, 5, 6, 7]);
        // On its own family, score order holds within a group.
        let mut own = vec![
            cand(1, 0.2, 0.6, 0.3, 20, Role::Death, 10),
            cand(2, 0.9, 0.4, 0.3, 20, Role::Death, 10),
        ];
        sort_by_role(&mut own);
        assert_eq!(own[0].seq, 2);
    }

    #[test]
    fn nothing_named_nothing_inferred() {
        assert!(index().candidates(&[999], &BTreeSet::new()).is_empty());
        assert!(index().candidates(&[], &BTreeSet::new()).is_empty());
    }
}
