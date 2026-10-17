//! The 317 client's frame application, reproduced in integer arithmetic so
//! it matches the client vertex for vertex. This is the *reference
//! deformation* that converted clips are validated against, and that the
//! explorer's cache playback uses.

use std::sync::OnceLock;

use rs2cache::types::anim::{BaseTransform, Frame, kind};
use rs2cache::types::model::Model;

/// The client's 2048-entry sine and cosine tables (16.16 fixed point).
fn tables() -> &'static ([i32; 2048], [i32; 2048]) {
    static T: OnceLock<([i32; 2048], [i32; 2048])> = OnceLock::new();
    T.get_or_init(|| {
        let mut sin = [0; 2048];
        let mut cos = [0; 2048];
        for k in 0..2048 {
            let a = k as f64 * 0.003_067_961_5;
            sin[k] = (65536.0 * a.sin()) as i32;
            cos[k] = (65536.0 * a.cos()) as i32;
        }
        (sin, cos)
    })
}

pub fn sin(k: usize) -> i32 {
    tables().0[k & 2047]
}

pub fn cos(k: usize) -> i32 {
    tables().1[k & 2047]
}

/// Vertices grouped by label, as the client builds them (`method469`).
#[derive(Debug, Clone)]
pub struct Groups(pub Vec<Vec<usize>>);

impl Groups {
    pub fn new(labels: &[u8]) -> Self {
        let max = labels.iter().copied().max().map_or(0, usize::from);
        let mut g = vec![Vec::new(); if labels.is_empty() { 0 } else { max + 1 }];
        for (i, &l) in labels.iter().enumerate() {
            g[usize::from(l)].push(i);
        }
        Self(g)
    }

    fn of<'a>(&'a self, labels: &'a [u8]) -> impl Iterator<Item = usize> + 'a {
        labels
            .iter()
            .filter_map(|&l| self.0.get(usize::from(l)))
            .flatten()
            .copied()
    }
}

/// Rotate a point about the origin by 317 angles (8-bit, 1/256 turn) in
/// the client's order and sign convention: z (roll), then x, then y.
pub fn rotate(v: [i32; 3], rx: i32, ry: i32, rz: i32) -> [i32; 3] {
    let [mut x, mut y, mut z] = v;
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

/// Apply one frame to `vertices` in place, exactly as `Model.method470`.
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
/// scale (the client scales after animating).
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

/// Scale every vertex by `scale / 128` per axis, as `Model.method478`.
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
}
