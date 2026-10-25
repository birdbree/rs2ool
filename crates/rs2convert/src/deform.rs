//! The original client's frame application, redone in the same integer
//! arithmetic so it matches vertex for vertex.
//!
//! This is the *reference deformation*: converted clips are validated
//! against it, and the explorer's cache playback uses it. A frame is a list
//! of transforms, each pointing at a slot in the frame base. The slot says
//! what kind of op it is and which vertex labels it moves:
//!
//! ```text
//! origin     origin = centroid of the slot's vertices + (dx, dy, dz)
//!            (just (dx, dy, dz) when the slot has no vertices)
//! translate  v += (dx, dy, dz)
//! rotate     v = origin + R(dx, dy, dz) · (v − origin)
//! scale      v = origin + (v − origin) · (dx, dy, dz) / 128
//! ```
//!
//! The origin starts at (0, 0, 0) for each frame and carries over from one
//! op to the next until another origin op replaces it.
//!
//! [`apply_interleaved`] is the mix the original client draws when a
//! creature plays a sequence with an interleave list while it moves: the
//! moving sequence's frame drives the listed slots, the action's frame the
//! rest.

use std::sync::OnceLock;

use rs2cache::types::anim::{BaseTransform, Frame, kind};
use rs2cache::types::model::Model;

/// The original client's 2048-entry sine and cosine tables (16.16 fixed
/// point).
fn tables() -> &'static ([i32; 2048], [i32; 2048]) {
    static T: OnceLock<([i32; 2048], [i32; 2048])> = OnceLock::new();
    T.get_or_init(|| {
        let mut sin = [0; 2048];
        let mut cos = [0; 2048];
        for k in 0..2048 {
            // About 2π / 2048. It's the constant the original client uses,
            // so the truncated values come out identical.
            let a = k as f64 * 0.003_067_961_5;
            sin[k] = (65536.0 * a.sin()) as i32;
            cos[k] = (65536.0 * a.cos()) as i32;
        }
        (sin, cos)
    })
}

/// Sine of a 2048-per-turn angle, in 16.16 fixed point (65536 = 1.0).
/// Angles wrap.
pub fn sin(k: usize) -> i32 {
    tables().0[k & 2047]
}

/// Cosine of a 2048-per-turn angle, in 16.16 fixed point. Angles wrap.
pub fn cos(k: usize) -> i32 {
    tables().1[k & 2047]
}

/// Vertex indices grouped by label, as the original client groups them
/// before applying frames.
#[derive(Debug, Clone)]
pub struct Groups(
    /// Entry `l` lists the vertices labelled `l`, in vertex order. There's
    /// one entry per label up to the highest one used.
    pub Vec<Vec<usize>>,
);

impl Groups {
    /// Group vertices by their labels (`labels[v]` is vertex `v`'s label).
    pub fn new(labels: &[u8]) -> Self {
        let max = labels.iter().copied().max().map_or(0, usize::from);
        let mut g = vec![Vec::new(); if labels.is_empty() { 0 } else { max + 1 }];
        for (i, &l) in labels.iter().enumerate() {
            g[usize::from(l)].push(i);
        }
        Self(g)
    }

    /// Every vertex carrying any of `labels`, label by label. Labels past
    /// the highest one in the mesh are just skipped.
    fn of<'a>(&'a self, labels: &'a [u8]) -> impl Iterator<Item = usize> + 'a {
        labels
            .iter()
            .filter_map(|&l| self.0.get(usize::from(l)))
            .flatten()
            .copied()
    }
}

/// Rotate a point about (0, 0, 0) by 317 angles (8-bit, 1/256 turn) in the
/// original client's order and sign convention: z (roll), then x, then y.
///
/// Only the low byte of each angle counts. Each step truncates with `>> 16`,
/// exactly as the original client does, so results can be a unit off a
/// float rotation.
pub fn rotate(v: [i32; 3], rx: i32, ry: i32, rz: i32) -> [i32; 3] {
    let [mut x, mut y, mut z] = v;
    // 1/256 turn to a table index: one unit is 8 of the 2048 steps.
    let (kx, ky, kz) = (
        ((rx & 0xff) * 8) as usize,
        ((ry & 0xff) * 8) as usize,
        ((rz & 0xff) * 8) as usize,
    );
    if kz != 0 {
        let (s, c) = (sin(kz), cos(kz));
        let nx = (y * s + x * c) >> 16;
        y = (y * c - x * s) >> 16;
        x = nx;
    }
    if kx != 0 {
        let (s, c) = (sin(kx), cos(kx));
        let ny = (y * c - z * s) >> 16;
        z = (y * s + z * c) >> 16;
        y = ny;
    }
    if ky != 0 {
        let (s, c) = (sin(ky), cos(ky));
        let nx = (z * s + x * c) >> 16;
        z = (z * c - x * s) >> 16;
        x = nx;
    }
    [x, y, z]
}

/// Mix two frames as the original client does for a creature playing an
/// action sequence with an interleave list while it moves: the action
/// frame's [part](interleave_part) without the listed slots, then the
/// moving frame's part with only them.
///
/// Both frames' slots are looked up in `base`, the action frame's base,
/// even if the moving frame comes from a pack with another one. Each part
/// starts from a zero origin, as any frame does.
pub fn apply_interleaved(
    vertices: &mut [[i32; 3]],
    groups: &Groups,
    base: &[BaseTransform],
    action: &Frame,
    moving: &Frame,
    slots: &[u8],
) {
    apply_frame(
        vertices,
        groups,
        base,
        &interleave_part(base, action, slots, false),
    );
    apply_frame(
        vertices,
        groups,
        base,
        &interleave_part(base, moving, slots, true),
    );
}

/// The transforms of `frame` one side of an interleave applies: with
/// `listed`, those whose slot is in `slots` (the moving frame's share),
/// otherwise the rest (the action frame's). Origin transforms are kept
/// either way, and transforms past the end of `base` dropped.
///
/// `slots` is matched the way the original client walks it: one pass
/// through the list in step with the frame's ascending slots, so a value
/// smaller than one before it is never matched.
pub fn interleave_part(base: &[BaseTransform], frame: &Frame, slots: &[u8], listed: bool) -> Frame {
    let mut at = 0;
    let transforms = frame
        .transforms
        .iter()
        .filter(|t| {
            while slots.get(at).is_some_and(|&v| t.transform > v) {
                at += 1;
            }
            let hit = slots.get(at) == Some(&t.transform);
            base.get(usize::from(t.transform))
                .is_some_and(|b| hit == listed || b.kind == kind::ORIGIN)
        })
        .copied()
        .collect();
    Frame {
        transforms,
        ..frame.clone()
    }
}

/// Apply one frame to `vertices` in place, exactly as the original client
/// applies it.
///
/// Transforms that point past the end of `base`, and slot kinds other than
/// origin, translate, rotate and scale (alpha, say), are ignored.
pub fn apply_frame(
    vertices: &mut [[i32; 3]],
    groups: &Groups,
    base: &[BaseTransform],
    frame: &Frame,
) {
    let mut origin = [0i32; 3];
    for t in &frame.transforms {
        let Some(b) = base.get(usize::from(t.transform)) else {
            continue;
        };
        let d = [t.dx, t.dy, t.dz];
        match b.kind {
            kind::ORIGIN => {
                // Integer centroid of the slot's vertices (truncating, as
                // the original client does), then the frame's offset.
                let (mut sum, mut n) = ([0i32; 3], 0i32);
                for v in groups.of(&b.labels) {
                    for a in 0..3 {
                        sum[a] += vertices[v][a];
                    }
                    n += 1;
                }
                origin = if n > 0 {
                    [sum[0] / n + d[0], sum[1] / n + d[1], sum[2] / n + d[2]]
                } else {
                    d
                };
            }
            kind::TRANSLATE => {
                for v in groups.of(&b.labels) {
                    for a in 0..3 {
                        vertices[v][a] += d[a];
                    }
                }
            }
            kind::ROTATE => {
                for v in groups.of(&b.labels) {
                    let p = vertices[v];
                    let r = rotate(
                        [p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]],
                        d[0],
                        d[1],
                        d[2],
                    );
                    vertices[v] = [r[0] + origin[0], r[1] + origin[1], r[2] + origin[2]];
                }
            }
            kind::SCALE => {
                // d is in 1/128 units: 128 leaves the axis alone.
                for v in groups.of(&b.labels) {
                    for a in 0..3 {
                        vertices[v][a] = (vertices[v][a] - origin[a]) * d[a] / 128 + origin[a];
                    }
                }
            }
            _ => {}
        }
    }
}

/// The model's vertices after applying `frame`, then the definition's
/// scale. (The original client scales after animating, so this order
/// matters.)
pub fn deformed(
    model: &Model,
    groups: &Groups,
    base: &[BaseTransform],
    frame: &Frame,
    scale: [i32; 3],
) -> Vec<[i32; 3]> {
    let mut v = model.vertices.clone();
    apply_frame(&mut v, groups, base, frame);
    apply_scale(&mut v, scale);
    v
}

/// Scale every vertex by `scale / 128` per axis, truncating, as the
/// original client does. `[128; 3]` is a no-op and skips the loop.
pub fn apply_scale(vertices: &mut [[i32; 3]], scale: [i32; 3]) {
    if scale == [128; 3] {
        return;
    }
    for v in vertices {
        for a in 0..3 {
            v[a] = v[a] * scale[a] / 128;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rs2cache::types::anim::FrameTransform;

    #[test]
    fn tables_match_the_client_at_key_angles() {
        assert_eq!((sin(0), cos(0)), (0, 65536));
        assert_eq!(sin(512), 65535, "quarter turn truncates just below 1.0");
        assert!(cos(1024) <= -65535);
    }

    #[test]
    fn quarter_turns() {
        // A quarter turn is 64 units (64 × 8 = 512 of 2048).
        let r = rotate([100, 0, 0], 0, 64, 0);
        assert!((r[2] + 100).abs() <= 1 && r[0].abs() <= 1, "{r:?}");
        let r = rotate([0, 100, 0], 64, 0, 0);
        assert!((r[2] - 100).abs() <= 1, "{r:?}");
        let r = rotate([100, 0, 0], 0, 0, 64);
        assert!((r[1] + 100).abs() <= 1, "{r:?}");
    }

    #[test]
    fn origin_rotate_translate_scale() {
        let base = vec![
            BaseTransform {
                kind: kind::ORIGIN,
                labels: vec![1],
            },
            BaseTransform {
                kind: kind::ROTATE,
                labels: vec![1],
            },
            BaseTransform {
                kind: kind::TRANSLATE,
                labels: vec![0],
            },
            BaseTransform {
                kind: kind::SCALE,
                labels: vec![0],
            },
        ];
        let t = |transform, dx, dy, dz| FrameTransform {
            transform,
            dx,
            dy,
            dz,
        };
        let groups = Groups::new(&[0, 1, 1]);
        let mut v = vec![[0, 0, 0], [10, 0, 0], [30, 0, 0]];
        let frame = Frame {
            id: 0,
            delay: 1,
            alpha: false,
            transforms: vec![
                t(0, 0, 0, 0),
                t(1, 0, 128, 0),
                t(2, 5, 6, 7),
                t(3, 256, 128, 128),
            ],
        };
        apply_frame(&mut v, &groups, &base, &frame);
        // Labels 1 rotate a half turn about their centroid (20, 0, 0).
        assert!(
            (v[1][0] - 30).abs() <= 1 && (v[2][0] - 10).abs() <= 1,
            "{v:?}"
        );
        // Label 0 translates by (5,6,7), then scales x by 2 about the origin
        // the last ORIGIN set: (20,0,0).
        assert_eq!(v[0], [(5 - 20) * 2 + 20, 6, 7]);
    }

    #[test]
    fn interleaving_takes_listed_slots_from_the_moving_frame() {
        // Slot 0 an origin over label 1, 1 moves the "legs" (label 0),
        // 2 the "arms" (label 1), 3 rotates the arms.
        let base = vec![
            BaseTransform {
                kind: kind::ORIGIN,
                labels: vec![1],
            },
            BaseTransform {
                kind: kind::TRANSLATE,
                labels: vec![0],
            },
            BaseTransform {
                kind: kind::TRANSLATE,
                labels: vec![1],
            },
            BaseTransform {
                kind: kind::ROTATE,
                labels: vec![1],
            },
        ];
        let t = |transform, dx, dy, dz| FrameTransform {
            transform,
            dx,
            dy,
            dz,
        };
        let frame = |transforms| Frame {
            id: 0,
            delay: 1,
            alpha: false,
            transforms,
        };
        let groups = Groups::new(&[0, 1]);
        let action = frame(vec![t(1, 5, 0, 0), t(2, 0, 7, 0)]);
        let moving = frame(vec![t(1, 0, 0, 3), t(2, 100, 100, 100)]);
        let mut v = vec![[0, 0, 0], [10, 0, 0]];
        apply_interleaved(&mut v, &groups, &base, &action, &moving, &[1]);
        assert_eq!(v, [[0, 0, 3], [10, 7, 0]]);

        // The list is walked once, forwards: after 2, slot 1 can't match.
        let mut v = vec![[0, 0, 0], [10, 0, 0]];
        apply_interleaved(&mut v, &groups, &base, &action, &moving, &[2, 1]);
        assert_eq!(v, [[5, 0, 0], [110, 100, 100]]);

        // No slots listed: the action alone. Every slot listed: the moving
        // frame alone.
        for (slots, alone) in [(&[][..], &action), (&[0, 1, 2, 3][..], &moving)] {
            let mut mixed = vec![[0, 0, 0], [10, 0, 0]];
            apply_interleaved(&mut mixed, &groups, &base, &action, &moving, slots);
            let mut v = vec![[0, 0, 0], [10, 0, 0]];
            apply_frame(&mut v, &groups, &base, alone);
            assert_eq!(mixed, v, "{slots:?}");
        }

        // Each frame starts from a zero origin: the action's origin op
        // doesn't carry over into the moving frame's rotation.
        let action = frame(vec![t(0, 0, 0, 0)]);
        let moving = frame(vec![t(3, 0, 0, 128)]);
        let mut v = vec![[0, 0, 0], [10, 0, 0]];
        apply_interleaved(&mut v, &groups, &base, &action, &moving, &[3]);
        assert!((v[1][0] + 10).abs() <= 1, "{v:?}");
    }
}
