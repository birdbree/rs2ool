//! Validation: re-skin the converted model with the rig and clip records as
//! the client would, and compare with the 317 reference deformation.

use glam::{DAffine3, DVec3};
use rs2pack::records::anim::Clip;
use rs2pack::records::rig::{NO_PARENT, Rig};
use serde::Serialize;

use crate::clip::local_from_key;
use crate::creature::{Creature, Sequence};
use crate::deform::{self, Groups};

/// Error threshold in cache units; a clip above it is flagged.
pub const THRESHOLD: f64 = 4.0;

#[derive(Debug, Clone, Serialize)]
pub struct WorstVertex {
    pub vertex: usize,
    pub label: u8,
    pub bone: u8,
    pub error: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FrameReport {
    pub frame: usize,
    pub max: f64,
    pub mean: f64,
    pub worst: Vec<WorstVertex>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClipReport {
    pub sequence: u32,
    pub max: f64,
    pub mean: f64,
    pub flagged: bool,
    pub frames: Vec<FrameReport>,
}

/// World transforms of every bone at frame `t`, from the records.
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
            local_from_key(rest, k.rot, k.pos)
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

/// Compare a clip against the reference deformation for every frame.
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
                let b = if bone == rs2pack::records::rig::UNBOUND {
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
