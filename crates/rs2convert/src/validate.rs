//! Validation: pose the stored rig and clip, skin the creature with them,
//! and see how far that lands from the original client's deformation.
//!
//! [`validate`] takes the creature's scaled rest mesh ([`Creature::rest`]),
//! maps it to pack space, and skins it against a rig and clip that have
//! already been encoded and decoded, so you're checking exactly what's
//! stored. The reference is [`Creature`]'s unscaled mesh deformed by
//! [`deform::deformed`], also mapped to pack space. Errors are plain
//! distances in cache units (the mapping is a rotation, so it doesn't
//! change them).
//!
//! The pose formula, from the records alone:
//!
//! ```text
//! local_b = T(restPos_b + pos) · R(rot)   if bone b has a key at t
//!         = T(restPos_b)                  otherwise
//! world_b = world_parent · local_b        (root: local_b)
//! pivot_b = pivot_parent + restPos_b      (root: restPos_b)
//! skinned = world_b · (v − pivot_b)       b = labelBone[label], 0 if unbound
//! ```

use contentpack::records::anim::Clip;
use contentpack::records::model::PackModel;
use contentpack::records::rig::{NO_PARENT, Rig, UNBOUND};
use glam::{DAffine3, DVec3};
use serde::Serialize;

use crate::clip::local_from_key;
use crate::creature::{Creature, Sequence};
use crate::deform::{self, Groups};

/// Error threshold in cache units. A clip whose worst vertex goes over it
/// gets flagged.
pub const THRESHOLD: f64 = 4.0;

/// One vertex's error at one frame.
#[derive(Debug, Clone, Serialize)]
pub struct WorstVertex {
    /// Vertex index in the merged mesh.
    pub vertex: usize,
    /// The vertex's label (0 when the mesh has none).
    pub label: u8,
    /// The bone it was skinned by (0 when its label is unbound).
    pub bone: u8,
    /// Distance from the reference position, in cache units.
    pub error: f64,
}

/// The error for one frame of a clip.
#[derive(Debug, Clone, Serialize)]
pub struct FrameReport {
    /// Frame index in the sequence.
    pub frame: usize,
    /// Largest vertex error.
    pub max: f64,
    /// Mean over all vertices.
    pub mean: f64,
    /// Up to five worst vertices, largest error first. Equal errors stay in
    /// vertex order.
    pub worst: Vec<WorstVertex>,
}

/// The error for a whole clip.
#[derive(Debug, Clone, Serialize)]
pub struct ClipReport {
    /// The sequence id the clip came from.
    pub sequence: u32,
    /// Largest frame `max`.
    pub max: f64,
    /// Mean of the frame means.
    pub mean: f64,
    /// Whether `max` is over [`THRESHOLD`].
    pub flagged: bool,
    /// Per-frame reports. Frames that couldn't be found are left out.
    pub frames: Vec<FrameReport>,
}

/// World transforms of every bone at frame `t`, from the records alone.
/// A bone with no key at `t` sits at its rest offset, unrotated. Assumes
/// parents come before children, which a decoded rig guarantees.
pub fn pose(rig: &Rig, clip: &Clip, t: u8) -> Vec<DAffine3> {
    let mut world: Vec<DAffine3> = Vec::with_capacity(rig.bones.len());
    for (b, bone) in rig.bones.iter().enumerate() {
        let key = clip
            .keys
            .iter()
            .find(|k| k.t == t && usize::from(k.bone) == b);
        let rest = DVec3::new(
            bone.rest_pos[0].into(),
            bone.rest_pos[1].into(),
            bone.rest_pos[2].into(),
        );
        let local = key.map_or(DAffine3::from_translation(rest), |k| {
            local_from_key(rest, k.rot, k.pos, k.scale)
        });
        let parent = if bone.parent == NO_PARENT {
            DAffine3::IDENTITY
        } else {
            world[usize::from(bone.parent)]
        };
        world.push(parent * local);
    }
    world
}

/// A pack model's vertices (pack space) at frame `t` of a clip, skinned by
/// the rig: each vertex follows the bone its label binds to (bone 0 when
/// unbound or unlabelled).
pub fn skinned(model: &PackModel, rig: &Rig, clip: &Clip, t: u8) -> Vec<[f32; 3]> {
    let world = pose(rig, clip, t);
    let pivots = rest_pivots(rig);
    model
        .vertices
        .iter()
        .zip(vertex_bones(model, rig))
        .map(|(p, b)| {
            let rest = DVec3::new(p[0].into(), p[1].into(), p[2].into());
            let q = world[b].transform_point3(rest - pivots[b]);
            [q.x as f32, q.y as f32, q.z as f32]
        })
        .collect()
}

/// The bone each of a pack model's vertices follows: the one its label
/// binds to, or bone 0 when it's unbound or unlabelled.
pub fn vertex_bones(model: &PackModel, rig: &Rig) -> Vec<usize> {
    (0..model.vertices.len())
        .map(|v| {
            let label = model.labels.as_ref().map_or(0, |l| l[v]);
            match rig.label_bone[usize::from(label)] {
                UNBOUND => 0,
                b => usize::from(b),
            }
        })
        .collect()
}

/// Rest-pose world positions of the bones (the pivots), from the records.
pub fn rest_pivots(rig: &Rig) -> Vec<DVec3> {
    let mut out: Vec<DVec3> = Vec::with_capacity(rig.bones.len());
    for bone in &rig.bones {
        let rest = DVec3::new(
            bone.rest_pos[0].into(),
            bone.rest_pos[1].into(),
            bone.rest_pos[2].into(),
        );
        let parent = if bone.parent == NO_PARENT {
            DVec3::ZERO
        } else {
            out[usize::from(bone.parent)]
        };
        out.push(parent + rest);
    }
    out
}

/// Compare a clip against the reference deformation, frame by frame.
///
/// `rig` and `clip` should be decoded records, so the report covers exactly
/// what's stored. See the module docs for what's skinned and compared.
pub fn validate(creature: &Creature, rig: &Rig, clip: &Clip, s: &Sequence) -> ClipReport {
    let labels = creature.model.vertex_labels.clone().unwrap_or_default();
    let groups = Groups::new(&labels);
    let pivots = rest_pivots(rig);
    let mut frames = Vec::new();
    for i in 0..s.def.frames.len() {
        let Some((pack, frame)) = creature.frame(s, i) else {
            continue;
        };
        let reference =
            deform::deformed(&creature.model, &groups, &pack.base, frame, creature.scale);
        let world = pose(rig, clip, i as u8);
        let mut errors: Vec<WorstVertex> = creature
            .rest
            .iter()
            .enumerate()
            .map(|(v, p)| {
                let label = labels.get(v).copied().unwrap_or(0);
                let bone = rig.label_bone[usize::from(label)];
                let b = if bone == contentpack::records::rig::UNBOUND {
                    0
                } else {
                    usize::from(bone)
                };
                let rest = crate::space::to_pack_vec(*p);
                let skinned = world[b].transform_point3(rest - pivots[b]);
                let r = reference[v];
                let want = crate::space::to_pack_vec(r);
                WorstVertex {
                    vertex: v,
                    label,
                    bone: b as u8,
                    error: skinned.distance(want),
                }
            })
            .collect();
        let n = errors.len().max(1) as f64;
        let mean = errors.iter().map(|e| e.error).sum::<f64>() / n;
        errors.sort_by(|a, b| b.error.total_cmp(&a.error));
        let max = errors.first().map_or(0.0, |e| e.error);
        errors.truncate(5);
        frames.push(FrameReport {
            frame: i,
            max,
            mean,
            worst: errors,
        });
    }
    let max = frames.iter().map(|f| f.max).fold(0.0, f64::max);
    let mean = frames.iter().map(|f| f.mean).sum::<f64>() / frames.len().max(1) as f64;
    ClipReport {
        sequence: s.def.id,
        max,
        mean,
        flagged: max > THRESHOLD,
        frames,
    }
}
