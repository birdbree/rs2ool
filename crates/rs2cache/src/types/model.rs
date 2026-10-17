//! Models from index 1.

use serde::{Deserialize, Serialize};

/// A decoded 317 model, in the cache's own coordinates: right-handed with y
/// pointing down, so negative y is up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Model {
    /// Vertex positions `[x, y, z]` in model units.
    pub vertices: Vec<[i32; 3]>,
    /// Vertex indices per triangle.
    pub faces: Vec<[u16; 3]>,
    /// 16-bit HSL colour per face (`hue 6, sat 3, lum 7`). On a textured
    /// face it's the texture id (0–49) instead.
    pub face_colours: Vec<u16>,
    /// Per-face render type, when present: low 2 bits 0 = Gouraud, 1 = flat,
    /// 2 = textured Gouraud, 3 = textured flat; `type >> 2` indexes
    /// `textured_faces`. `None` means every face is Gouraud.
    pub face_types: Option<Vec<u8>>,
    /// The priority of every face when `face_priorities` is absent; 0 when
    /// it's present.
    pub priority: u8,
    /// Per-face draw priority, when the trailer's priority byte is 255.
    pub face_priorities: Option<Vec<u8>>,
    /// Per-face alpha: 0 is opaque and larger values are more see-through.
    /// `None` means every face is opaque.
    pub face_alphas: Option<Vec<u8>>,
    /// Labels grouping faces for alpha transforms.
    pub face_labels: Option<Vec<u8>>,
    /// Labels grouping vertices for skeletal transforms.
    pub vertex_labels: Option<Vec<u8>>,
    /// Texture-space triangles `[P, M, N]` as vertex indices.
    pub textured_faces: Vec<[u16; 3]>,
}

impl Model {
    /// Axis-aligned bounds `(min, max)`, or `None` for a model with no
    /// vertices.
    pub fn bounds(&self) -> Option<([i32; 3], [i32; 3])> {
        let first = *self.vertices.first()?;
        Some(
            self.vertices
                .iter()
                // Running per-axis minimum (`lo`) and maximum (`hi`).
                .fold((first, first), |(mut lo, mut hi), v| {
                    for a in 0..3 {
                        lo[a] = lo[a].min(v[a]);
                        hi[a] = hi[a].max(v[a]);
                    }
                    (lo, hi)
                }),
        )
    }
}
