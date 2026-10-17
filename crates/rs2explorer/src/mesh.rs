//! Bevy meshes from decoded models and converted pack models. Every face
//! gets its own three vertices so normals are hard (flat shading).

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Mesh, PrimitiveTopology};
use rs2intermediate::model::Model;
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
        let nrm = flat_normal(p.map(|q| [q[0] as f32, -q[1] as f32, q[2] as f32]));
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
            pos.push([q[0] as f32, -q[1] as f32, q[2] as f32]);
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

/// Build a mesh from a converted pack model and the pack palette. Pack
/// positions are already y up.
pub fn mesh_from_pack_model(m: &PackModel, palette: &[[u8; 3]]) -> Mesh {
    let n = m.faces.len() * 3;
    let mut pos = Vec::with_capacity(n);
    let mut normals = Vec::with_capacity(n);
    let mut colors = Vec::with_capacity(n);
    for (i, f) in m.faces.iter().enumerate() {
        let p = f.map(|v| m.vertices[usize::from(v)].map(f32::from));
        let nrm = flat_normal(p);
        let base = if m.material_slots[i] != 0 {
            [0.55, 0.5, 0.45]
        } else {
            palette
                .get(usize::from(m.palette_indices[i]))
                .map_or([1.0, 0.0, 1.0], |c| c.map(|v| f32::from(v) / 255.0))
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
