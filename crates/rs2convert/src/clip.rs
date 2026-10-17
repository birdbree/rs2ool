//! Animation conversion: each frame of a sequence becomes pose keys per
//! bone.
//!
//! The frame's ops are replayed as the client does. Alongside the integer
//! reference deformation (whose origins are used exactly), every bone
//! accumulates the affine transform of the ops that move its whole label
//! set. Each bone's local transform relative to its parent is then
//! decomposed into a 317-convention rotation (1/256 turn, applied z, x, y)
//! and a translation. Bones are processed parents first against the
//! parent's *quantised* transform, so rounding does not accumulate down the
//! chain.
//!
//! Pack space is y-up: `P = diag(1, −1, 1)` maps cache coordinates to pack
//! coordinates, and a key rotation `(rx, ry, rz)` means `P · R317 · P`.

use std::f64::consts::TAU;

use glam::{DAffine3, DMat3, DVec3};
use rs2cache::types::anim::{Frame, FramePack, kind};
use rs2pack::records::anim::{Clip, Key, NO_LOOP};

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

/// The client's rotation (cache space) for 8-bit angles: `Ry · Rx · Rz'`.
pub fn rotation_317(rx: i32, ry: i32, rz: i32) -> DMat3 {
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

/// The 8-bit 317 angles of a cache-space rotation `Ry · Rx · Rz'`.
pub fn angles_317(m: DMat3) -> [u8; 3] {
    // m.col(c)[r] is row r, column c.
    let at = |r: usize, c: usize| m.col(c)[r];
    let a = (-at(1, 2)).clamp(-1.0, 1.0).asin();
    let (b, c) = if a.cos().abs() > 1e-6 {
        (at(0, 2).atan2(at(2, 2)), (-at(1, 0)).atan2(at(1, 1)))
    } else {
        // Gimbal lock: fold z into y.
        ((-at(2, 0)).atan2(at(0, 0)), 0.0)
    };
    [a, b, c].map(|t| ((t / TAU * 256.0).round() as i64).rem_euclid(256) as u8)
}

/// The key rotation as a pack-space matrix.
pub fn key_rotation(rot: [u8; 3]) -> DMat3 {
    let p = DMat3::from_diagonal(FLIP);
    p * rotation_317(rot[0].into(), rot[1].into(), rot[2].into()) * p
}

/// Per-bone cache-space transforms from the scaled rest mesh to the
/// deformed (then scaled) mesh, for one frame. Bone 0 (root) is identity.
pub fn bone_transforms(
    creature: &Creature,
    rig: &InferredRig,
    groups: &Groups,
    pack: &FramePack,
    frame: &Frame,
) -> Vec<DAffine3> {
    let mesh = creature.mesh_labels();
    let mut verts = creature.model.vertices.clone();
    let mut world = vec![DAffine3::IDENTITY; rig.bones.len()];
    let mut origin = [0i32; 3];
    for t in &frame.transforms {
        let Some(b) = pack.base.get(usize::from(t.transform)) else {
            continue;
        };
        let d = [t.dx, t.dy, t.dz];
        let set: std::collections::BTreeSet<u8> = b
            .labels
            .iter()
            .copied()
            .filter(|l| mesh.contains(l))
            .collect();
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
            // apply_frame resets the origin, so restore it around the op.
            apply_with_origin(&mut verts, groups, &pack.base, &single, origin);
        }
    }
    // The rest mesh has the scale baked in, so conjugate: S · A · S⁻¹.
    let scale = DAffine3::from_scale(v3(creature.scale) / 128.0);
    world.iter().map(|w| scale * *w * scale.inverse()).collect()
}

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

/// Group a frame's vertices by bone, in pack space.
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

/// Keys for one frame. `world` is from [`bone_transforms`]; `bound` holds
/// each bone's vertices at rest and in the reference deformation. Each
/// bone's rotation comes from its exact transform, then the quantised
/// angles and their neighbours are tried, each with the least-squares
/// translation for the bone's vertices, and the best kept.
pub fn frame_keys(rig: &InferredRig, world: &[DAffine3], bound: &Bound, t: u8) -> Vec<Key> {
    let mut quantised: Vec<DAffine3> = Vec::with_capacity(rig.bones.len());
    let mut keys = Vec::with_capacity(rig.bones.len());
    let p = DMat3::from_diagonal(FLIP);
    for (b, bone) in rig.bones.iter().enumerate() {
        let skin = flip_affine(world[b]);
        let target = skin * DAffine3::from_translation(pivot(rig, b));
        let parent = bone.parent.map_or(DAffine3::IDENTITY, |p| quantised[p]);
        let local = parent.inverse() * target;
        let rest = pivot(rig, b) - bone.parent.map_or(DVec3::ZERO, |p| pivot(rig, p));
        let exact = angles_317(p * local.matrix3 * p);
        let to_i16 = |t: DVec3| {
            [t.x, t.y, t.z].map(|v| v.round().clamp(i16::MIN.into(), i16::MAX.into()) as i16)
        };
        // Reference positions in the parent's frame.
        let inv = parent.inverse();
        let pts: Vec<(DVec3, DVec3)> = bound[b]
            .iter()
            .map(|(r, d)| (*r - pivot(rig, b), inv.transform_point3(*d)))
            .collect();
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
                    let r = key_rotation(rot);
                    let mean =
                        pts.iter().map(|(o, d)| *d - r * *o).sum::<DVec3>() / pts.len() as f64;
                    let pos = to_i16(mean - rest);
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
        quantised.push(parent * local_from_key(rest, rot, pos));
        keys.push(Key {
            t,
            bone: b as u8,
            rot,
            pos,
        });
    }
    keys
}

/// A bone's local transform from its rest offset and key.
pub fn local_from_key(rest: DVec3, rot: [u8; 3], pos: [i16; 3]) -> DAffine3 {
    let t = rest + DVec3::new(pos[0].into(), pos[1].into(), pos[2].into());
    DAffine3::from_translation(t) * DAffine3::from_mat3(key_rotation(rot))
}

/// Frame lengths in client cycles: the sequence's delay, else the frame's
/// own, else 1 (as `Animation.method258`).
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
        let (pack, frame) = creature.frame(s, i).ok_or_else(|| {
            ConvertError::Invalid(format!("sequence {} frame {i} not found", s.def.id))
        })?;
        let world = bone_transforms(creature, rig, &groups, pack, frame);
        let reference =
            deform::deformed(&creature.model, &groups, &pack.base, frame, creature.scale);
        keys.extend(frame_keys(
            rig,
            &world,
            &bound(creature, rig, &reference),
            i as u8,
        ));
    }
    let lengths = delays(creature, s);
    let mut frame_start = Vec::with_capacity(n);
    let mut at = 0u32;
    for &d in &lengths {
        frame_start.push(at.min(u32::from(u16::MAX)) as u16);
        at += u32::from(d);
    }
    Ok(Clip {
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
}
