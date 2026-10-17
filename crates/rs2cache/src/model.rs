//! Model decoder (index 1).
//!
//! The last 18 bytes of the inflated file are a trailer. A flag counts only
//! when its byte is exactly 1.
//!
//! ```text
//! vertexCount        u16
//! faceCount          u16
//! texturedFaceCount  u8
//! hasFaceTypes       u8
//! priority           u8    255 = per-face priorities follow
//! hasAlpha           u8
//! hasFaceLabels      u8
//! hasVertexLabels    u8
//! xLen, yLen, zLen   u16   byte lengths of the three delta streams
//! faceIndexLen       u16   byte length of the face-index stream
//! ```
//!
//! The sections run back to back from offset 0. Optional ones take no space
//! when absent, and spare bytes before the trailer are ignored:
//!
//! ```text
//! vertex flags       u8 × vertexCount     bit 1/2/4: x/y/z delta follows
//! face compression   u8 × faceCount       1–4, see decode
//! face priorities    u8 × faceCount       if priority == 255
//! face labels        u8 × faceCount       if hasFaceLabels
//! face types         u8 × faceCount       if hasFaceTypes
//! vertex labels      u8 × vertexCount     if hasVertexLabels
//! face alphas        u8 × faceCount       if hasAlpha
//! face indices       faceIndexLen bytes   signed smarts
//! face colours       u16 × faceCount
//! textured faces     u16 × 3 × texturedFaceCount
//! x, y, z deltas     xLen, yLen, zLen bytes of signed smarts
//! ```
//!
//! Yes, priorities come before labels, and face labels before face types.
//! [`Layout::compute`] has the same order in code.

use crate::types::model::Model;
use thiserror::Error;

use crate::io::{Cursor, IoError};

const TRAILER: usize = 18;

/// A model that wouldn't decode.
#[derive(Debug, Error)]
pub enum ModelError {
    /// The file is shorter than the trailer.
    #[error("{len} bytes is too short for the 18-byte model trailer")]
    TooShort {
        /// File length.
        len: usize,
    },
    /// The trailer's sections don't fit before it.
    #[error("sections need {need} bytes before the trailer but {have} are present")]
    SectionOverrun {
        /// Bytes the sections add up to.
        need: usize,
        /// Bytes before the trailer.
        have: usize,
    },
    /// A stream ran out mid-read, say a delta stream shorter than its flags
    /// promise.
    #[error("truncated section: {0}")]
    Truncated(#[from] IoError),
    /// A face compression byte isn't 1–4.
    #[error("face {face} has compression type {kind}; expected 1 to 4")]
    BadCompressionType {
        /// Face number.
        face: usize,
        /// The byte found.
        kind: u8,
    },
    /// A face index is negative or not below the vertex count.
    #[error("face {face} uses vertex {index} but the model has {vertices}")]
    FaceIndex {
        /// Face number.
        face: usize,
        /// The decoded index.
        index: i32,
        /// Vertex count.
        vertices: usize,
    },
    /// A textured-face index isn't below the vertex count.
    #[error("textured face {face} uses vertex {index} but the model has {vertices}")]
    TexturedFaceIndex {
        /// Textured-face number.
        face: usize,
        /// The index found.
        index: u16,
        /// Vertex count.
        vertices: usize,
    },
}

/// The trailer's counts and flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    /// Number of vertices.
    pub vertex_count: u16,
    /// Number of faces.
    pub face_count: u16,
    /// Number of textured-face triples.
    pub textured_face_count: u8,
    /// A face-type byte per face follows.
    pub has_face_types: bool,
    /// 255 means per-face priorities follow; anything else is the priority
    /// of every face.
    pub priority: u8,
    /// An alpha byte per face follows.
    pub has_alpha: bool,
    /// A face-label byte per face follows.
    pub has_face_labels: bool,
    /// A vertex-label byte per vertex follows.
    pub has_vertex_labels: bool,
    /// Byte length of the x delta stream.
    pub x_len: u16,
    /// Byte length of the y delta stream.
    pub y_len: u16,
    /// Byte length of the z delta stream.
    pub z_len: u16,
    /// Byte length of the face-index stream.
    pub face_index_len: u16,
}

impl Header {
    /// Read the trailer from the last 18 bytes of an inflated model.
    pub fn read(data: &[u8]) -> Result<Self, ModelError> {
        let at = data
            .len()
            .checked_sub(TRAILER)
            .ok_or(ModelError::TooShort { len: data.len() })?;
        let mut c = Cursor::at(data, at)?;
        Ok(Self {
            vertex_count: c.u16()?,
            face_count: c.u16()?,
            textured_face_count: c.u8()?,
            has_face_types: c.u8()? == 1,
            priority: c.u8()?,
            has_alpha: c.u8()? == 1,
            has_face_labels: c.u8()? == 1,
            has_vertex_labels: c.u8()? == 1,
            x_len: c.u16()?,
            y_len: c.u16()?,
            z_len: c.u16()?,
            face_index_len: c.u16()?,
        })
    }
}

/// Start offsets of every section; optional sections are `None` when absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// Vertex flag bytes.
    pub vertex_flags: usize,
    /// Face compression bytes.
    pub face_compression: usize,
    /// Per-face priorities.
    pub face_priorities: Option<usize>,
    /// Face labels.
    pub face_labels: Option<usize>,
    /// Face types.
    pub face_types: Option<usize>,
    /// Vertex labels.
    pub vertex_labels: Option<usize>,
    /// Face alphas.
    pub face_alphas: Option<usize>,
    /// Face-index stream.
    pub face_indices: usize,
    /// Face colours.
    pub face_colours: usize,
    /// Textured-face triples.
    pub textured_faces: usize,
    /// X delta stream.
    pub x: usize,
    /// Y delta stream.
    pub y: usize,
    /// Z delta stream.
    pub z: usize,
    /// Where the sections end; the trailer should start here.
    pub end: usize,
}

impl Layout {
    /// Lay the sections out from the trailer's counts and flags, in file
    /// order.
    pub fn compute(h: &Header) -> Self {
        let (v, f) = (usize::from(h.vertex_count), usize::from(h.face_count));
        // Hand out `n` bytes from the running offset; returns where they start.
        let mut at = 0;
        let mut take = |n: usize| {
            let start = at;
            at += n;
            start
        };
        let vertex_flags = take(v);
        let face_compression = take(f);
        let face_priorities = (h.priority == 255).then(|| take(f));
        let face_labels = h.has_face_labels.then(|| take(f));
        let face_types = h.has_face_types.then(|| take(f));
        let vertex_labels = h.has_vertex_labels.then(|| take(v));
        let face_alphas = h.has_alpha.then(|| take(f));
        let face_indices = take(h.face_index_len.into());
        let face_colours = take(f * 2);
        // Three u16 vertex indices per triple.
        let textured_faces = take(usize::from(h.textured_face_count) * 6);
        let x = take(h.x_len.into());
        let y = take(h.y_len.into());
        let z = take(h.z_len.into());
        Self {
            vertex_flags,
            face_compression,
            face_priorities,
            face_labels,
            face_types,
            vertex_labels,
            face_alphas,
            face_indices,
            face_colours,
            textured_faces,
            x,
            y,
            z,
            end: at,
        }
    }
}

/// Decode an inflated model file.
///
/// Faces are built from the compression byte and the previous face:
///
/// ```text
/// 1   a = next; b = next; c = next
/// 2   b = c; c = next          (a kept)
/// 3   a = c; c = next          (b kept)
/// 4   swap a and b; c = next
/// ```
///
/// `next` adds the next signed smart to a running index that starts at 0
/// and carries over from face to face.
pub fn decode(data: &[u8]) -> Result<Model, ModelError> {
    let h = Header::read(data)?;
    let l = Layout::compute(&h);
    let body = &data[..data.len() - TRAILER];
    if l.end > body.len() {
        return Err(ModelError::SectionOverrun {
            need: l.end,
            have: body.len(),
        });
    }
    let (vc, fc) = (usize::from(h.vertex_count), usize::from(h.face_count));
    let bytes = |at: usize, n: usize| -> Result<Vec<u8>, ModelError> {
        Ok(Cursor::at(body, at)?.bytes(n)?.to_vec())
    };

    // Vertices: a flag byte per vertex says which of x, y, z deltas follow.
    let flags = bytes(l.vertex_flags, vc)?;
    let (mut xs, mut ys, mut zs) = (
        Cursor::at(body, l.x)?,
        Cursor::at(body, l.y)?,
        Cursor::at(body, l.z)?,
    );
    let mut vertices = Vec::with_capacity(vc);
    // An axis whose bit is clear keeps the previous vertex's value.
    let mut prev = [0i32; 3];
    for f in flags {
        for (axis, c) in [&mut xs, &mut ys, &mut zs].into_iter().enumerate() {
            if f & (1 << axis) != 0 {
                prev[axis] += i32::from(c.signed_smart()?);
            }
        }
        vertices.push(prev);
    }

    // Faces: type 1 reads three new indices; types 2–4 keep two of the
    // previous face's and read one. Every index is a signed-smart delta from
    // the last index read.
    let mut kinds = Cursor::at(body, l.face_compression)?;
    let mut deltas = Cursor::at(body, l.face_indices)?;
    let mut faces = Vec::with_capacity(fc);
    // `a`, `b`, `c` are the current triangle; `last` is the running index.
    let (mut a, mut b, mut c, mut last) = (0i32, 0i32, 0i32, 0i32);
    for face in 0..fc {
        let mut next = || -> Result<i32, IoError> {
            last += i32::from(deltas.signed_smart()?);
            Ok(last)
        };
        match kinds.u8()? {
            1 => {
                a = next()?;
                b = next()?;
                c = next()?;
            }
            2 => {
                b = c;
                c = next()?;
            }
            3 => {
                a = c;
                c = next()?;
            }
            4 => {
                std::mem::swap(&mut a, &mut b);
                c = next()?;
            }
            kind => return Err(ModelError::BadCompressionType { face, kind }),
        }
        // Range-check in i32 first: a bad delta can run negative.
        let check = |index: i32| {
            u16::try_from(index)
                .ok()
                .filter(|&i| usize::from(i) < vc)
                .ok_or(ModelError::FaceIndex {
                    face,
                    index,
                    vertices: vc,
                })
        };
        faces.push([check(a)?, check(b)?, check(c)?]);
    }

    let mut colours = Cursor::at(body, l.face_colours)?;
    let face_colours = (0..fc).map(|_| colours.u16()).collect::<Result<_, _>>()?;

    let mut tex = Cursor::at(body, l.textured_faces)?;
    let mut textured_faces = Vec::with_capacity(h.textured_face_count.into());
    for face in 0..usize::from(h.textured_face_count) {
        let mut t = [0u16; 3];
        for v in &mut t {
            *v = tex.u16()?;
            if usize::from(*v) >= vc {
                return Err(ModelError::TexturedFaceIndex {
                    face,
                    index: *v,
                    vertices: vc,
                });
            }
        }
        textured_faces.push(t);
    }

    // Read an optional byte section, keeping `None` when it's absent.
    let optional = |at: Option<usize>, n: usize| at.map(|at| bytes(at, n)).transpose();
    Ok(Model {
        vertices,
        faces,
        face_colours,
        face_types: optional(l.face_types, fc)?,
        // 255 only says "per-face priorities follow"; it's not a priority.
        priority: if h.priority == 255 { 0 } else { h.priority },
        face_priorities: optional(l.face_priorities, fc)?,
        face_alphas: optional(l.face_alphas, fc)?,
        face_labels: optional(l.face_labels, fc)?,
        vertex_labels: optional(l.vertex_labels, vc)?,
        textured_faces,
    })
}

#[cfg(test)]
pub(crate) mod synth {
    //! Encode models for tests.

    use crate::types::model::Model;

    fn signed_smart(out: &mut Vec<u8>, v: i32) {
        if (-64..64).contains(&v) {
            out.push((v + 64) as u8);
        } else {
            out.extend_from_slice(&((v + 49152) as u16).to_be_bytes());
        }
    }

    /// Encode with every face as compression type 1.
    pub fn encode(m: &Model) -> Vec<u8> {
        let (v, f) = (m.vertices.len(), m.faces.len());
        let mut flags = Vec::new();
        let mut axes = [Vec::new(), Vec::new(), Vec::new()];
        let mut prev = [0; 3];
        for p in &m.vertices {
            let mut flag = 0;
            for a in 0..3 {
                if p[a] != prev[a] {
                    flag |= 1 << a;
                    signed_smart(&mut axes[a], p[a] - prev[a]);
                }
            }
            flags.push(flag);
            prev = *p;
        }
        let mut indices = Vec::new();
        let mut last = 0;
        for face in &m.faces {
            for &i in face {
                signed_smart(&mut indices, i32::from(i) - last);
                last = i32::from(i);
            }
        }
        let mut out = flags;
        out.extend(std::iter::repeat_n(1u8, f));
        if let Some(p) = &m.face_priorities {
            out.extend(p);
        }
        if let Some(x) = &m.face_labels {
            out.extend(x);
        }
        if let Some(x) = &m.face_types {
            out.extend(x);
        }
        if let Some(x) = &m.vertex_labels {
            out.extend(x);
        }
        if let Some(x) = &m.face_alphas {
            out.extend(x);
        }
        out.extend(&indices);
        for c in &m.face_colours {
            out.extend(c.to_be_bytes());
        }
        for t in &m.textured_faces {
            for i in t {
                out.extend(i.to_be_bytes());
            }
        }
        for a in &axes {
            out.extend(a);
        }
        out.extend((v as u16).to_be_bytes());
        out.extend((f as u16).to_be_bytes());
        out.push(m.textured_faces.len() as u8);
        out.push(m.face_types.is_some().into());
        out.push(if m.face_priorities.is_some() {
            255
        } else {
            m.priority
        });
        out.push(m.face_alphas.is_some().into());
        out.push(m.face_labels.is_some().into());
        out.push(m.vertex_labels.is_some().into());
        for a in &axes {
            out.extend((a.len() as u16).to_be_bytes());
        }
        out.extend((indices.len() as u16).to_be_bytes());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tetra(all_sections: bool) -> Model {
        let some = |v: Vec<u8>| all_sections.then_some(v);
        Model {
            vertices: vec![[0, 0, 0], [100, 0, 0], [0, -200, 0], [0, 0, 300]],
            faces: vec![[0, 1, 2], [0, 2, 3], [3, 1, 0], [1, 2, 3]],
            face_colours: vec![1, 2, 3, 0xffff],
            face_types: some(vec![0, 1, 2, 3]),
            priority: if all_sections { 0 } else { 7 },
            face_priorities: some(vec![1, 2, 3, 4]),
            face_alphas: some(vec![0, 64, 128, 255]),
            face_labels: some(vec![5, 5, 6, 6]),
            vertex_labels: some(vec![1, 2, 3, 4]),
            textured_faces: if all_sections {
                vec![[0, 1, 2]]
            } else {
                vec![]
            },
        }
    }

    #[test]
    fn round_trips_with_every_section() {
        let m = tetra(true);
        assert_eq!(decode(&synth::encode(&m)).unwrap(), m);
    }

    #[test]
    fn round_trips_with_no_optional_section() {
        let m = tetra(false);
        assert_eq!(decode(&synth::encode(&m)).unwrap(), m);
    }

    #[test]
    fn reused_index_compression() {
        // Hand-built faces: type 1 (0,1,2), type 2 keeps a and old c as b,
        // type 3 keeps old c as a and b, type 4 swaps a and b.
        let mut m = tetra(false);
        m.faces = vec![[0, 1, 2]];
        let mut data = synth::encode(&m);
        // Patch in three more faces of types 2, 3, 4, each adding vertex 3.
        let h = Header::read(&data).unwrap();
        let l = Layout::compute(&h);
        let body_len = data.len() - TRAILER;
        let mut body = data[..body_len].to_vec();
        let mut trailer = data[body_len..].to_vec();
        body.splice(l.face_compression + 1..l.face_compression + 1, [2, 3, 4]);
        body.splice(l.face_colours + 3..l.face_colours + 3, [0, 0, 0, 0, 0, 0]);
        let at = l.face_indices + 3 + 3;
        body.splice(at..at, [64 + 1, 64, 64]);
        trailer[2..4].copy_from_slice(&4u16.to_be_bytes());
        trailer[16..18].copy_from_slice(&6u16.to_be_bytes());
        data = [body, trailer].concat();
        let d = decode(&data).unwrap();
        assert_eq!(d.faces, [[0, 1, 2], [0, 2, 3], [3, 2, 3], [2, 3, 3]]);
    }

    #[test]
    fn bad_input_is_an_error() {
        assert!(matches!(decode(&[0; 10]), Err(ModelError::TooShort { .. })));
        let mut data = synth::encode(&tetra(true));
        let n = data.len();
        data[n - 18..n - 16].copy_from_slice(&1000u16.to_be_bytes());
        assert!(decode(&data).is_err());

        let mut m = tetra(false);
        m.faces[1] = [0, 2, 9];
        assert!(matches!(
            decode(&synth::encode(&m)),
            Err(ModelError::FaceIndex {
                face: 1,
                index: 9,
                vertices: 4
            })
        ));
    }
}
