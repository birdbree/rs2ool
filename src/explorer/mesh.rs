//! Bevy meshes from decoded models and converted pack models.
//!
//! Every face gets its own three vertices, so normals are hard and
//! everything's flat-shaded. Colours are either the raw face colours (for
//! the engine's lights) or have the original client's flat directional
//! lighting baked in, so they can be drawn unlit and look the way that
//! client draws them.
//!
//! Cache positions go through [`world`] on the way in. Pack models are
//! already in that space, since pack space is the cache with y and z
//! negated too.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Mesh, PrimitiveTopology};
use contentpack::palette::{hsl_to_rgb, split};
use contentpack::records::model::PackModel;
use rs2cache::types::model::Model;

/// The original client's flat lighting parameters: ambient, contrast and
/// the light direction in cache coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lighting {
    /// The lightness every face starts from before the directional term
    /// (a total of 128 leaves the colour as stored).
    pub ambient: i32,
    /// Divides the directional term down; bigger is flatter.
    pub contrast: i32,
    /// Towards the light, in cache space (y down). Doesn't need to be unit
    /// length.
    pub direction: [i32; 3],
}

/// The lighting the original client gives NPC models: ambient 64,
/// contrast 850, light from (−30, −50, −30).
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
    /// The original client's flat directional term baked into the colours,
    /// drawn unlit.
    Client(Lighting),
}

/// The explorer's world position of a cache-space point: y and z negated.
/// The cache is right-handed with y down and z north (tile y); negating
/// two axes is a rotation, so models keep their handedness and sit in the
/// same frame as map terrain (negating y alone would mirror them).
pub fn world(v: [i32; 3]) -> [f32; 3] {
    [v[0] as f32, -v[1] as f32, -v[2] as f32]
}

/// A packed `0xRRGGBB` colour as 0–1 floats.
fn rgb(c: u32) -> [f32; 3] {
    let [r, g, b] = split(c);
    [r, g, b].map(|v| f32::from(v) / 255.0)
}

/// The original client's flat-face lightness for a face normal (cache
/// coordinates, any length): `ambient + dot(L, n̂·256) / (1.5 · contrast ·
/// |L| / 256)`, in integer maths like the original. 128 means unchanged.
pub fn face_lightness(normal: [i32; 3], l: &Lighting) -> i32 {
    let [mut nx, mut ny, mut nz] = normal;
    // Halve until the squares can't overflow i32, as the original does.
    while nx.abs() > 8192 || ny.abs() > 8192 || nz.abs() > 8192 {
        nx >>= 1;
        ny >>= 1;
        nz >>= 1;
    }
    let len = (f64::from(nx * nx + ny * ny + nz * nz)).sqrt() as i32;
    let len = len.max(1);
    // Normal scaled to length 256.
    let (nx, ny, nz) = (nx * 256 / len, ny * 256 / len, nz * 256 / len);
    let [lx, ly, lz] = l.direction;
    let llen = (f64::from(lx * lx + ly * ly + lz * lz)).sqrt() as i32;
    // `k + k / 2` below is the 1.5 · contrast · |L| / 256 divisor.
    let k = (l.contrast * llen) >> 8;
    l.ambient + (lx * nx + ly * ny + lz * nz) / (k + k / 2).max(1)
}

/// A 317 HSL colour with a face lightness applied, the way the original
/// client lights a flat face: the 7-bit luminance is scaled by
/// `lightness / 128` and clamped to 2–126. Hue and saturation stay put.
pub fn lit_hsl(hsl: u16, lightness: i32) -> u16 {
    // Bits 0–6 are luminance; the upper 9 bits are hue and saturation.
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

/// Builds one mesh from a decoded model, posed at `vertices` (the model's
/// own, or an animation frame's).
///
/// Positions are cache units mapped through [`world`], so y and z are
/// negated and y is up. Textured faces get a flat stand-in colour; use
/// [`model_parts`] to draw textures. `labels` colours each vertex by its
/// label instead of the face colour.
pub fn mesh_from_model(m: &Model, vertices: &[[i32; 3]], shading: Shading, labels: bool) -> Mesh {
    let n = m.faces.len() * 3;
    let mut pos = Vec::with_capacity(n);
    let mut normals = Vec::with_capacity(n);
    let mut colors = Vec::with_capacity(n);
    for (i, f) in m.faces.iter().enumerate() {
        let p = f.map(|v| vertices[usize::from(v)]);
        // The original client lights with the cache-space cross product.
        let normal = cross(sub(p[1], p[0]), sub(p[2], p[0]));
        let nrm = flat_normal(p.map(world));
        // Bit 1 of the face type marks a textured face.
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
        // Cache alpha is transparency (0 = opaque); flip it to opacity.
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

/// A distinct colour per label (or bone), for the vertex colour overlays
/// and the bone overlay.
pub fn label_colour(l: u8) -> [f32; 3] {
    // Multiplying by 47 scatters neighbouring labels round the hue wheel;
    // `h` is the hue in sixths.
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

/// Builds a mesh from a converted pack model and the pack palette (grey
/// without one, magenta for an index past its end).
///
/// Pack space is the explorer's world space. With [`Shading::Client`] the
/// face normal is taken back to cache space for the original client's
/// lightness term, which is applied to the colour's HSL lightness the way
/// that client applies it to its 7-bit luminance.
///
/// With `tags` (one per vertex) each corner is coloured by its vertex's tag
/// through [`label_colour`] instead of the face colour.
pub fn mesh_from_pack_model(
    m: &PackModel,
    palette: Option<&[[u8; 3]]>,
    shading: Shading,
    tags: Option<&[u8]>,
) -> Mesh {
    let n = m.faces.len() * 3;
    let mut pos = Vec::with_capacity(n);
    let mut normals = Vec::with_capacity(n);
    let mut colors = Vec::with_capacity(n);
    for (i, f) in m.faces.iter().enumerate() {
        // Pack space is the explorer's world space (y and z negated).
        let p = f.map(|v| m.vertices[usize::from(v)].map(f32::from));
        let nrm = flat_normal(p);
        // A face with a material slot is textured: flat stand-in colour.
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
                // Back to cache space; negating y and z is its own inverse.
                let c = f.map(|v| {
                    let [x, y, z] = m.vertices[usize::from(v)].map(i32::from);
                    [x, -y, -z]
                });
                let lightness = face_lightness(cross(sub(c[1], c[0]), sub(c[2], c[0])), &l);
                scale_lightness(base, lightness as f32 / 128.0)
            }
        };
        // Pack alpha is already opacity (255 = opaque).
        let alpha = m.alphas.as_ref().map_or(1.0, |a| f32::from(a[i]) / 255.0);
        for (q, v) in p.into_iter().zip(f) {
            let c = tags.map_or(base, |t| label_colour(t[usize::from(*v)]));
            pos.push(q);
            normals.push(nrm);
            colors.push([c[0], c[1], c[2], alpha]);
        }
    }
    build(pos, normals, colors)
}

/// An RGB colour with its HSL lightness multiplied by `k`, clamped to the
/// same 2/128–126/128 range the original client's luminance gets.
fn scale_lightness([r, g, b]: [f32; 3], k: f32) -> [f32; 3] {
    // RGB → HSL, swap in the new lightness, HSL → RGB.
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let target = (l * k).clamp(2.0 / 128.0, 126.0 / 128.0);
    // Grey: no hue to keep.
    if max == min {
        return [target; 3];
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    // Hue in 0–1.
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } / 6.0;
    // The usual HSL → RGB helpers: `q` and `p` bound each channel.
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

/// The unit flat normal of a world-space triangle, `e1 × e2`: the same
/// cross product the original client lights with in cache space. World
/// space is cache space turned half a turn about x (a rotation, not a
/// mirror), so the cross product turns with it and still points out of
/// the model. Straight up for a degenerate triangle.
fn flat_normal(p: [[f32; 3]; 3]) -> [f32; 3] {
    let e1 = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
    let e2 = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
    let c = [
        e1[1] * e2[2] - e1[2] * e2[1],
        e1[2] * e2[0] - e1[0] * e2[2],
        e1[0] * e2[1] - e1[1] * e2[0],
    ];
    let len = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
    if len > 0.0 {
        c.map(|v| v / len)
    } else {
        [0.0, 1.0, 0.0]
    }
}

/// A triangle list from per-corner positions, normals and colours.
fn build(pos: Vec<[f32; 3]>, normals: Vec<[f32; 3]>, colors: Vec<[f32; 4]>) -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
}

/// Axis-aligned bounds of a mesh's positions, as `(min, max)`. `None` for
/// an empty mesh.
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
    /// Texture id, 0–49; `None` for untextured faces.
    pub texture: Option<u16>,
    /// The part's faces, with UVs.
    pub mesh: Mesh,
}

/// The original client's shade for a textured face as a brightness
/// factor: the shade picks one of four darker texture copies (1, ⅞, ¾, ⅝)
/// and halves again from 64 up.
pub fn texture_shade(shade: i32) -> f32 {
    // Bits 4–5 pick the copy.
    let level = [1.0, 0.875, 0.75, 0.625][((shade >> 4) & 3) as usize];
    if shade >= 64 { level * 0.5 } else { level }
}

/// Texture coordinates for a textured face's corners from its P/M/N
/// triangle: u along M − P and v along N − P (not clamped; the material
/// clamps u and repeats v, as the original client does).
pub fn texture_uvs(corners: [[i32; 3]; 3], p: [i32; 3], m: [i32; 3], n: [i32; 3]) -> [[f32; 2]; 3] {
    // Solve `d = u·e1 + v·e2` for each corner offset `d` by least squares
    // (the 2 × 2 normal equations), which also copes with corners slightly
    // off the P/M/N plane.
    let f = |v: [i32; 3]| v.map(|c| c as f64);
    let (pp, e1, e2) = (f(p), f(m), f(n));
    let e1 = [e1[0] - pp[0], e1[1] - pp[1], e1[2] - pp[2]];
    let e2 = [e2[0] - pp[0], e2[1] - pp[1], e2[2] - pp[2]];
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    // The Gram matrix [[a, b], [b, c]] and its determinant.
    let (a, b, c) = (dot(e1, e1), dot(e1, e2), dot(e2, e2));
    let det = a * c - b * b;
    corners.map(|v| {
        let d = f(v);
        let d = [d[0] - pp[0], d[1] - pp[1], d[2] - pp[2]];
        let (d1, d2) = (dot(d, e1), dot(d, e2));
        // M, N and P in a line: no sensible mapping.
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

/// A model posed at `vertices`, split into parts by texture, with UVs on
/// textured parts. The untextured part (if any) comes first, then
/// textures by id.
///
/// A face counts as textured when its type says so, its P/M/N triangle
/// exists and its texture id is under 50; otherwise it's drawn in its
/// colour.
pub fn model_parts(m: &Model, vertices: &[[i32; 3]], shading: Shading) -> Vec<Part> {
    let mut groups: std::collections::BTreeMap<Option<u16>, Buffers> = Default::default();
    for (i, f) in m.faces.iter().enumerate() {
        let p = f.map(|v| vertices[usize::from(v)]);
        let normal = cross(sub(p[1], p[0]), sub(p[2], p[0]));
        let nrm = flat_normal(p.map(world));
        let ty = m.face_types.as_ref().map_or(0, |t| t[i]);
        // Bit 1 marks a textured face; the bits above it index its P/M/N
        // triangle.
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
                // A textured face's colour is its texture id.
                let shade = match shading {
                    Shading::Raw => 1.0,
                    // Flip lightness into a 0–127 darkness for the copies.
                    Shading::Client(l) => {
                        texture_shade(127 - face_lightness(normal, &l).clamp(0, 127))
                    }
                };
                // The original client draws textured faces without face
                // alpha.
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

    /// A closed tetrahedron wound the way cache models are, with `e1 × e2`
    /// pointing out of the shape in cache space.
    fn tetrahedron() -> Model {
        Model {
            vertices: vec![[0, 0, 0], [100, 0, 0], [0, 100, 0], [0, 0, 100]],
            faces: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
            face_colours: vec![0; 4],
            face_types: None,
            priority: 0,
            face_priorities: None,
            face_alphas: None,
            face_labels: None,
            vertex_labels: None,
            textured_faces: Vec::new(),
        }
    }

    #[test]
    fn flat_normals_point_out_of_the_model() {
        let m = tetrahedron();
        let mesh = mesh_from_model(&m, &m.vertices, Shading::Raw, false);
        let Some(VertexAttributeValues::Float32x3(n)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("normals");
        };
        let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions");
        };
        // Centre of the shape in world space.
        let c = m
            .vertices
            .iter()
            .map(|&v| world(v))
            .fold([0.0f32; 3], |a, v| {
                [a[0] + v[0] / 4.0, a[1] + v[1] / 4.0, a[2] + v[2] / 4.0]
            });
        for (face, (pos, nrm)) in p.chunks(3).zip(n.chunks(3)).enumerate() {
            let mid = [0, 1, 2].map(|a| (pos[0][a] + pos[1][a] + pos[2][a]) / 3.0);
            let out: f32 = (0..3).map(|a| nrm[0][a] * (mid[a] - c[a])).sum();
            assert!(out > 0.0, "face {face}'s normal points into the shape");
        }
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
