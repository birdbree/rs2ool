//! Animation conversion: each frame of a sequence becomes one pose key per
//! bone.
//!
//! The frame's ops are replayed the way the original client applies them.
//! Next to the integer reference deformation (whose origins are used as
//! is), every bone collects the affine transform of the ops that move its
//! whole label set. Each bone's transform relative to its parent is then
//! split into a per-axis scale, a 317-convention rotation (1/256 turn,
//! applied z, x, y) and a translation. Bones go parents first, each against its parent's
//! *quantised* transform, so rounding doesn't pile up down the chain.
//!
//! Pack space is y up: `P = diag(1, −1, −1)` ([`FLIP`]) maps cache
//! coordinates to pack coordinates, and a key rotation `(rx, ry, rz)` means
//! `P · R317 · P`.
//!
//! A key's scale is per pack-space axis, about the bone's pivot and before
//! its rotation, so a scale op followed by a rotation of the same bone is
//! exact, but a non-uniform scale that's turned before it's scaled leaves a
//! shear no key can hold, and that shows up as validation error. A
//! sequence frame with a secondary frame converts to one key per bone for
//! both, applied one after the other, as the original client draws them.

use std::f64::consts::TAU;

use contentpack::records::anim::{Clip, Fade, Key, NO_LOOP, SCALE_ONE};
use glam::{DAffine3, DMat3, DVec3};
use rs2cache::types::anim::{Frame, FramePack, kind};

use crate::ConvertError;
use crate::creature::{Creature, Sequence};
use crate::deform::{self, Groups};
use crate::rig::InferredRig;

use crate::space::FLIP;

fn flip_affine(a: DAffine3) -> DAffine3 {
    let p = DAffine3::from_scale(FLIP);
    p * a * p
}

fn v3(p: [i32; 3]) -> DVec3 {
    DVec3::new(p[0].into(), p[1].into(), p[2].into())
}

/// The original client's rotation for 8-bit angles, as a cache-space
/// matrix: `Ry · Rx · Rz'`.
///
/// `Rz'` turns the opposite way to the textbook z rotation; that's the
/// original client's sign. Sines and cosines come from the same truncated
/// tables as [`deform::rotate`], so the two agree to within rounding.
pub fn rotation_317(rx: i32, ry: i32, rz: i32) -> DMat3 {
    // (sin, cos) of a 1/256-turn angle, via the 2048-entry tables.
    let sc = |v: i32| {
        let k = ((v & 0xff) * 8) as usize;
        (
            f64::from(deform::sin(k)) / 65536.0,
            f64::from(deform::cos(k)) / 65536.0,
        )
    };
    let (sa, ca) = sc(rx);
    let (sb, cb) = sc(ry);
    let (sz, cz) = sc(rz);
    // Columns of each matrix (glam is column-major).
    let rz = DMat3::from_cols(DVec3::new(cz, -sz, 0.0), DVec3::new(sz, cz, 0.0), DVec3::Z);
    let rxm = DMat3::from_cols(DVec3::X, DVec3::new(0.0, ca, sa), DVec3::new(0.0, -sa, ca));
    let rym = DMat3::from_cols(DVec3::new(cb, 0.0, -sb), DVec3::Y, DVec3::new(sb, 0.0, cb));
    rym * rxm * rz
}

/// The 8-bit 317 angles `[rx, ry, rz]` of a cache-space rotation
/// `Ry · Rx · Rz'`, each rounded to the nearest 1/256 turn.
pub fn angles_317(m: DMat3) -> [u8; 3] {
    // m.col(c)[r] is row r, column c.
    let at = |r: usize, c: usize| m.col(c)[r];
    // Row 1, column 2 is −sin(x angle); the rest come from the entries that
    // pair sin and cos of y (column 2) and of z (row 1).
    let a = (-at(1, 2)).clamp(-1.0, 1.0).asin();
    let (b, c) = if a.cos().abs() > 1e-6 {
        (at(0, 2).atan2(at(2, 2)), (-at(1, 0)).atan2(at(1, 1)))
    } else {
        // Gimbal lock: fold z into y.
        ((-at(2, 0)).atan2(at(0, 0)), 0.0)
    };
    // Radians to 1/256 turns, wrapped into 0–255.
    [a, b, c].map(|t| ((t / TAU * 256.0).round() as i64).rem_euclid(256) as u8)
}

/// A key's rotation as a pack-space matrix: `P · R317 · P`.
pub fn key_rotation(rot: [u8; 3]) -> DMat3 {
    let p = DMat3::from_diagonal(FLIP);
    p * rotation_317(rot[0].into(), rot[1].into(), rot[2].into()) * p
}

/// Per-bone cache-space transforms from the scaled rest mesh to the
/// deformed (then scaled) mesh, for one sequence frame: its frames applied
/// one after another (see [`Creature::frames`]), each starting from a zero
/// origin. Bone 0 (root) is identity.
///
/// An op counts towards a bone only when its slot moves every one of the
/// bone's labels. Origins come from the integer reference, advanced one op
/// at a time, so they match the original client exactly.
pub fn bone_transforms(
    creature: &Creature,
    rig: &InferredRig,
    groups: &Groups,
    frames: &[(&FramePack, &Frame)],
) -> Vec<DAffine3> {
    let mesh = creature.mesh_labels();
    // verts: the integer reference, advanced op by op. world[b]: bone b's
    // accumulated transform so far.
    let mut verts = creature.model.vertices.clone();
    let mut world = vec![DAffine3::IDENTITY; rig.bones.len()];
    for (pack, frame) in frames {
        let mut origin = [0i32; 3];
        for t in &frame.transforms {
            let Some(b) = pack.base.get(usize::from(t.transform)) else {
                continue;
            };
            let d = [t.dx, t.dy, t.dz];
            // The slot's labels the mesh actually has.
            let set: std::collections::BTreeSet<u8> = b
                .labels
                .iter()
                .copied()
                .filter(|l| mesh.contains(l))
                .collect();
            // The op as an affine map. Rotate and scale go about the current
            // origin: move it to (0, 0, 0), apply, move back.
            let op = match b.kind {
                kind::ORIGIN => None,
                kind::TRANSLATE => Some(DAffine3::from_translation(v3(d))),
                kind::ROTATE => {
                    let o = v3(origin);
                    Some(
                        DAffine3::from_translation(o)
                            * DAffine3::from_mat3(rotation_317(d[0], d[1], d[2]))
                            * DAffine3::from_translation(-o),
                    )
                }
                kind::SCALE => {
                    let o = v3(origin);
                    Some(
                        DAffine3::from_translation(o)
                            * DAffine3::from_scale(v3(d) / 128.0)
                            * DAffine3::from_translation(-o),
                    )
                }
                _ => None,
            };
            if let Some(op) = op {
                for (i, bone) in rig.bones.iter().enumerate().skip(1) {
                    if !bone.labels.is_empty() && bone.labels.is_subset(&set) {
                        world[i] = op * world[i];
                    }
                }
            }
            // Advance the integer reference by this one op, for later origins.
            let single = Frame {
                id: frame.id,
                delay: frame.delay,
                alpha: false,
                transforms: vec![*t],
            };
            if b.kind == kind::ORIGIN {
                // Same integer centroid plus offset as deform::apply_frame.
                let (mut sum, mut n) = ([0i32; 3], 0i32);
                for &l in &b.labels {
                    for &v in groups.0.get(usize::from(l)).into_iter().flatten() {
                        for a in 0..3 {
                            sum[a] += verts[v][a];
                        }
                        n += 1;
                    }
                }
                origin = if n > 0 {
                    [sum[0] / n + d[0], sum[1] / n + d[1], sum[2] / n + d[2]]
                } else {
                    d
                };
            } else {
                // apply_frame starts each frame with a zero origin, so feed it
                // the current one along with the op.
                apply_with_origin(&mut verts, groups, &pack.base, &single, origin);
            }
        }
    }
    // The rest mesh has the scale baked in, so conjugate: S · A · S⁻¹.
    let scale = DAffine3::from_scale(v3(creature.scale) / 128.0);
    world.iter().map(|w| scale * *w * scale.inverse()).collect()
}

/// Apply frame `f` to `verts` starting from `origin` instead of
/// (0, 0, 0).
fn apply_with_origin(
    verts: &mut [[i32; 3]],
    groups: &Groups,
    base: &[rs2cache::types::anim::BaseTransform],
    f: &Frame,
    origin: [i32; 3],
) {
    // Prepend a synthetic origin op on an empty label set, which sets the
    // origin to exactly its offset.
    let mut base = base.to_vec();
    base.push(rs2cache::types::anim::BaseTransform {
        kind: kind::ORIGIN,
        labels: Vec::new(),
    });
    let o = (base.len() - 1) as u8;
    let mut transforms = vec![rs2cache::types::anim::FrameTransform {
        transform: o,
        dx: origin[0],
        dy: origin[1],
        dz: origin[2],
    }];
    transforms.extend_from_slice(&f.transforms);
    deform::apply_frame(
        verts,
        groups,
        &base,
        &Frame {
            transforms,
            ..f.clone()
        },
    );
}

/// Pack-space rest pivot of bone `b`.
fn pivot(rig: &InferredRig, b: usize) -> DVec3 {
    v3(rig.pivot(b)) * FLIP
}

/// The vertices bound to each bone: `(rest, reference)` pairs in pack
/// space for one frame.
pub type Bound = Vec<Vec<(DVec3, DVec3)>>;

/// Group a frame's vertices by the bone their label binds to (the root
/// for unlabelled or unbound vertices), in pack space.
pub fn bound(creature: &Creature, rig: &InferredRig, reference: &[[i32; 3]]) -> Bound {
    let mut out = vec![Vec::new(); rig.bones.len()];
    let labels = creature.model.vertex_labels.as_deref().unwrap_or_default();
    for (v, p) in creature.rest.iter().enumerate() {
        let b = labels
            .get(v)
            .and_then(|l| rig.label_bone.get(l))
            .copied()
            .unwrap_or(0);
        out[b].push((v3(*p) * FLIP, v3(reference[v]) * FLIP));
    }
    out
}

/// Keys for frame `t`, one per bone, root included.
///
/// `world` is from [`bone_transforms`]; `bound` holds each bone's vertices
/// at rest and in the reference deformation. Each bone's angles start from
/// its exact transform, rounded. Then those angles and their 26 neighbours
/// (±1 on each axis) are tried, each with the least-squares translation
/// for the bone's vertices, and the one with the smallest worst-vertex
/// error wins. A bone with no vertices just takes the rounded angles and
/// translation.
pub fn frame_keys(rig: &InferredRig, world: &[DAffine3], bound: &Bound, t: u8) -> Vec<Key> {
    let mut quantised: Vec<DAffine3> = Vec::with_capacity(rig.bones.len());
    let mut keys = Vec::with_capacity(rig.bones.len());
    let p = DMat3::from_diagonal(FLIP);
    for (b, bone) in rig.bones.iter().enumerate() {
        // The bone's exact world transform in pack space, as a frame placed
        // at its rest pivot.
        let skin = flip_affine(world[b]);
        let target = skin * DAffine3::from_translation(pivot(rig, b));
        // Relative to where the parent's keys actually put it.
        let parent = bone.parent.map_or(DAffine3::IDENTITY, |p| quantised[p]);
        let local = parent.inverse() * target;
        // Rest offset from the parent's pivot; keys store pos relative to it.
        let rest = pivot(rig, b) - bone.parent.map_or(DVec3::ZERO, |p| pivot(rig, p));
        // The scale per axis is the length of each column of the linear
        // part (M = R · S), quantised to 1/256; the rotation is what's left.
        let cols = [
            local.matrix3.x_axis,
            local.matrix3.y_axis,
            local.matrix3.z_axis,
        ];
        let scale = cols.map(|c| {
            (c.length() * f64::from(SCALE_ONE))
                .round()
                .clamp(1.0, f64::from(u16::MAX)) as u16
        });
        let s = scale_of(scale);
        let unscaled = local.matrix3 * DMat3::from_diagonal(DVec3::ONE / s);
        // Back to cache convention (P · M · P) to read off 317 angles.
        let exact = angles_317(p * unscaled * p);
        let to_i16 = |t: DVec3| {
            [t.x, t.y, t.z].map(|v| v.round().clamp(i16::MIN.into(), i16::MAX.into()) as i16)
        };
        // Pairs of (rest offset from the bone's pivot, reference position
        // in the parent's frame).
        let inv = parent.inverse();
        let pts: Vec<(DVec3, DVec3)> = bound[b]
            .iter()
            .map(|(r, d)| (*r - pivot(rig, b), inv.transform_point3(*d)))
            .collect();
        // (worst-vertex error, angles, pos) of the best try so far.
        let mut best = (f64::INFINITY, exact, to_i16(local.translation - rest));
        if pts.is_empty() {
            best.0 = 0.0;
        }
        let deltas = [-1i32, 0, 1];
        for dx in deltas {
            for dy in deltas {
                for dz in deltas {
                    if pts.is_empty() {
                        break;
                    }
                    let rot = [
                        (i32::from(exact[0]) + dx).rem_euclid(256) as u8,
                        (i32::from(exact[1]) + dy).rem_euclid(256) as u8,
                        (i32::from(exact[2]) + dz).rem_euclid(256) as u8,
                    ];
                    let r = key_rotation(rot) * DMat3::from_diagonal(s);
                    // With the rotation and scale fixed, the mean of
                    // d − R·S·o is the least-squares translation.
                    let mean =
                        pts.iter().map(|(o, d)| *d - r * *o).sum::<DVec3>() / pts.len() as f64;
                    let pos = to_i16(mean - rest);
                    // Score with the rounded pos, since that's what's stored.
                    let tr = rest + DVec3::new(pos[0].into(), pos[1].into(), pos[2].into());
                    let err = pts
                        .iter()
                        .map(|(o, d)| (r * *o + tr).distance(*d))
                        .fold(0.0, f64::max);
                    if err < best.0 {
                        best = (err, rot, pos);
                    }
                }
            }
        }
        let (_, rot, pos) = best;
        // Children are fitted against this, not the exact transform.
        quantised.push(parent * local_from_key(rest, rot, pos, scale));
        keys.push(Key {
            t,
            bone: b as u8,
            rot,
            pos,
            scale,
        });
    }
    keys
}

/// A key's scale as a multiplier per axis.
fn scale_of(scale: [u16; 3]) -> DVec3 {
    DVec3::new(scale[0].into(), scale[1].into(), scale[2].into()) / f64::from(SCALE_ONE)
}

/// A bone's local transform from its rest offset and key: scale, then
/// rotate by the key angles, then translate by `rest + pos`.
pub fn local_from_key(rest: DVec3, rot: [u8; 3], pos: [i16; 3], scale: [u16; 3]) -> DAffine3 {
    let t = rest + DVec3::new(pos[0].into(), pos[1].into(), pos[2].into());
    DAffine3::from_translation(t)
        * DAffine3::from_mat3(key_rotation(rot) * DMat3::from_diagonal(scale_of(scale)))
}

/// What a run of the original client's alpha steps does to one face's
/// transparency `x` (0–255): `clamp(x + add, lo, hi)`. Each step adds
/// and clamps to 0–255, and a chain of them collapses into this one form,
/// so a single fade per face group and frame reproduces it exactly
/// whatever each face starts at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Ramp {
    add: i32,
    lo: i32,
    hi: i32,
}

impl Ramp {
    const IDENTITY: Self = Self {
        add: 0,
        lo: 0,
        hi: 255,
    };

    /// This ramp followed by one step of `dx`:
    /// `clamp(clamp(x + a, l, h) + s, 0, 255) = clamp(x + a + s, l', h')`
    /// with `l'` and `h'` the bounds moved by `s` and clamped. An `add`
    /// past ±255 does nothing more to a value in 0–255, so it's held there.
    fn step(self, dx: i32) -> Self {
        let s = dx * 8;
        Self {
            add: (self.add + s).clamp(-255, 255),
            lo: (self.lo + s).clamp(0, 255),
            hi: (self.hi + s).clamp(0, 255),
        }
    }

    /// As a fade on opacity (255 − transparency) for `group` at frame `t`.
    fn fade(self, t: u8, group: u8) -> Fade {
        Fade {
            t,
            group,
            add: -self.add as i16,
            lo: (255 - self.hi) as u8,
            hi: (255 - self.lo) as u8,
        }
    }
}

/// The fades of sequence `s`: per frame, one per face group its alpha
/// transforms change (secondary frames included), on the model's face
/// labels. None when the model has no face labels, as the original client
/// fades nothing then.
pub fn fades(creature: &Creature, s: &Sequence) -> Vec<Fade> {
    let Some(labels) = creature.model.face_labels.as_deref() else {
        return Vec::new();
    };
    let present: std::collections::BTreeSet<u8> = labels.iter().copied().collect();
    let mut out = Vec::new();
    for i in 0..s.def.frames.len() {
        let mut ramps = std::collections::BTreeMap::<u8, Ramp>::new();
        for (pack, frame) in creature.frames(s, i).unwrap_or_default() {
            for t in &frame.transforms {
                let Some(b) = pack.base.get(usize::from(t.transform)) else {
                    continue;
                };
                if b.kind != kind::ALPHA {
                    continue;
                }
                // A label listed twice steps its faces twice, as in the
                // original client.
                for l in b.labels.iter().filter(|l| present.contains(l)) {
                    let r = ramps.entry(*l).or_insert(Ramp::IDENTITY);
                    *r = r.step(t.dx);
                }
            }
        }
        out.extend(
            ramps
                .into_iter()
                .filter(|(_, r)| *r != Ramp::IDENTITY)
                .map(|(g, r)| r.fade(i as u8, g)),
        );
    }
    out
}

/// Keys for the bones not in `moving`, for playing over a movement clip:
/// each frame converted from only the transforms the original client keeps
/// of it when it mixes (its [part](deform::interleave_part) without the
/// listed slots), so they hold the pose under the movement's share.
fn mix_keys(
    creature: &Creature,
    rig: &InferredRig,
    groups: &Groups,
    s: &Sequence,
    slots: &[u8],
    moving: &[u8],
) -> Result<Vec<Key>, ConvertError> {
    let mut out = Vec::new();
    for i in 0..s.def.frames.len() {
        let (pack, frame) = creature.frame(s, i).ok_or_else(|| {
            ConvertError::Invalid(format!("sequence {} frame {i} not found", s.def.id))
        })?;
        let part = deform::interleave_part(&pack.base, frame, slots, false);
        let world = bone_transforms(creature, rig, groups, &[(pack, &part)]);
        let mut reference = creature.model.vertices.clone();
        deform::apply_frame(&mut reference, groups, &pack.base, &part);
        deform::apply_scale(&mut reference, creature.scale);
        out.extend(
            frame_keys(rig, &world, &bound(creature, rig, &reference), i as u8)
                .into_iter()
                .filter(|k| !moving.contains(&k.bone)),
        );
    }
    Ok(out)
}

/// The bones a movement clip drives while sequence `s` plays as the
/// creature moves: none unless the sequence has an interleave list.
///
/// The original client takes the listed frame-base slots from the movement
/// sequence's frame and the rest from the action's. A bone goes to the
/// movement clip when the slots that move it and not its parent (its own
/// slots, the ones a local transform holds) include a listed one and no
/// unlisted one the action actually moves; an unlisted slot whose every
/// transform in the action is the identity doesn't count. A bone with no
/// own slots follows its parent. Slots are looked up in the base of the
/// sequence's first frame, as every frame of a sequence shares one in
/// practice.
pub fn moving_bones(creature: &Creature, rig: &InferredRig, s: &Sequence) -> Vec<u8> {
    let Some(slots) = s.interleave() else {
        return Vec::new();
    };
    let Some((pack, _)) = creature.frame(s, 0) else {
        return Vec::new();
    };
    let mesh = creature.mesh_labels();
    // Slots the action moves by something other than the identity.
    let mut used = std::collections::BTreeSet::new();
    for i in 0..s.def.frames.len() {
        for t in creature.frame(s, i).map_or(&[][..], |(_, f)| &f.transforms) {
            let identity = match pack.base.get(usize::from(t.transform)).map(|b| b.kind) {
                Some(kind::SCALE) => (t.dx, t.dy, t.dz) == (128, 128, 128),
                _ => (t.dx, t.dy, t.dz) == (0, 0, 0),
            };
            if !identity {
                used.insert(t.transform);
            }
        }
    }
    // Whether slot `i` moves every one of bone `b`'s labels, as
    // bone_transforms counts it.
    let moves = |i: usize, b: usize| {
        let (Some(slot), Some(bone)) = (pack.base.get(i), rig.bones.get(b)) else {
            return false;
        };
        matches!(slot.kind, kind::TRANSLATE | kind::ROTATE | kind::SCALE)
            && !bone.labels.is_empty()
            && bone
                .labels
                .iter()
                .all(|l| slot.labels.contains(l) || !mesh.contains(l))
    };
    let mut out = vec![false; rig.bones.len()];
    for b in 1..rig.bones.len() {
        let parent = rig.bones[b].parent.unwrap_or(0);
        let own: Vec<usize> = (0..pack.base.len())
            .filter(|&i| moves(i, b) && !moves(i, parent))
            .collect();
        let listed = |i: &usize| slots.contains(&(*i as u8));
        out[b] = if own.is_empty() {
            out[parent]
        } else {
            own.iter().any(listed) && !own.iter().any(|i| !listed(i) && used.contains(&(*i as u8)))
        };
    }
    (0..rig.bones.len())
        .filter(|&b| out[b])
        .map(|b| b as u8)
        .collect()
}

/// Frame lengths in original-client cycles: the sequence's delay, else the
/// frame's own, else 1. That's the same fallback the original client uses.
pub fn delays(creature: &Creature, s: &Sequence) -> Vec<u16> {
    (0..s.def.frames.len())
        .map(|i| {
            let d = s.def.frames[i].delay;
            let d = if d == 0 {
                creature.frame(s, i).map_or(0, |(_, f)| u16::from(f.delay))
            } else {
                d
            };
            d.max(1)
        })
        .collect()
}

/// Convert one sequence into a clip driving rig `rig_id`.
///
/// Fails if the sequence has more than 255 frames or a frame can't be
/// found. Frame start times and the duration saturate at `u16::MAX`.
pub fn convert(
    creature: &Creature,
    rig: &InferredRig,
    s: &Sequence,
    rig_id: u32,
) -> Result<Clip, ConvertError> {
    let labels = creature.model.vertex_labels.as_deref().unwrap_or_default();
    let groups = Groups::new(labels);
    let n = s.def.frames.len();
    if n > 255 {
        return Err(ConvertError::Invalid(format!(
            "sequence {} has {n} frames",
            s.def.id
        )));
    }
    let mut keys = Vec::new();
    for i in 0..n {
        let missing =
            || ConvertError::Invalid(format!("sequence {} frame {i} not found", s.def.id));
        let frames = creature.frames(s, i).ok_or_else(missing)?;
        let world = bone_transforms(creature, rig, &groups, &frames);
        let reference = creature.reference(s, i).ok_or_else(missing)?;
        keys.extend(frame_keys(
            rig,
            &world,
            &bound(creature, rig, &reference),
            i as u8,
        ));
    }
    let moving = moving_bones(creature, rig, s);
    let mix_keys = match s.interleave() {
        Some(slots) if !moving.is_empty() => mix_keys(creature, rig, &groups, s, slots, &moving)?,
        _ => Vec::new(),
    };
    let lengths = delays(creature, s);
    // Running sum of frame lengths; `at` ends up as the clip's duration.
    let mut frame_start = Vec::with_capacity(n);
    let mut at = 0u32;
    for &d in &lengths {
        frame_start.push(at.min(u32::from(u16::MAX)) as u16);
        at += u32::from(d);
    }
    Ok(Clip {
        fades: fades(creature, s),
        moving,
        mix_keys,
        rig: rig_id,
        frame_start,
        duration: at.min(u32::from(u16::MAX)) as u16,
        loop_offset: s.def.loop_offset.unwrap_or(NO_LOOP),
        priority: s.def.priority,
        replay_mode: s.def.replay_mode,
        keys,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angles_round_trip_through_the_317_rotation() {
        for (rx, ry, rz) in [
            (0, 0, 0),
            (10, 0, 0),
            (0, 20, 0),
            (0, 0, 30),
            (12, 200, 7),
            (250, 3, 128),
        ] {
            let m = rotation_317(rx, ry, rz);
            let back = angles_317(m);
            let again = rotation_317(back[0].into(), back[1].into(), back[2].into());
            assert!(
                (m - again).abs_diff_eq(DMat3::ZERO, 1e-3),
                "({rx},{ry},{rz}) -> {back:?}"
            );
        }
    }

    #[test]
    fn rotation_matches_the_integer_client_rotation() {
        let p = [100, -50, 30];
        for (rx, ry, rz) in [(16, 0, 0), (0, 40, 0), (0, 0, 64), (9, 77, 201)] {
            let i = deform::rotate(p, rx, ry, rz);
            let f = rotation_317(rx, ry, rz) * v3(p);
            for a in 0..3 {
                assert!(
                    (f64::from(i[a]) - f[a]).abs() < 3.0,
                    "{rx},{ry},{rz}: {i:?} vs {f:?}"
                );
            }
        }
    }

    #[test]
    fn a_ramp_is_the_client_steps_composed() {
        // Mixed signs, big and small, saturating both ways.
        let runs: [&[i32]; 5] = [
            &[32],
            &[4, -1, 30, -12],
            &[-40, 3],
            &[255, -255, 2],
            &[1; 40],
        ];
        for steps in runs {
            let ramp = steps.iter().fold(Ramp::IDENTITY, |r, &dx| r.step(dx));
            let fade = ramp.fade(0, 0);
            for x in 0..=255u8 {
                let stepped = steps.iter().fold(x, |a, &dx| deform::alpha_step(a, dx));
                assert_eq!(
                    (ramp.add + i32::from(x)).clamp(ramp.lo, ramp.hi),
                    i32::from(stepped),
                    "{steps:?} from {x}"
                );
                // As opacity, through the record's own arithmetic.
                assert_eq!(255 - fade.apply(255 - x), stepped, "{steps:?} from {x}");
            }
        }
    }
}
