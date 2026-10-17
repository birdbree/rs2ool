//! Model conversion: 317 model → `models` record.

use glam::DVec3;
use rs2cache::types::model::Model;
use rs2pack::records::model::PackModel;

use crate::palette::{Palette, is_textured};
use crate::{ConvertError, material_id};

/// Texture-space UVs for a face's corners, from its P/M/N triangle: each
/// vertex is projected onto the plane spanned by `M − P` and `N − P`, with u
/// along `M − P` and v along `N − P`, then scaled to 0–255 (clamped, so
/// coordinates outside the triangle's unit square saturate).
pub fn face_uv(corners: [DVec3; 3], p: DVec3, m: DVec3, n: DVec3) -> [u8; 6] {
    let (e1, e2) = (m - p, n - p);
    let (a, b, c) = (e1.dot(e1), e1.dot(e2), e2.dot(e2));
    let det = a * c - b * b;
    let mut out = [0u8; 6];
    for (i, v) in corners.iter().enumerate() {
        let d = *v - p;
        let (d1, d2) = (d.dot(e1), d.dot(e2));
        let (u, w) = if det.abs() < 1e-9 {
            (0.0, 0.0)
        } else {
            ((c * d1 - b * d2) / det, (a * d2 - b * d1) / det)
        };
        out[i * 2] = (u * 255.0).round().clamp(0.0, 255.0) as u8;
        out[i * 2 + 1] = (w * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    out
}

fn to_i16(v: i32, what: &str) -> Result<i16, ConvertError> {
    i16::try_from(v).map_err(|_| ConvertError::Invalid(format!("{what} {v} does not fit i16")))
}

/// Convert a decoded model into pack space ([`space::to_pack`]: y and z
/// negated, a rotation). Faces keep their winding.
///
/// [`space::to_pack`]: crate::space::to_pack
pub fn convert(m: &Model, palette: &Palette) -> Result<PackModel, ConvertError> {
    let vertices = m
        .vertices
        .iter()
        .map(|&v| {
            let [x, y, z] = crate::space::to_pack(v);
            Ok([to_i16(x, "x")?, to_i16(y, "y")?, to_i16(z, "z")?])
        })
        .collect::<Result<Vec<_>, ConvertError>>()?;
    let f = m.faces.len();
    let mut material_slots = vec![material_id::PALETTE as u8; f];
    let mut palette_indices = vec![0u16; f];
    let mut uvs = vec![[0u8; 6]; f];
    let mut any_textured = false;
    let pos = |i: u16| {
        let v = m.vertices[usize::from(i)];
        DVec3::new(v[0].into(), v[1].into(), v[2].into())
    };
    for i in 0..f {
        let colour = m.face_colours[i];
        if is_textured(m, i) {
            // A textured face's colour is its texture id.
            let types = m.face_types.as_ref().expect("textured implies types");
            let triple = usize::from(types[i] >> 2);
            let slot = material_id::TEXTURE_BASE + colour;
            if colour < 50
                && let Some(&[p, mm, n]) = m.textured_faces.get(triple)
            {
                any_textured = true;
                material_slots[i] = slot as u8;
                let face = m.faces[i];
                uvs[i] = face_uv(face.map(pos), pos(p), pos(mm), pos(n));
                continue;
            }
            tracing::warn!(
                face = i,
                colour,
                triple,
                "textured face without a usable texture; drawn as colour"
            );
        }
        palette_indices[i] = palette.index_of(colour);
    }
    let alphas = m
        .face_alphas
        .as_ref()
        .map(|a| a.iter().map(|&t| 255 - t).collect());
    let priorities = match &m.face_priorities {
        Some(p) => Some(p.clone()),
        None if m.priority != 0 => Some(vec![m.priority; f]),
        None => None,
    };
    Ok(PackModel {
        vertices,
        faces: m.faces.clone(),
        material_slots,
        palette_indices,
        uvs: any_textured.then_some(uvs),
        alphas,
        priorities,
        labels: m.vertex_labels.clone(),
        attach_points: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uv_projection_maps_the_triangle_to_the_unit_corners() {
        let (p, m, n) = (
            DVec3::ZERO,
            DVec3::new(10.0, 0.0, 0.0),
            DVec3::new(0.0, 0.0, 10.0),
        );
        assert_eq!(face_uv([p, m, n], p, m, n), [0, 0, 255, 0, 0, 255]);
        let mid = DVec3::new(5.0, 3.0, 5.0);
        assert_eq!(face_uv([mid, mid, mid], p, m, n)[..2], [128, 128]);
        // Degenerate texture triangle.
        assert_eq!(face_uv([p, m, n], p, p, p), [0; 6]);
    }

    #[test]
    fn converts_geometry_alpha_and_textures() {
        let model = Model {
            vertices: vec![[0, 0, 0], [10, -20, 0], [0, 0, 10]],
            faces: vec![[0, 1, 2], [0, 2, 1]],
            face_colours: vec![127, 3],
            face_types: Some(vec![0, 2]),
            priority: 0,
            face_priorities: None,
            face_alphas: Some(vec![0, 200]),
            face_labels: None,
            vertex_labels: Some(vec![1, 2, 3]),
            textured_faces: vec![[0, 1, 2]],
        };
        let palette = Palette::from_rgb(vec![[0, 0, 0], [255, 255, 255]]);
        let p = convert(&model, &palette).unwrap();
        assert_eq!(p.vertices[1], [10, 20, 0], "y is negated");
        assert_eq!(p.material_slots, [0, 4], "texture 3 is material 4");
        assert_eq!(p.palette_indices[0], 1, "light grey maps to white");
        assert_eq!(p.alphas, Some(vec![255, 55]));
        assert!(p.uvs.is_some());
        assert_eq!(p.labels, model.vertex_labels);
        p.encode().unwrap();
    }
}
