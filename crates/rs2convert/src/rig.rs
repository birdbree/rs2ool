//! Rig inference from frame-base label sets.
//!
//! A *bone candidate* is a distinct label set moved by a transform the
//! creature's frames use, restricted to the labels its mesh has. Candidates
//! nest: `A` is `B`'s parent when `B ⊂ A` strictly (after removing
//! transitive edges; among several minimal supersets the largest wins). A
//! candidate with no superset but overlapping another is attached by
//! co-movement. A root bone sits above the rest. Each label binds to the
//! smallest candidate containing it (the deepest bone).

use std::collections::{BTreeMap, BTreeSet};

use glam::DVec3;
use rs2intermediate::anim::kind;
use rs2pack::records::rig::{Bone, NO_PARENT, Rig, UNBOUND};
use serde::Serialize;

use crate::ConvertError;
use crate::creature::Creature;
use crate::deform::Groups;

pub type LabelSet = BTreeSet<u8>;

/// An inferred bone.
#[derive(Debug, Clone, Serialize)]
pub struct InferredBone {
    pub name: String,
    pub labels: LabelSet,
    pub parent: Option<usize>,
    /// Pivot in cache coordinates (y down): the centroid of the bone's
    /// vertices in the rest mesh.
    pub pivot: [i32; 3],
}

/// The inferred rig: bones ordered parents first, bone 0 the root.
#[derive(Debug, Clone, Serialize)]
pub struct InferredRig {
    pub bones: Vec<InferredBone>,
    /// Bone index per label (labels absent from every candidate bind to
    /// the root).
    pub label_bone: BTreeMap<u8, usize>,
}

/// A manual parent table for one creature (the `--override` file): bone
/// candidates by sorted label list, each with its parent's label list
/// (an empty parent list means the root).
pub type ParentOverride = Vec<(Vec<u8>, Vec<u8>)>;

/// The label sets of the transforms `creature`'s frames use, restricted to
/// the mesh's labels, with how often each set is rotated.
fn candidates(creature: &Creature) -> BTreeMap<LabelSet, usize> {
    let mesh = creature.mesh_labels();
    let mut out: BTreeMap<LabelSet, usize> = BTreeMap::new();
    for s in &creature.sequences {
        for i in 0..s.def.frames.len() {
            let Some((pack, frame)) = creature.frame(s, i) else {
                continue;
            };
            for t in &frame.transforms {
                let b = &pack.base[usize::from(t.transform)];
                if !matches!(b.kind, kind::TRANSLATE | kind::ROTATE | kind::SCALE) {
                    continue;
                }
                let set: LabelSet = b
                    .labels
                    .iter()
                    .copied()
                    .filter(|l| mesh.contains(l))
                    .collect();
                if set.is_empty() {
                    continue;
                }
                let moved = b.kind == kind::ROTATE && (t.dx, t.dy, t.dz) != (0, 0, 0);
                *out.entry(set).or_insert(0) += usize::from(moved);
            }
        }
    }
    out
}

fn centroid(
    creature: &Creature,
    groups: &Groups,
    labels: &LabelSet,
) -> ([i32; 3], Option<(DVec3, DVec3)>) {
    let (mut sum, mut n) = ([0i64; 3], 0i64);
    let (mut lo, mut hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
    for &l in labels {
        for &v in groups.0.get(usize::from(l)).into_iter().flatten() {
            let p = creature.rest[v];
            for a in 0..3 {
                sum[a] += i64::from(p[a]);
            }
            let p = DVec3::new(p[0].into(), p[1].into(), p[2].into());
            lo = lo.min(p);
            hi = hi.max(p);
            n += 1;
        }
    }
    if n == 0 {
        return ([0; 3], None);
    }
    (
        [
            (sum[0] / n) as i32,
            (sum[1] / n) as i32,
            (sum[2] / n) as i32,
        ],
        Some((lo, hi)),
    )
}

/// Infer the rig. Bones are named by [`PLAYER_REGIONS`] where their labels
/// match the default body, `bone_<n>` otherwise. `overrides` replaces the
/// inferred parent of the candidates it names.
pub fn infer(
    creature: &Creature,
    overrides: Option<&ParentOverride>,
) -> Result<InferredRig, ConvertError> {
    let labels = creature.model.vertex_labels.as_ref().ok_or_else(|| {
        ConvertError::Invalid(format!("{} has no vertex labels to rig", creature.name))
    })?;
    let groups = Groups::new(labels);
    let cands = candidates(creature);
    let sets: Vec<LabelSet> = cands.keys().cloned().collect();
    let n = sets.len();

    // Parent of each candidate: the smallest strict superset (largest set
    // among equally small ones keeps the plan's tie rule meaningful when
    // sizes differ only by labels the mesh lacks).
    let mut parent: Vec<Option<usize>> = vec![None; n];
    for (b, sb) in sets.iter().enumerate() {
        let supers: Vec<usize> = (0..n)
            .filter(|&a| a != b && sets[a].is_superset(sb) && sets[a] != *sb)
            .collect();
        // Transitive reduction: keep supersets with no other superset
        // strictly between them and `b`.
        let minimal: Vec<usize> = supers
            .iter()
            .copied()
            .filter(|&a| {
                !supers
                    .iter()
                    .any(|&c| c != a && sets[a].is_superset(&sets[c]) && sets[a] != sets[c])
            })
            .collect();
        parent[b] = minimal
            .into_iter()
            .max_by_key(|&a| (sets[a].len(), std::cmp::Reverse(a)));
    }
    // Overlapping candidates with no superset: attach to an overlapping,
    // larger, more often rotated candidate whose bounds contain the pivot.
    let pivots: Vec<_> = sets
        .iter()
        .map(|s| centroid(creature, &groups, s))
        .collect();
    for b in 0..n {
        if parent[b].is_some() {
            continue;
        }
        let best = (0..n)
            .filter(|&a| {
                a != b
                    && sets[a].len() > sets[b].len()
                    && !sets[a].is_disjoint(&sets[b])
                    && cands[&sets[a]] > 0
            })
            .filter(|&a| {
                let p = pivots[b].0;
                let p = DVec3::new(p[0].into(), p[1].into(), p[2].into());
                pivots[a]
                    .1
                    .is_some_and(|(lo, hi)| p.cmpge(lo).all() && p.cmple(hi).all())
            })
            .max_by_key(|&a| (sets[a].len(), std::cmp::Reverse(a)));
        parent[b] = best;
    }
    if let Some(table) = overrides {
        let index: BTreeMap<Vec<u8>, usize> = sets
            .iter()
            .enumerate()
            .map(|(i, s)| (s.iter().copied().collect(), i))
            .collect();
        for (child, par) in table {
            let &c = index.get(child).ok_or_else(|| {
                ConvertError::Invalid(format!("override names no candidate {child:?}"))
            })?;
            parent[c] = if par.is_empty() {
                None
            } else {
                Some(*index.get(par).ok_or_else(|| {
                    ConvertError::Invalid(format!("override names no candidate {par:?}"))
                })?)
            };
        }
    }

    // Order parents first: depth, then size descending, then set.
    let depth = |mut b: usize| {
        let mut d = 0;
        let mut seen = BTreeSet::new();
        while let Some(p) = parent[b] {
            if !seen.insert(p) {
                return None;
            }
            b = p;
            d += 1;
        }
        Some(d)
    };
    let mut order: Vec<usize> = (0..n).collect();
    let depths: Vec<usize> = (0..n)
        .map(|b| depth(b).ok_or_else(|| ConvertError::Invalid("parent cycle".into())))
        .collect::<Result<_, _>>()?;
    order.sort_by(|&a, &b| {
        (depths[a], std::cmp::Reverse(sets[a].len()), &sets[a]).cmp(&(
            depths[b],
            std::cmp::Reverse(sets[b].len()),
            &sets[b],
        ))
    });
    let position: BTreeMap<usize, usize> =
        order.iter().enumerate().map(|(i, &c)| (c, i + 1)).collect();

    let mut bones = vec![InferredBone {
        name: "root".into(),
        labels: LabelSet::new(),
        parent: None,
        pivot: [0; 3],
    }];
    for (i, &c) in order.iter().enumerate() {
        bones.push(InferredBone {
            name: format!("bone_{}", i + 1),
            labels: sets[c].clone(),
            parent: Some(parent[c].map_or(0, |p| position[&p])),
            pivot: pivots[c].0,
        });
    }
    // Each label binds to the smallest candidate containing it.
    let mut label_bone = BTreeMap::new();
    for l in creature.mesh_labels() {
        let deepest = (1..bones.len())
            .filter(|&b| bones[b].labels.contains(&l))
            .min_by_key(|&b| (bones[b].labels.len(), std::cmp::Reverse(b)));
        label_bone.insert(l, deepest.unwrap_or(0));
    }
    name_bones(&mut bones, &label_bone);
    Ok(InferredRig { bones, label_bone })
}

/// Regions of the default 317 body (shared by players and humanoid NPCs),
/// by vertex label. A bone is named after the region its bound labels fall
/// in most, when that region is not already taken.
pub const PLAYER_REGIONS: &[(&str, &[u8])] = &[
    ("head", &[1, 3]),
    ("torso", &[2, 8]),
    ("waist", &[4, 5, 29, 30]),
    ("pelvis", &[39, 41]),
    ("r_upper_arm", &[17, 20, 21]),
    ("r_forearm", &[19]),
    ("r_hand", &[27]),
    ("l_upper_arm", &[23, 25, 26]),
    ("l_forearm", &[22]),
    ("l_hand", &[28]),
    ("r_thigh", &[34, 42]),
    ("r_shin", &[31, 32, 48]),
    ("r_foot", &[45]),
    ("l_thigh", &[35, 40]),
    ("l_shin", &[37, 38, 47]),
    ("l_foot", &[46]),
];

/// Name bones by [`PLAYER_REGIONS`]; others keep `bone_<n>`.
fn name_bones(bones: &mut [InferredBone], label_bone: &BTreeMap<u8, usize>) {
    let mut taken = BTreeSet::new();
    for (b, bone) in bones.iter_mut().enumerate().skip(1) {
        let bound: Vec<u8> = label_bone
            .iter()
            .filter(|&(_, &x)| x == b)
            .map(|(&l, _)| l)
            .collect();
        let best = PLAYER_REGIONS
            .iter()
            .map(|(name, labels)| (name, bound.iter().filter(|l| labels.contains(l)).count()))
            .filter(|&(name, n)| n > 0 && !taken.contains(name))
            .max_by_key(|&(_, n)| n);
        if let Some((name, _)) = best {
            taken.insert(*name);
            bone.name = (*name).to_owned();
        }
    }
}

impl InferredRig {
    /// Pivot of bone `b` in cache coordinates.
    pub fn pivot(&self, b: usize) -> [i32; 3] {
        self.bones[b].pivot
    }

    /// The `rigs` record. Rest positions are pivots relative to the parent
    /// pivot, converted to y-up.
    pub fn record(&self) -> Result<Rig, ConvertError> {
        let mut label_bone = [UNBOUND; 256];
        for (&l, &b) in &self.label_bone {
            label_bone[usize::from(l)] = b as u8;
        }
        let bones = self
            .bones
            .iter()
            .map(|b| {
                let pp = b.parent.map_or([0; 3], |p| self.bones[p].pivot);
                let d = [
                    b.pivot[0] - pp[0],
                    -(b.pivot[1] - pp[1]),
                    b.pivot[2] - pp[2],
                ];
                Ok(Bone {
                    name_hash: name_hash(&b.name),
                    parent: b.parent.map_or(NO_PARENT, |p| p as u8),
                    rest_pos: d.map(|v| v as i16),
                    rest_rot: [0; 3],
                })
            })
            .collect::<Result<Vec<_>, ConvertError>>()?;
        if bones.len() > 254 {
            return Err(ConvertError::Invalid(format!(
                "{} bones is more than a rig holds",
                bones.len()
            )));
        }
        Ok(Rig { bones, label_bone })
    }

    /// Whether the parent links form one tree under bone 0.
    pub fn is_tree(&self) -> bool {
        self.bones[0].parent.is_none()
            && self
                .bones
                .iter()
                .enumerate()
                .skip(1)
                .all(|(i, b)| b.parent.is_some_and(|p| p < i))
    }
}

/// FNV-1a of a bone name, used as the rig record's `nameHash`.
pub fn name_hash(s: &str) -> u32 {
    s.bytes().fold(0x811c_9dc5, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    })
}
