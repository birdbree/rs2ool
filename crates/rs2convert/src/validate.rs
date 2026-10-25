//! Validation: pose the stored rig and clip, skin the creature with them,
//! and see how far that lands from the original client's deformation.
//!
//! [`validate`] takes the creature's scaled rest mesh ([`Creature::rest`]),
//! maps it to pack space, and skins it against a rig and clip that have
//! already been encoded and decoded, so you're checking exactly what's
//! stored. The reference is [`Creature::reference`], the original client's
//! deformation of the unscaled mesh, scaled, also mapped to pack space. Errors are plain
//! distances in cache units (the mapping is a rotation, so it doesn't
//! change them).
//!
//! The pose formula, from the records alone:
//!
//! ```text
//! local_b = T(restPos_b + pos) · R(rot) · S(scale / 256)
//!                                         if bone b has a key at t
//!         = T(restPos_b)                  otherwise
//! world_b = world_parent · local_b        (root: local_b)
//! pivot_b = pivot_parent + restPos_b      (root: restPos_b)
//! skinned = world_b · (v − pivot_b)       b = labelBone[label], 0 if unbound
//! ```

use contentpack::records::anim::{Clip, Key};
use contentpack::records::model::PackModel;
use contentpack::records::rig::{NO_PARENT, Rig, UNBOUND};
use glam::{DAffine3, DVec3};
use serde::Serialize;

use crate::clip::local_from_key;
use crate::creature::{Creature, Sequence};

/// Error threshold in cache units. A clip whose worst vertex goes over it
/// gets flagged.
pub const THRESHOLD: f64 = 4.0;

/// Error threshold for a clip mixed with a movement clip: twice
/// [`THRESHOLD`], since the mix carries both clips' error.
pub const MIX_THRESHOLD: f64 = 2.0 * THRESHOLD;

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
    /// For a clip with moving bones, how it mixes with each of the
    /// creature's movement clips.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub moving: Vec<MovingReport>,
}

/// The error of a clip mixed with one movement clip, over every pair of
/// their frames.
#[derive(Debug, Clone, Serialize)]
pub struct MovingReport {
    /// The movement sequence's id.
    pub sequence: u32,
    /// Largest vertex error over every pair.
    pub max: f64,
    /// Mean over every pair of the pair's mean error.
    pub mean: f64,
    /// Whether `max` is over [`MIX_THRESHOLD`].
    pub flagged: bool,
}

/// World transforms of every bone at frame `t`, from the records alone.
/// A bone with no key at `t` sits at its rest offset, unrotated. Assumes
/// parents come before children, which a decoded rig guarantees.
pub fn pose(rig: &Rig, clip: &Clip, t: u8) -> Vec<DAffine3> {
    pose_by(rig, |b| {
        clip.keys
            .iter()
            .find(|k| k.t == t && usize::from(k.bone) == b)
    })
}

/// World transforms of every bone at frame `t` of `clip` while the
/// creature moves, at frame `tm` of movement clip `moving`: the bones in
/// `clip.moving` take their key from `moving`'s keys, the rest from
/// `clip`'s mix keys, each child under its parent's mixed transform.
pub fn pose_moving(rig: &Rig, clip: &Clip, t: u8, moving: &Clip, tm: u8) -> Vec<DAffine3> {
    pose_by(rig, |b| {
        let (keys, t) = if clip.moving.contains(&(b as u8)) {
            (&moving.keys, tm)
        } else {
            (&clip.mix_keys, t)
        };
        keys.iter().find(|k| k.t == t && usize::from(k.bone) == b)
    })
}

/// World transforms from each bone's key, or its rest offset without one.
fn pose_by<'a>(rig: &Rig, key: impl Fn(usize) -> Option<&'a Key>) -> Vec<DAffine3> {
    let mut world: Vec<DAffine3> = Vec::with_capacity(rig.bones.len());
    for (b, bone) in rig.bones.iter().enumerate() {
        let rest = DVec3::new(
            bone.rest_pos[0].into(),
            bone.rest_pos[1].into(),
            bone.rest_pos[2].into(),
        );
        let local = key(b).map_or(DAffine3::from_translation(rest), |k| {
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
    skin_model(model, rig, pose(rig, clip, t))
}

/// [`skinned`] mixed with a movement clip as [`pose_moving`] mixes them.
pub fn skinned_moving(
    model: &PackModel,
    rig: &Rig,
    clip: &Clip,
    t: u8,
    moving: &Clip,
    tm: u8,
) -> Vec<[f32; 3]> {
    skin_model(model, rig, pose_moving(rig, clip, t, moving, tm))
}

fn skin_model(model: &PackModel, rig: &Rig, world: Vec<DAffine3>) -> Vec<[f32; 3]> {
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

/// The creature's scaled rest mesh skinned by the rig posed as `world`,
/// in pack space, with each vertex's label and bone.
fn skin_rest<'a>(
    creature: &'a Creature,
    rig: &'a Rig,
    world: Vec<DAffine3>,
) -> impl Iterator<Item = (DVec3, u8, usize)> + 'a {
    let labels = creature.model.vertex_labels.as_deref().unwrap_or_default();
    let pivots = rest_pivots(rig);
    creature.rest.iter().enumerate().map(move |(v, p)| {
        let label = labels.get(v).copied().unwrap_or(0);
        let b = match rig.label_bone[usize::from(label)] {
            UNBOUND => 0,
            b => usize::from(b),
        };
        let rest = crate::space::to_pack_vec(*p);
        (world[b].transform_point3(rest - pivots[b]), label, b)
    })
}

/// The creature's vertices at frame `t` of a clip, skinned by the rig
/// the way [`validate`] skins them, back in cache space and rounded: the
/// converted pose, to draw beside [`Creature::posed`].
pub fn converted(creature: &Creature, rig: &Rig, clip: &Clip, t: u8) -> Vec<[i32; 3]> {
    to_cache(skin_rest(creature, rig, pose(rig, clip, t)))
}

/// [`converted`] mixed with a movement clip as [`pose_moving`] mixes them:
/// to draw beside [`Creature::posed_moving`].
pub fn converted_moving(
    creature: &Creature,
    rig: &Rig,
    clip: &Clip,
    t: u8,
    moving: &Clip,
    tm: u8,
) -> Vec<[i32; 3]> {
    to_cache(skin_rest(
        creature,
        rig,
        pose_moving(rig, clip, t, moving, tm),
    ))
}

fn to_cache(skinned: impl Iterator<Item = (DVec3, u8, usize)>) -> Vec<[i32; 3]> {
    skinned
        .map(|(p, _, _)| {
            let c = p * crate::space::FLIP;
            [c.x, c.y, c.z].map(|v| v.round() as i32)
        })
        .collect()
}

/// Compare a clip with moving bones, mixed with movement clip `moving`
/// (converted from sequence `m`), against the original client's mix of
/// the two sequences, for every pair of their frames.
pub fn validate_moving(
    creature: &Creature,
    rig: &Rig,
    clip: &Clip,
    s: &Sequence,
    moving: &Clip,
    m: &Sequence,
) -> MovingReport {
    let (mut max, mut sum, mut pairs) = (0.0f64, 0.0f64, 0usize);
    for i in 0..s.def.frames.len() {
        for j in 0..m.def.frames.len() {
            let Some(reference) = creature.reference_moving(s, i, m, j) else {
                continue;
            };
            let world = pose_moving(rig, clip, i as u8, moving, j as u8);
            let (mut pair_max, mut pair_sum) = (0.0f64, 0.0f64);
            for (v, (p, _, _)) in skin_rest(creature, rig, world).enumerate() {
                let e = p.distance(crate::space::to_pack_vec(reference[v]));
                pair_max = pair_max.max(e);
                pair_sum += e;
            }
            max = max.max(pair_max);
            sum += pair_sum / creature.rest.len().max(1) as f64;
            pairs += 1;
        }
    }
    MovingReport {
        sequence: m.def.id,
        max,
        mean: sum / pairs.max(1) as f64,
        flagged: max > MIX_THRESHOLD,
    }
}

/// Compare a clip against the reference deformation, frame by frame.
///
/// `rig` and `clip` should be decoded records, so the report covers exactly
/// what's stored. See the module docs for what's skinned and compared.
pub fn validate(creature: &Creature, rig: &Rig, clip: &Clip, s: &Sequence) -> ClipReport {
    let mut frames = Vec::new();
    for i in 0..s.def.frames.len() {
        let Some(reference) = creature.reference(s, i) else {
            continue;
        };
        let mut errors: Vec<WorstVertex> = skin_rest(creature, rig, pose(rig, clip, i as u8))
            .enumerate()
            .map(|(v, (skinned, label, b))| WorstVertex {
                vertex: v,
                label,
                bone: b as u8,
                error: skinned.distance(crate::space::to_pack_vec(reference[v])),
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
        moving: Vec::new(),
    }
}
