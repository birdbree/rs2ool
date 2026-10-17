//! Rig inference from frame-base label sets.
//!
//! The 317 cache has no skeletons, just frame-base slots that each move a
//! set of vertex labels. This module turns those sets into a bone tree.
//!
//! - A *bone candidate* is a distinct label set moved by a translate,
//!   rotate or scale transform that the creature's frames use, cut down to
//!   the labels its mesh has. Candidates are kept sorted by set, and
//!   "lowest index" below means first in that order.
//! - Parents: for each candidate `B`, take its strict supersets, keep the
//!   inclusion-minimal ones (transitive reduction), and pick the largest of
//!   those. Ties go to the lowest candidate index.
//! - A candidate with no superset can still hang off a larger, overlapping,
//!   rotated candidate whose bounding box holds its pivot (co-movement).
//! - An optional override table replaces parents after both rules.
//! - A root bone sits above everything; parentless candidates go under it.
//! - Each label binds to the smallest candidate containing it (the deepest
//!   bone), or to the root when no candidate has it.
//! - Bones get names from [`PLAYER_REGIONS`] where their bound labels
//!   allow, `bone_<n>` otherwise.

use std::collections::{BTreeMap, BTreeSet};

use contentpack::records::rig::{Bone, NO_PARENT, Rig, UNBOUND};
use glam::DVec3;
use rs2cache::types::anim::kind;
use serde::Serialize;

use crate::ConvertError;
use crate::creature::Creature;
use crate::deform::Groups;

/// A set of vertex labels, sorted.
pub type LabelSet = BTreeSet<u8>;

/// An inferred bone.
#[derive(Debug, Clone, Serialize)]
pub struct InferredBone {
    /// `root`, a region name from [`PLAYER_REGIONS`], or `bone_<n>`.
    pub name: String,
    /// The labels the bone's transforms move. Empty for the root.
    pub labels: LabelSet,
    /// Index of the parent bone, always lower than this one's. `None` only
    /// for the root.
    pub parent: Option<usize>,
    /// Pivot in cache coordinates (y down): the centroid of the bone's
    /// vertices in the rest mesh.
    pub pivot: [i32; 3],
}

/// The inferred rig: bones ordered parents first, bone 0 the root.
#[derive(Debug, Clone, Serialize)]
pub struct InferredRig {
    /// The bones, parents before children. Bone 0 is the root.
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
/// the mesh's labels, with how many non-zero rotations each set gets.
/// Sets that only translate or scale are in there with a count of 0.
fn candidates(creature: &Creature) -> BTreeMap<LabelSet, usize> {
    let mesh = creature.mesh_labels();
    let mut out: BTreeMap<LabelSet, usize> = BTreeMap::new();
    for s in &creature.sequences {
        for i in 0..s.def.frames.len() {
            let Some((pack, frame)) = creature.frame(s, i) else {
                continue;
            };
            for t in &frame.transforms {
                // A slot past the frame base is skipped, as deform and clip
                // skip it; decoded packs never have one, built ones might.
                let Some(b) = pack.base.get(usize::from(t.transform)) else {
                    continue;
                };
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
                // Only a rotation by a non-zero angle bumps the count.
                let moved = b.kind == kind::ROTATE && (t.dx, t.dy, t.dz) != (0, 0, 0);
                *out.entry(set).or_insert(0) += usize::from(moved);
            }
        }
    }
    out
}

/// The integer centroid of `labels`' vertices in the rest mesh, and their
/// bounding box as `(lo, hi)`. `([0; 3], None)` when no vertex has any of
/// the labels.
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

/// Infer the rig.
///
/// The region names in [`PLAYER_REGIONS`] are tried on every creature,
/// humanoid or not. A bone that ends up with no bound labels in a free
/// region keeps `bone_<n>`. `overrides` replaces the inferred parent of
/// each candidate it names.
///
/// Fails when the mesh has no vertex labels, when an override names a
/// label set that isn't a candidate, or when the parents form a cycle
/// (only possible through overrides).
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

    // Parent of each candidate: of its inclusion-minimal strict supersets,
    // the one with the most labels, ties to the lowest candidate index.
    let mut parent: Vec<Option<usize>> = vec![None; n];
    for (b, sb) in sets.iter().enumerate() {
        // Every candidate that strictly contains b.
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
        // max_by_key keeps the last maximum, so Reverse(a) makes the lowest
        // index win a size tie.
        parent[b] = minimal
            .into_iter()
            .max_by_key(|&a| (sets[a].len(), std::cmp::Reverse(a)));
    }
    // Candidates still without a parent: attach to the largest candidate
    // that's bigger, shares a label, is rotated at least once, and whose
    // bounding box holds this one's pivot. Ties to the lowest index.
    // pivots[i] is (centroid, bounding box) for candidate i.
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
            // b's pivot inside a's bounding box, edges included.
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
        // Sorted label list -> candidate index, to look override entries up.
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
    // depth(b) counts parent steps up to a parentless candidate, or gives
    // None if it walks into a cycle.
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
    // Candidate index -> bone index. Bone 0 is the root, so candidates
    // start at 1.
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
    // Each label binds to the smallest bone containing it. On a size tie
    // min_by_key keeps the first minimum, so Reverse(b) picks the later
    // bone.
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

/// Named regions of the default 317 body, by vertex label.
///
/// Bones are named from this table on every creature. Each bone, in order,
/// takes the free region its bound labels fall in most often (later
/// regions win ties). A non-humanoid that happens to use these labels gets
/// these names too, so treat them as hints.
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

/// Name bones by [`PLAYER_REGIONS`]. Bones with no bound label in a free
/// region keep `bone_<n>`; the root keeps `root`.
fn name_bones(bones: &mut [InferredBone], label_bone: &BTreeMap<u8, usize>) {
    let mut taken = BTreeSet::new();
    for (b, bone) in bones.iter_mut().enumerate().skip(1) {
        // The labels bound to this bone.
        let bound: Vec<u8> = label_bone
            .iter()
            .filter(|&(_, &x)| x == b)
            .map(|(&l, _)| l)
            .collect();
        // Region with the most bound labels, among the ones still free.
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

    /// The `rigs` record.
    ///
    /// Rest positions are each pivot relative to its parent's, in pack
    /// space, cast to `i16` with no range check. Rest rotations are zero.
    /// Labels the mesh doesn't use are left [`UNBOUND`]. Fails with more
    /// than 254 bones.
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
                let d = crate::space::to_pack([
                    b.pivot[0] - pp[0],
                    b.pivot[1] - pp[1],
                    b.pivot[2] - pp[2],
                ]);
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

/// 32-bit FNV-1a of a bone name, stored as the rig record's `nameHash`.
pub fn name_hash(s: &str) -> u32 {
    s.bytes().fold(0x811c_9dc5, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use rs2cache::types::anim::{BaseTransform, Frame, FramePack, FrameTransform};
    use rs2cache::types::config::{SeqDef, SeqFrame};
    use rs2cache::types::model::Model;

    use super::*;
    use crate::creature::Sequence;

    /// A one-triangle creature whose only frame uses base slot 0 (a
    /// rotation of label 1) and also names `extra`, which may be past the
    /// two-slot base.
    fn creature(extra: u8) -> Creature {
        let model = Model {
            vertices: vec![[0, 0, 0], [10, 0, 0], [0, 10, 0]],
            faces: vec![[0, 1, 2]],
            face_colours: vec![0],
            face_types: None,
            priority: 0,
            face_priorities: None,
            face_alphas: None,
            face_labels: None,
            vertex_labels: Some(vec![1, 1, 1]),
            textured_faces: Vec::new(),
        };
        let slot = |transform| FrameTransform {
            transform,
            dx: 16,
            dy: 0,
            dz: 0,
        };
        let pack = FramePack {
            base: vec![
                BaseTransform {
                    kind: kind::ROTATE,
                    labels: vec![1],
                },
                BaseTransform {
                    kind: kind::ORIGIN,
                    labels: vec![1],
                },
            ],
            frames: vec![Frame {
                id: 7,
                delay: 1,
                transforms: vec![slot(0), slot(extra)],
                alpha: false,
            }],
        };
        let def = SeqDef {
            id: 0,
            frames: vec![SeqFrame {
                frame: 7,
                secondary: None,
                delay: 1,
            }],
            loop_offset: None,
            interleave_labels: None,
            stretches: false,
            priority: 5,
            offhand_override: None,
            mainhand_override: None,
            max_loops: 99,
            animating_precedence: 0,
            walking_precedence: 0,
            replay_mode: 2,
            unknown_opcodes: Vec::new(),
        };
        let seq = Sequence {
            def,
            packs: vec![0],
        };
        Creature::new(
            "test".into(),
            model,
            [128; 3],
            vec![seq],
            BTreeMap::from([(0, pack)]),
        )
    }

    #[test]
    fn a_slot_past_the_frame_base_is_skipped() {
        let rig = infer(&creature(9), None).expect("no panic, and a rig");
        // The root plus the one rotated label set; slot 9 adds nothing.
        assert_eq!(rig.bones.len(), 2);
        assert_eq!(rig.bones[1].labels, LabelSet::from([1]));
        // Same result as a frame that only uses real slots.
        assert_eq!(infer(&creature(0), None).unwrap().bones.len(), 2);
    }
}
