//! Models from index 1.

use serde::{Deserialize, Serialize};

/// A decoded 317 model, in the cache's own coordinates (y is down-negative).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Model {
    pub vertices: Vec<[i32; 3]>,
    /// Vertex indices per triangle.
    pub faces: Vec<[u16; 3]>,
    /// 16-bit HSL colour per face (`hue 6, sat 3, lum 7`).
    pub face_colours: Vec<u16>,
    /// Per-face render type, when present: low 2 bits 0 = Gouraud, 1 = flat,
    /// 2 = textured Gouraud, 3 = textured flat; `type >> 2` indexes
    /// `textured_faces`.
    pub face_types: Option<Vec<u8>>,
    /// The priority of every face when `face_priorities` is absent.
    pub priority: u8,
    pub face_priorities: Option<Vec<u8>>,
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
