//! Bevy meshes from decoded models and converted pack models. Every face
//! gets its own three vertices so normals are hard (flat shading).

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Mesh, PrimitiveTopology};
use rs2cache::types::model::Model;
use rs2pack::palette::{hsl_to_rgb, split};
use rs2pack::records::model::PackModel;

/// The client's flat lighting parameters: ambient, contrast and the light
/// direction in cache coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lighting {
    pub ambient: i32,
    pub contrast: i32,
    pub direction: [i32; 3],
}

/// The NPC lighting the client uses (`EntityDef`): ambient 64, contrast
/// 850, light from (−30, −50, −30).
pub const NPC_LIGHTING: Lighting = Lighting {
    ambient: 64,
    contrast: 850,
    direction: [-30, -50, -30],
};

/// How a mesh's colours are computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shading {
    /// Raw face colours, for real (engine) lighting.
    Raw,
    /// The client's flat directional term baked into the colours, drawn
    /// unlit.
    Client(Lighting),
}

/// The explorer's world position of a cache-space point: y and z negated.
/// The cache is right-handed with y down and z north (tile y); negating
/// two axes is a rotation, so models keep their handedness and sit in the
/// same frame as map terrain (negating y alone would mirror them).
pub fn world(v: [i32; 3]) -> [f32; 3] {
    [v[0] as f32, -v[1] as f32, -v[2] as f32]
}

fn rgb(c: u32) -> [f32; 3] {
    let [r, g, b] = split(c);
    [r, g, b].map(|v| f32::from(v) / 255.0)
}

/// The client's flat-face lightness for a face normal (cache coordinates):
/// `ambient + dot(L, n̂·256) / (1.5 · contrast · |L| / 256)`.
pub fn face_lightness(normal: [i32; 3], l: &Lighting) -> i32 {
    let [mut nx, mut ny, mut nz] = normal;
    while nx.abs() > 8192 || ny.abs() > 8192 || nz.abs() > 8192 {
        nx >>= 1;
        ny >>= 1;
        nz >>= 1;
    }
    let len = (f64::from(nx * nx + ny * ny + nz * nz)).sqrt() as i32;
    let len = len.max(1);
    let (nx, ny, nz) = (nx * 256 / len, ny * 256 / len, nz * 256 / len);
    let [lx, ly, lz] = l.direction;
    let llen = (f64::from(lx * lx + ly * ly + lz * lz)).sqrt() as i32;
    let k = (l.contrast * llen) >> 8;
    l.ambient + (lx * nx + ly * ny + lz * nz) / (k + k / 2).max(1)
}

/// A 317 HSL colour with the lightness applied, as `Model.method481`.
pub fn lit_hsl(hsl: u16, lightness: i32) -> u16 {
    let lum = ((lightness * i32::from(hsl & 0x7f)) >> 7).clamp(2, 126);
    (hsl & 0xff80) | lum as u16
}

fn sub(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Build a mesh from a decoded model. Positions are cache units with y
/// negated (y up); `labels` recolours vertices by label instead.
pub fn mesh_from_model(m: &Model, vertices: &[[i32; 3]], shading: Shading, labels: bool) -> Mesh {
    let n = m.faces.len() * 3;
    let mut pos = Vec::with_capacity(n);
    let mut normals = Vec::with_capacity(n);
    let mut colors = Vec::with_capacity(n);
    for (i, f) in m.faces.iter().enumerate() {
        let p = f.map(|v| vertices[usize::from(v)]);
        // The client lights with the cache-space cross product.
        let normal = cross(sub(p[1], p[0]), sub(p[2], p[0]));
        let nrm = flat_normal(p.map(world));
        let textured = m.face_types.as_ref().is_some_and(|t| t[i] & 2 != 0);
        let hsl = m.face_colours[i];
        let base = if textured {
            [0.55, 0.5, 0.45]
        } else {
            match shading {
                Shading::Raw => rgb(hsl_to_rgb(hsl)),
                Shading::Client(l) => rgb(hsl_to_rgb(lit_hsl(hsl, face_lightness(normal, &l)))),
            }
        };
        let alpha = m
            .face_alphas
            .as_ref()
            .map_or(1.0, |a| 1.0 - f32::from(a[i]) / 255.0);
        for (corner, v) in f.iter().enumerate() {
            let q = p[corner];
            pos.push(world(q));
            normals.push(nrm);
            let c = if labels {
                let l = m.vertex_labels.as_ref().map_or(0, |l| l[usize::from(*v)]);
                label_colour(l)
            } else {
                base
            };
            colors.push([c[0], c[1], c[2], alpha]);
        }
    }
    build(pos, normals, colors)
}

/// A distinct colour per label, for the labels overlay.
pub fn label_colour(l: u8) -> [f32; 3] {
    let h = f32::from(l.wrapping_mul(47)) / 255.0 * 6.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    match h as u8 {
        0 => [1.0, x, 0.2],
        1 => [x, 1.0, 0.2],
        2 => [0.2, 1.0, x],
        3 => [0.2, x, 1.0],
        4 => [x, 0.2, 1.0],
        _ => [1.0, 0.2, x],
    }
}

/// Build a mesh from a converted pack model and the pack palette (grey
/// without one). Pack space is the explorer's world space. With client
/// shading the face normal is taken back to cache space for the client's
/// lightness term, applied to the colour's HSL lightness as the client
/// applies it to its 7-bit luminance.
pub fn mesh_from_pack_model(m: &PackModel, palette: Option<&[[u8; 3]]>, shading: Shading) -> Mesh {
    let n = m.faces.len() * 3;
    let mut pos = Vec::with_capacity(n);
    let mut normals = Vec::with_capacity(n);
    let mut colors = Vec::with_capacity(n);
    for (i, f) in m.faces.iter().enumerate() {
        // Pack space is the explorer's world space (y and z negated).
        let p = f.map(|v| m.vertices[usize::from(v)].map(f32::from));
        let nrm = flat_normal(p);
        let base = if m.material_slots[i] != 0 {
            [0.55, 0.5, 0.45]
        } else {
            palette.map_or([0.6, 0.6, 0.6], |pal| {
                pal.get(usize::from(m.palette_indices[i]))
                    .map_or([1.0, 0.0, 1.0], |c| c.map(|v| f32::from(v) / 255.0))
            })
        };
        let base = match shading {
            Shading::Raw => base,
            Shading::Client(l) => {
                let c = f.map(|v| {
                    let [x, y, z] = m.vertices[usize::from(v)].map(i32::from);
                    [x, -y, -z]
                });
                let lightness = face_lightness(cross(sub(c[1], c[0]), sub(c[2], c[0])), &l);
                scale_lightness(base, lightness as f32 / 128.0)
            }
        };
        let alpha = m.alphas.as_ref().map_or(1.0, |a| f32::from(a[i]) / 255.0);
        for q in p {
            pos.push(q);
            normals.push(nrm);
            colors.push([base[0], base[1], base[2], alpha]);
        }
    }
    build(pos, normals, colors)
}

/// An RGB colour with its HSL lightness multiplied by `k` (clamped).
fn scale_lightness([r, g, b]: [f32; 3], k: f32) -> [f32; 3] {
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let target = (l * k).clamp(2.0 / 128.0, 126.0 / 128.0);
    if max == min {
        return [target; 3];
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } / 6.0;
    let q = if target < 0.5 {
        target * (1.0 + s)
    } else {
        target + s - target * s
    };
    let p = 2.0 * target - q;
    let channel = |t: f32| {
        let t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    [channel(h + 1.0 / 3.0), channel(h), channel(h - 1.0 / 3.0)]
}

/// The flat normal of a y-up triangle whose winding came from the y-down
/// cache: flipping y mirrors the triangle, so the normal is `e2 × e1`.
fn flat_normal(p: [[f32; 3]; 3]) -> [f32; 3] {
    let e1 = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
    let e2 = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
    let c = [
        e2[1] * e1[2] - e2[2] * e1[1],
        e2[2] * e1[0] - e2[0] * e1[2],
        e2[0] * e1[1] - e2[1] * e1[0],
    ];
    let len = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
    if len > 0.0 {
        c.map(|v| v / len)
    } else {
        [0.0, 1.0, 0.0]
    }
}

fn build(pos: Vec<[f32; 3]>, normals: Vec<[f32; 3]>, colors: Vec<[f32; 4]>) -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
}

/// Axis-aligned bounds of a mesh's positions.
pub fn bounds(mesh: &Mesh) -> Option<([f32; 3], [f32; 3])> {
    let bevy::mesh::VertexAttributeValues::Float32x3(p) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)?
    else {
        return None;
    };
    let first = *p.first()?;
    Some(p.iter().fold((first, first), |(mut lo, mut hi), v| {
        for a in 0..3 {
            lo[a] = lo[a].min(v[a]);
            hi[a] = hi[a].max(v[a]);
        }
        (lo, hi)
    }))
}

/// One part of a model: the faces drawn with one texture (or none).
pub struct Part {
    /// `None` for untextured faces.
    pub texture: Option<u16>,
    pub mesh: Mesh,
}

/// The client's shade for a textured face as a brightness factor: the
/// shade picks one of four darker texture copies (1, ⅞, ¾, ⅝) and halves
/// again past 64.
pub fn texture_shade(shade: i32) -> f32 {
    let level = [1.0, 0.875, 0.75, 0.625][((shade >> 4) & 3) as usize];
    if shade >= 64 { level * 0.5 } else { level }
}

/// Texture coordinates for a textured face's corners from its P/M/N
/// triangle: u along M − P and v along N − P (not clamped; the material
/// clamps u and repeats v, as the client does).
pub fn texture_uvs(corners: [[i32; 3]; 3], p: [i32; 3], m: [i32; 3], n: [i32; 3]) -> [[f32; 2]; 3] {
    let f = |v: [i32; 3]| v.map(|c| c as f64);
    let (pp, e1, e2) = (f(p), f(m), f(n));
    let e1 = [e1[0] - pp[0], e1[1] - pp[1], e1[2] - pp[2]];
    let e2 = [e2[0] - pp[0], e2[1] - pp[1], e2[2] - pp[2]];
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let (a, b, c) = (dot(e1, e1), dot(e1, e2), dot(e2, e2));
    let det = a * c - b * b;
    corners.map(|v| {
        let d = f(v);
        let d = [d[0] - pp[0], d[1] - pp[1], d[2] - pp[2]];
        let (d1, d2) = (dot(d, e1), dot(d, e2));
        if det.abs() < 1e-9 {
            [0.0, 0.0]
        } else {
            [
                ((c * d1 - b * d2) / det) as f32,
                ((a * d2 - b * d1) / det) as f32,
            ]
        }
    })
}

/// A model split into parts by texture, with UVs on textured parts.
pub fn model_parts(m: &Model, vertices: &[[i32; 3]], shading: Shading) -> Vec<Part> {
    let mut groups: std::collections::BTreeMap<Option<u16>, Buffers> = Default::default();
    for (i, f) in m.faces.iter().enumerate() {
        let p = f.map(|v| vertices[usize::from(v)]);
        let normal = cross(sub(p[1], p[0]), sub(p[2], p[0]));
        let nrm = flat_normal(p.map(world));
        let ty = m.face_types.as_ref().map_or(0, |t| t[i]);
        let triple = m
            .textured_faces
            .get(usize::from(ty >> 2))
            .filter(|_| ty & 2 != 0);
        let alpha = m
            .face_alphas
            .as_ref()
            .map_or(1.0, |a| 1.0 - f32::from(a[i]) / 255.0);
        let (key, colour, uv, alpha) = match triple {
            Some(&[tp, tm, tn]) if m.face_colours[i] < 50 => {
                let at = |v: u16| vertices[usize::from(v)];
                let shade = match shading {
                    Shading::Raw => 1.0,
                    Shading::Client(l) => {
                        texture_shade(127 - face_lightness(normal, &l).clamp(0, 127))
                    }
                };
                // The client draws textured faces without face alpha.
                (
                    Some(m.face_colours[i]),
                    [shade; 3],
                    texture_uvs(p, at(tp), at(tm), at(tn)),
                    1.0,
                )
            }
            _ => {
                let hsl = m.face_colours[i];
                let c = match shading {
                    Shading::Raw => rgb(hsl_to_rgb(hsl)),
                    Shading::Client(l) => rgb(hsl_to_rgb(lit_hsl(hsl, face_lightness(normal, &l)))),
                };
                (None, c, [[0.0; 2]; 3], alpha)
            }
        };
        let g = groups.entry(key).or_default();
        for (corner, q) in p.iter().enumerate() {
            g.pos.push(world(*q));
            g.normals.push(nrm);
            g.colors.push([colour[0], colour[1], colour[2], alpha]);
            g.uvs.push(uv[corner]);
        }
    }
    groups
        .into_iter()
        .map(|(texture, g)| Part {
            texture,
            mesh: build(g.pos, g.normals, g.colors)
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, g.uvs),
        })
        .collect()
}

/// Per-corner vertex data of one part while it is collected.
#[derive(Default)]
struct Buffers {
    pos: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    uvs: Vec<[f32; 2]>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    #[test]
    fn texture_uvs_put_m_along_u_and_n_along_v() {
        let (p, m, n) = ([0, 0, 0], [128, 0, 0], [0, 0, 256]);
        let uv = texture_uvs([p, m, [64, 0, 512]], p, m, n);
        assert_eq!(uv[0], [0.0, 0.0]);
        assert_eq!(uv[1], [1.0, 0.0]);
        // Past N, v runs on so the repeating sampler wraps it.
        assert_eq!(uv[2], [0.5, 2.0]);
    }

    #[test]
    fn world_mapping_is_a_rotation_not_a_mirror() {
        let [x, y, z] = [[1, 0, 0], [0, 1, 0], [0, 0, 1]].map(world);
        let c = [
            x[1] * y[2] - x[2] * y[1],
            x[2] * y[0] - x[0] * y[2],
            x[0] * y[1] - x[1] * y[0],
        ];
        // x × y = z in the cache, so it must stay so after mapping.
        assert_eq!(c, z);
        assert_eq!(world([0, -100, 0])[1], 100.0, "cache y is down");
    }

    #[test]
    fn texture_shade_matches_the_four_copies() {
        assert_eq!(texture_shade(0), 1.0);
        assert_eq!(texture_shade(16), 0.875);
        assert_eq!(texture_shade(63), 0.625);
        assert_eq!(texture_shade(64), 0.5);
    }

    #[test]
    fn parts_split_faces_by_texture() {
        let m = Model {
            vertices: vec![[0, 0, 0], [100, 0, 0], [0, 0, 100], [100, 0, 100]],
            faces: vec![[0, 1, 2], [1, 3, 2], [0, 2, 1]],
            face_colours: vec![7, 1000, 7],
            face_types: Some(vec![2, 0, 3]),
            face_alphas: Some(vec![128, 128, 128]),
            textured_faces: vec![[0, 1, 2]],
            priority: 0,
            face_priorities: None,
            face_labels: None,
            vertex_labels: None,
        };
        let parts = model_parts(&m, &m.vertices, Shading::Client(NPC_LIGHTING));
        let keys: Vec<_> = parts.iter().map(|p| p.texture).collect();
        assert_eq!(keys, [None, Some(7)]);
        assert_eq!(parts[1].mesh.count_vertices(), 6);
        let Some(VertexAttributeValues::Float32x4(c)) =
            parts[1].mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("vertex colours");
        };
        assert!(
            c.iter().all(|c| c[3] == 1.0),
            "textured faces ignore face alpha"
        );
        assert!(parts[1].mesh.attribute(Mesh::ATTRIBUTE_UV_0).is_some());
    }

    #[test]
    fn scaling_lightness_keeps_hue() {
        let red = [0.8, 0.2, 0.2];
        let same = scale_lightness(red, 1.0);
        assert!(
            same.iter().zip(red).all(|(a, b)| (a - b).abs() < 1e-5),
            "{same:?}"
        );
        let dark = scale_lightness(red, 0.5);
        assert!(dark[0] < red[0] && dark[0] > dark[1] && (dark[1] - dark[2]).abs() < 1e-6);
    }
}
