//! Lighting, done once per model the way the original client does it.
//!
//! Flat faces get one lit colour from the face normal. Smooth faces sum the
//! normals of the smooth faces at each vertex and light each corner from
//! that. Textured faces get a 0–127 shade instead of a colour.
//!
//! ```text
//! n      = face normal, halved until each part is within ±8192,
//!          then scaled to length ~256
//! k      = (contrast · |direction|) >> 8
//! flat   light = ambient + (direction · n) / max(1, k + k/2)
//! smooth light = ambient + (direction · Σn) / max(1, k · count)
//! colour = (hsl & 0xff80) + clamp((light · (hsl & 0x7f)) >> 7, 2, 126)
//! shade  = 127 − clamp(light, 0, 127)               (textured)
//! ```
//!
//! Note the different divisors: a lone smooth face reacts to the light
//! half as much again as the same face drawn flat.

use rs2cache::types::model::Model;

/// Ambient, contrast and the light direction (cache coordinates).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lighting {
    /// Base light level, added to every light value. (A total of 128
    /// leaves a colour's lightness as it is.)
    pub ambient: i32,
    /// How much the angle to the light matters; bigger means weaker.
    pub contrast: i32,
    /// Light vector in cache coordinates (y down). Faces whose normal
    /// points along it get brighter.
    pub direction: [i32; 3],
}

/// What the original client uses for NPCs and players.
pub const NPC: Lighting = Lighting {
    ambient: 64,
    contrast: 850,
    direction: [-30, -50, -30],
};
/// What the original client uses for world objects and inventory models.
pub const OBJECT: Lighting = Lighting {
    ambient: 64,
    contrast: 768,
    direction: [-50, -10, -50],
};

/// Face kinds beyond the four in the cache (`type & 3`).
pub mod kind {
    /// A flat face whose colour is 24-bit RGB rather than HSL (terrain
    /// tiles coloured straight from `flo.dat`).
    pub const FLAT_RGB: u8 = 4;
}

/// A lit model, ready to render with any vertex positions. (The original
/// client lights the rest pose and keeps those colours while animating, so
/// this does too.)
#[derive(Debug, Clone)]
pub struct LitModel {
    /// Vertex indices per face, as in the model.
    pub faces: Vec<[u16; 3]>,
    /// `face type & 3`: 0 smooth, 1 flat, 2 textured smooth, 3 textured
    /// flat, or [`kind::FLAT_RGB`].
    pub kinds: Vec<u8>,
    /// Lit HSL per corner (smooth), lit HSL in `[0]` (flat), the shade per
    /// corner or in `[0]` (textured smooth or flat), or `0xRRGGBB` in `[0]`
    /// ([`kind::FLAT_RGB`]).
    pub colours: Vec<[i32; 3]>,
    /// The face's original colour: its texture id for textured faces.
    pub base: Vec<u16>,
    /// Index into `textured_faces` for textured faces (`type >> 2`).
    pub texture_triple: Vec<usize>,
    /// The model's P/M/N texture triangles, as vertex indices.
    pub textured_faces: Vec<[u16; 3]>,
    /// 317 transparency per face (0 opaque).
    pub alphas: Vec<u8>,
    /// Per-face priorities, 0–11 (12 and up aren't drawn). `None` draws
    /// purely by depth.
    pub priorities: Option<Vec<u8>>,
}

/// Apply a light value to a face colour.
///
/// For an untextured face (bit 1 of `kind` clear) only the lightness
/// changes, clamped to 2–126. For a textured one it's a shade, 0 (fully
/// lit) to 127 (darkest).
pub fn lit_colour(hsl: u16, light: i32, kind: u8) -> i32 {
    if kind & 2 == 2 {
        return 127 - light.clamp(0, 127);
    }
    // Scale the low 7 lightness bits by light / 128; hue and saturation in
    // the top 9 bits pass through.
    let lum = ((light * i32::from(hsl & 0x7f)) >> 7).clamp(2, 126);
    i32::from(hsl & 0xff80) + lum
}

/// Light a model with `l`. A model without face types is all smooth.
pub fn light(m: &Model, l: &Lighting) -> LitModel {
    let [lx, ly, lz] = l.direction;
    let len = (f64::from(lx * lx + ly * ly + lz * lz)).sqrt() as i32;
    // The light's scale: divisor base for every light value below.
    let k = (l.contrast * len) >> 8;
    let n = m.faces.len();
    let kinds: Vec<u8> = (0..n)
        .map(|i| m.face_types.as_ref().map_or(0, |t| t[i] & 3))
        .collect();
    // Per vertex: summed smooth-face normal (x, y, z) and how many faces
    // went into it.
    let mut acc = vec![[0i32; 4]; m.vertices.len()];
    let mut colours = vec![[0i32; 3]; n];
    for (i, f) in m.faces.iter().enumerate() {
        // Face normal: cross product of the two edges from corner a.
        let [a, b, c] = f.map(|v| m.vertices[usize::from(v)]);
        let (d1, d2) = (
            [b[0] - a[0], b[1] - a[1], b[2] - a[2]],
            [c[0] - a[0], c[1] - a[1], c[2] - a[2]],
        );
        let mut nx = d1[1] * d2[2] - d2[1] * d1[2];
        let mut ny = d1[2] * d2[0] - d2[2] * d1[0];
        let mut nz = d1[0] * d2[1] - d2[0] * d1[1];
        // Halve until each part is within ±8192, so the squares can't
        // overflow 32 bits.
        while !(-8192..=8192).contains(&nx)
            || !(-8192..=8192).contains(&ny)
            || !(-8192..=8192).contains(&nz)
        {
            nx >>= 1;
            ny >>= 1;
            nz >>= 1;
        }
        // nl is the normal's length; rescale it to about 256.
        let nl = ((f64::from(nx * nx + ny * ny + nz * nz)).sqrt() as i32).max(1);
        let (nx, ny, nz) = (nx * 256 / nl, ny * 256 / nl, nz * 256 / nl);
        // Smooth faces add to their vertices' sums; flat faces are lit now.
        if kinds[i] & 1 == 0 {
            for v in *f {
                let s = &mut acc[usize::from(v)];
                s[0] += nx;
                s[1] += ny;
                s[2] += nz;
                s[3] += 1;
            }
        } else {
            // Flat faces divide by 1.5k.
            let light = l.ambient + (lx * nx + ly * ny + lz * nz) / (k + k / 2).max(1);
            colours[i][0] = lit_colour(
                m.face_colours[i],
                light,
                m.face_types.as_ref().map_or(0, |t| t[i]),
            );
        }
    }
    // Second pass: light each smooth corner from its vertex's summed normal.
    for (i, f) in m.faces.iter().enumerate() {
        if kinds[i] & 1 != 0 {
            continue;
        }
        let kind = m.face_types.as_ref().map_or(0, |t| t[i]);
        for (corner, v) in f.iter().enumerate() {
            // s = [Σx, Σy, Σz, face count]; the count scales the divisor.
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
    /// A model of flat `0xRRGGBB` triangles with no lighting, for terrain.
    /// `rgb[i]` is face `i`'s colour.
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
