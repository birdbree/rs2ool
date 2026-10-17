//! Lighting as the client does it once per model: flat faces get one lit
//! colour from the face normal; smooth faces sum the unit face normals at
//! each vertex and light each corner; textured faces get a 0–127 shade.

use rs2intermediate::model::Model;

/// Ambient, contrast and the light direction (cache coordinates).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lighting {
    pub ambient: i32,
    pub contrast: i32,
    pub direction: [i32; 3],
}

/// What the client uses for NPCs and players.
pub const NPC: Lighting = Lighting {
    ambient: 64,
    contrast: 850,
    direction: [-30, -50, -30],
};
/// What the client uses for world objects and inventory models.
pub const OBJECT: Lighting = Lighting {
    ambient: 64,
    contrast: 768,
    direction: [-50, -10, -50],
};

/// Face kinds beyond the client's four.
pub mod kind {
    /// A flat face whose colour is 24-bit RGB rather than HSL (terrain
    /// tiles coloured straight from `flo.dat`).
    pub const FLAT_RGB: u8 = 4;
}

/// A lit model, ready to render with any vertex positions (the client
/// lights the rest pose and keeps the colours while animating).
#[derive(Debug, Clone)]
pub struct LitModel {
    pub faces: Vec<[u16; 3]>,
    /// `face type & 3`: 0 smooth, 1 flat, 2 textured smooth, 3 textured flat.
    pub kinds: Vec<u8>,
    /// Lit HSL per corner (smooth), lit HSL in `[0]` (flat), or the shade
    /// per corner (textured).
    pub colours: Vec<[i32; 3]>,
    /// The face's original colour: its texture id for textured faces.
    pub base: Vec<u16>,
    /// Index into `textured_faces` for textured faces.
    pub texture_triple: Vec<usize>,
    pub textured_faces: Vec<[u16; 3]>,
    /// 317 transparency per face (0 opaque).
    pub alphas: Vec<u8>,
    pub priorities: Option<Vec<u8>>,
}

/// `Model.method481`: apply a lightness to a face colour.
pub fn lit_colour(hsl: u16, light: i32, kind: u8) -> i32 {
    if kind & 2 == 2 {
        return 127 - light.clamp(0, 127);
    }
    let lum = ((light * i32::from(hsl & 0x7f)) >> 7).clamp(2, 126);
    i32::from(hsl & 0xff80) + lum
}

/// Light a model.
pub fn light(m: &Model, l: &Lighting) -> LitModel {
    let [lx, ly, lz] = l.direction;
    let len = (f64::from(lx * lx + ly * ly + lz * lz)).sqrt() as i32;
    let k = (l.contrast * len) >> 8;
    let n = m.faces.len();
    let kinds: Vec<u8> = (0..n)
        .map(|i| m.face_types.as_ref().map_or(0, |t| t[i] & 3))
        .collect();
    let mut acc = vec![[0i32; 4]; m.vertices.len()];
    let mut colours = vec![[0i32; 3]; n];
    for (i, f) in m.faces.iter().enumerate() {
        let [a, b, c] = f.map(|v| m.vertices[usize::from(v)]);
        let (d1, d2) = (
            [b[0] - a[0], b[1] - a[1], b[2] - a[2]],
            [c[0] - a[0], c[1] - a[1], c[2] - a[2]],
        );
        let mut nx = d1[1] * d2[2] - d2[1] * d1[2];
        let mut ny = d1[2] * d2[0] - d2[2] * d1[0];
        let mut nz = d1[0] * d2[1] - d2[0] * d1[1];
        while !(-8192..=8192).contains(&nx)
            || !(-8192..=8192).contains(&ny)
            || !(-8192..=8192).contains(&nz)
        {
            nx >>= 1;
            ny >>= 1;
            nz >>= 1;
        }
        let nl = ((f64::from(nx * nx + ny * ny + nz * nz)).sqrt() as i32).max(1);
        let (nx, ny, nz) = (nx * 256 / nl, ny * 256 / nl, nz * 256 / nl);
        if kinds[i] & 1 == 0 {
            for v in *f {
                let s = &mut acc[usize::from(v)];
                s[0] += nx;
                s[1] += ny;
                s[2] += nz;
                s[3] += 1;
            }
        } else {
            let light = l.ambient + (lx * nx + ly * ny + lz * nz) / (k + k / 2).max(1);
            colours[i][0] = lit_colour(
                m.face_colours[i],
                light,
                m.face_types.as_ref().map_or(0, |t| t[i]),
            );
        }
    }
    for (i, f) in m.faces.iter().enumerate() {
        if kinds[i] & 1 != 0 {
            continue;
        }
        let kind = m.face_types.as_ref().map_or(0, |t| t[i]);
        for (corner, v) in f.iter().enumerate() {
            let s = acc[usize::from(*v)];
            let light = l.ambient + (lx * s[0] + ly * s[1] + lz * s[2]) / (k * s[3]).max(1);
            colours[i][corner] = lit_colour(m.face_colours[i], light, kind);
        }
    }
    LitModel {
        faces: m.faces.clone(),
        texture_triple: (0..n)
            .map(|i| m.face_types.as_ref().map_or(0, |t| usize::from(t[i] >> 2)))
            .collect(),
        kinds,
        colours,
        base: m.face_colours.clone(),
        textured_faces: m.textured_faces.clone(),
        alphas: m.face_alphas.clone().unwrap_or_else(|| vec![0; n]),
        priorities: m.face_priorities.clone(),
    }
}

impl LitModel {
    /// A model of flat RGB triangles (no lighting), for terrain.
    pub fn flat_rgb(faces: Vec<[u16; 3]>, rgb: Vec<u32>) -> Self {
        let n = faces.len();
        Self {
            faces,
            kinds: vec![kind::FLAT_RGB; n],
            colours: rgb.into_iter().map(|c| [c as i32, 0, 0]).collect(),
            base: vec![0; n],
            texture_triple: vec![0; n],
            textured_faces: Vec::new(),
            alphas: vec![0; n],
            priorities: None,
        }
    }
}
