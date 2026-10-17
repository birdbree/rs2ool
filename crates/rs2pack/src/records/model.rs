//! `models` record.
//!
//! ```text
//! magic "RMDL", version u8 = 2
//! vertexCount u16, faceCount u16, flags u8, attachCount u8
//! vertices      vertexCount × {x, y, z i16}          (y up, z = −cache z)
//! faces         faceCount × {a, b, c}                (u8 when vertexCount < 256, else u16)
//! materialSlot  faceCount × u8
//! paletteIndex  faceCount × u16
//! [UV]          faceCount × {u0, v0, u1, v1, u2, v2 u8}
//! [ALPHA]       faceCount × u8                       (255 = opaque)
//! [PRIORITY]    faceCount × u8                       (draw-order render flag)
//! [LABELS]      vertexCount × u8
//! attach points attachCount × {nameHash u32, x, y, z i16, yaw u8}
//! ```

use crate::bytes::{Reader, Writer};

use super::{RecordError, finish, magic};

pub const MAGIC: &[u8; 4] = b"RMDL";

pub mod flags {
    pub const UV: u8 = 1;
    pub const ALPHA: u8 = 2;
    pub const PRIORITY: u8 = 4;
    pub const LABELS: u8 = 8;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttachPoint {
    pub name_hash: u32,
    pub position: [i16; 3],
    pub yaw: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackModel {
    pub vertices: Vec<[i16; 3]>,
    pub faces: Vec<[u16; 3]>,
    pub material_slots: Vec<u8>,
    pub palette_indices: Vec<u16>,
    pub uvs: Option<Vec<[u8; 6]>>,
    pub alphas: Option<Vec<u8>>,
    pub priorities: Option<Vec<u8>>,
    pub labels: Option<Vec<u8>>,
    pub attach_points: Vec<AttachPoint>,
}

impl PackModel {
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

    pub fn encode(&self) -> Result<Vec<u8>, RecordError> {
        self.check()?;
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

    pub fn decode(data: &[u8]) -> Result<Self, RecordError> {
        let mut r = Reader::new(data);
        magic(&mut r, MAGIC)?;
        let (v, f) = (usize::from(r.u16()?), usize::from(r.u16()?));
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
