//! `models` record: one triangle mesh in pack space.
//!
//! ```text
//! magic "RMDL", version u8 = 2
//! vertexCount u16, faceCount u16, flags u8, attachCount u8
//! vertices      vertexCount × {x, y, z i16}          (x = cache x, y = −cache y, z = −cache z)
//! faces         faceCount × {a, b, c}                (u8 when vertexCount < 256, else u16)
//! materialSlot  faceCount × u8                       (material id per face)
//! paletteIndex  faceCount × u16                      (palette entry per face)
//! [UV]          faceCount × {u0, v0, u1, v1, u2, v2 u8}
//! [ALPHA]       faceCount × u8                       (255 = opaque)
//! [PRIORITY]    faceCount × u8                       (draw-order priority)
//! [LABELS]      vertexCount × u8                     (vertex label per vertex)
//! attach points attachCount × {nameHash u32, x, y, z i16, yaw u8}
//! ```
//!
//! Bracketed sections are there only when their [`flags`] bit is set, and
//! always in this order. No flag says how wide face indices are: the vertex
//! count does, so read it first.

use crate::bytes::{Reader, Writer};

use super::{RecordError, finish, magic};

/// Magic at the start of a model record.
pub const MAGIC: &[u8; 4] = b"RMDL";

/// Bits of the header's `flags` byte, one per optional section. Bits 4–7
/// are unused.
pub mod flags {
    /// Per-face texture coordinates follow.
    pub const UV: u8 = 1;
    /// Per-face alpha follows.
    pub const ALPHA: u8 = 2;
    /// Per-face draw-order priority follows.
    pub const PRIORITY: u8 = 4;
    /// Per-vertex labels follow.
    pub const LABELS: u8 = 8;
}

/// A named point on the model.
///
/// The format carries these, but the conversions never write any, so in
/// practice `attachCount` is 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttachPoint {
    /// Hash of the point's name.
    pub name_hash: u32,
    /// Position in pack space.
    pub position: [i16; 3],
    /// Yaw. Nothing here pins down its units.
    pub yaw: u8,
}

/// A decoded `models` record.
///
/// The per-face vectors all have one entry per face and `labels` has one
/// per vertex; [`encode`](Self::encode) and [`decode`](Self::decode) both
/// check that, and that every face index is in range.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackModel {
    /// Vertex positions in pack space (y and z negated from the cache).
    pub vertices: Vec<[i16; 3]>,
    /// Triangles as indices into `vertices`. Winding is the cache's, since
    /// the axis flip is a rotation.
    pub faces: Vec<[u16; 3]>,
    /// Material id per face, so only materials 0–255 are reachable.
    pub material_slots: Vec<u8>,
    /// Palette entry per face, an index into the `materials` pack's palette
    /// record.
    pub palette_indices: Vec<u16>,
    /// `(u, v)` for corners a, b and c per face, 0–255 across the texture.
    /// `None` when the model has no UV section.
    pub uvs: Option<Vec<[u8; 6]>>,
    /// Opacity per face; 255 is opaque. `None` when every face is opaque.
    pub alphas: Option<Vec<u8>>,
    /// Draw-order priority per face. `None` when the section's left out.
    pub priorities: Option<Vec<u8>>,
    /// Label per vertex, which a rig binds to a bone. `None` means every
    /// vertex has label 0.
    pub labels: Option<Vec<u8>>,
    /// Named points; at most 255.
    pub attach_points: Vec<AttachPoint>,
}

impl PackModel {
    /// The header's flags byte, worked out from which optional sections are
    /// `Some`.
    pub fn flags(&self) -> u8 {
        let mut f = 0;
        if self.uvs.is_some() {
            f |= flags::UV;
        }
        if self.alphas.is_some() {
            f |= flags::ALPHA;
        }
        if self.priorities.is_some() {
            f |= flags::PRIORITY;
        }
        if self.labels.is_some() {
            f |= flags::LABELS;
        }
        f
    }

    fn check(&self) -> Result<(), RecordError> {
        let (v, f) = (self.vertices.len(), self.faces.len());
        let bad = |what: &str| Err(RecordError::Invalid(what.to_owned()));
        if v > usize::from(u16::MAX) || f > usize::from(u16::MAX) || self.attach_points.len() > 255
        {
            return bad("too many vertices, faces or attach points");
        }
        if self.material_slots.len() != f || self.palette_indices.len() != f {
            return bad("per-face arrays must have one entry per face");
        }
        let per_face = [&self.alphas, &self.priorities]
            .into_iter()
            .flatten()
            .all(|a| a.len() == f);
        if !per_face || self.uvs.as_ref().is_some_and(|u| u.len() != f) {
            return bad("optional per-face arrays must have one entry per face");
        }
        if self.labels.as_ref().is_some_and(|l| l.len() != v) {
            return bad("labels must have one entry per vertex");
        }
        if self.faces.iter().flatten().any(|&i| usize::from(i) >= v) {
            return bad("face index out of range");
        }
        Ok(())
    }

    /// Encode the record, after checking its invariants.
    pub fn encode(&self) -> Result<Vec<u8>, RecordError> {
        self.check()?;
        // Under 256 vertices every index fits a byte, so faces use u8s.
        let small = self.vertices.len() < 256;
        let mut w = Writer::default();
        w.bytes(MAGIC)
            .u8(super::VERSION)
            .u16(self.vertices.len() as u16)
            .u16(self.faces.len() as u16)
            .u8(self.flags())
            .u8(self.attach_points.len() as u8);
        for v in &self.vertices {
            w.i16(v[0]).i16(v[1]).i16(v[2]);
        }
        for f in &self.faces {
            for &i in f {
                if small {
                    w.u8(i as u8);
                } else {
                    w.u16(i);
                }
            }
        }
        w.bytes(&self.material_slots);
        for &p in &self.palette_indices {
            w.u16(p);
        }
        for uv in self.uvs.iter().flatten() {
            w.bytes(uv);
        }
        // Alpha then priority, each only if present.
        for a in [&self.alphas, &self.priorities].into_iter().flatten() {
            w.bytes(a);
        }
        if let Some(l) = &self.labels {
            w.bytes(l);
        }
        for a in &self.attach_points {
            w.u32(a.name_hash)
                .i16(a.position[0])
                .i16(a.position[1])
                .i16(a.position[2])
                .u8(a.yaw);
        }
        Ok(w.0)
    }

    /// Decode a record and check its invariants.
    pub fn decode(data: &[u8]) -> Result<Self, RecordError> {
        let mut r = Reader::new(data);
        magic(&mut r, MAGIC)?;
        // Vertex and face counts.
        let (v, f) = (usize::from(r.u16()?), usize::from(r.u16()?));
        // Which optional sections follow.
        let fl = r.u8()?;
        let attach = usize::from(r.u8()?);
        let small = v < 256;
        let mut m = PackModel::default();
        for _ in 0..v {
            m.vertices.push([r.i16()?, r.i16()?, r.i16()?]);
        }
        for _ in 0..f {
            let mut face = [0u16; 3];
            for i in &mut face {
                *i = if small { r.u8()?.into() } else { r.u16()? };
            }
            m.faces.push(face);
        }
        m.material_slots = r.bytes(f)?.to_vec();
        m.palette_indices = (0..f).map(|_| r.u16()).collect::<Result<_, _>>()?;
        if fl & flags::UV != 0 {
            m.uvs = Some(
                (0..f)
                    .map(|_| r.bytes(6).map(|b| b.try_into().expect("6 bytes")))
                    .collect::<Result<_, _>>()?,
            );
        }
        if fl & flags::ALPHA != 0 {
            m.alphas = Some(r.bytes(f)?.to_vec());
        }
        if fl & flags::PRIORITY != 0 {
            m.priorities = Some(r.bytes(f)?.to_vec());
        }
        if fl & flags::LABELS != 0 {
            m.labels = Some(r.bytes(v)?.to_vec());
        }
        for _ in 0..attach {
            m.attach_points.push(AttachPoint {
                name_hash: r.u32()?,
                position: [r.i16()?, r.i16()?, r.i16()?],
                yaw: r.u8()?,
            });
        }
        finish(&r)?;
        m.check()?;
        Ok(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn synthetic(vertices: usize) -> PackModel {
        let vs: Vec<[i16; 3]> = (0..vertices).map(|i| [i as i16, -(i as i16), 3]).collect();
        let faces: Vec<[u16; 3]> = (0..vertices.saturating_sub(2))
            .map(|i| [i as u16, i as u16 + 1, i as u16 + 2])
            .collect();
        let n = faces.len();
        PackModel {
            vertices: vs,
            material_slots: vec![0; n],
            palette_indices: (0..n).map(|i| i as u16 * 3).collect(),
            uvs: Some(vec![[1, 2, 3, 4, 5, 6]; n]),
            alphas: Some(vec![200; n]),
            priorities: None,
            labels: Some((0..vertices).map(|i| (i % 7) as u8).collect()),
            attach_points: vec![AttachPoint {
                name_hash: 42,
                position: [1, 2, 3],
                yaw: 64,
            }],
            faces,
        }
    }

    #[test]
    fn round_trips_with_byte_and_word_indices() {
        for n in [4, 300] {
            let m = synthetic(n);
            let bytes = m.encode().unwrap();
            assert_eq!(PackModel::decode(&bytes).unwrap(), m, "{n} vertices");
            assert_eq!(PackModel::decode(&bytes).unwrap().encode().unwrap(), bytes);
        }
    }

    #[test]
    fn rejects_bad_input() {
        let mut m = synthetic(4);
        m.faces[0][0] = 9;
        assert!(m.encode().is_err());
        let bytes = synthetic(4).encode().unwrap();
        assert!(PackModel::decode(&bytes[..bytes.len() - 1]).is_err());
        assert!(matches!(
            PackModel::decode(&[bytes.clone(), vec![0]].concat()),
            Err(RecordError::Trailing(1))
        ));
        assert!(matches!(
            PackModel::decode(b"XXXX\x01"),
            Err(RecordError::Magic { .. })
        ));
    }
}
