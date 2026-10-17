//! Textures: the 50 texture sprites packed into one 1024² atlas with shelf
//! packing, plus a texture-mode material per texture.

use rs2cache::Records;
use rs2cache::types::record::{Record, RecordType};
use rs2pack::records::material::{Material, mode};
use rs2pack::records::texture::{ATLAS_BASE, Atlas, TextureRect, flags};
use rs2pack::{Kind, PackWriter};

use crate::{ConvertError, material_id};

pub const ATLAS_SIZE: u16 = 1024;

/// One texture's untrimmed RGBA pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureImage {
    pub id: u16,
    pub width: u16,
    pub height: u16,
    pub rgba: Vec<u8>,
}

/// Every texture, padded to its full size as the client does.
pub fn images(records: &Records) -> Result<Vec<TextureImage>, ConvertError> {
    let mut out = Vec::new();
    for id in records.ids(RecordType::Texture)? {
        let Record::Texture(sheet) = records.decode(RecordType::Texture, id)? else {
            unreachable!()
        };
        let full = sheet
            .untrimmed(0)
            .ok_or_else(|| ConvertError::Invalid(format!("texture {id} has no sprite")))?;
        let padded = rs2cache::types::sprite::SpriteSheet {
            sprites: vec![full],
            ..sheet
        };
        let s = &padded.sprites[0];
        out.push(TextureImage {
            id: id as u16,
            width: s.width,
            height: s.height,
            rgba: padded.rgba(0).unwrap_or_default(),
        });
    }
    Ok(out)
}

/// The packed atlas and where each texture went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureAtlas {
    pub atlas: Atlas,
    pub rects: Vec<(u16, TextureRect)>,
}

impl TextureAtlas {
    /// Shelf-pack textures (tallest first, then by id) into one atlas.
    pub fn build(images: &[TextureImage]) -> Result<Self, ConvertError> {
        let side = usize::from(ATLAS_SIZE);
        let mut order: Vec<&TextureImage> = images.iter().collect();
        order.sort_by_key(|t| (std::cmp::Reverse(t.height), t.id));
        let mut rgba = vec![0u8; side * side * 4];
        let mut rects = Vec::new();
        let (mut x, mut y, mut shelf) = (0usize, 0usize, 0usize);
        for t in order {
            let (w, h) = (usize::from(t.width), usize::from(t.height));
            if x + w > side {
                x = 0;
                y += shelf;
                shelf = 0;
            }
            if y + h > side || w > side {
                return Err(ConvertError::Invalid(format!(
                    "texture {} does not fit the atlas",
                    t.id
                )));
            }
            for row in 0..h {
                let src = &t.rgba[row * w * 4..(row + 1) * w * 4];
                let dst = ((y + row) * side + x) * 4;
                rgba[dst..dst + w * 4].copy_from_slice(src);
            }
            rects.push((
                t.id,
                TextureRect {
                    atlas: 0,
                    x: x as u16,
                    y: y as u16,
                    width: t.width,
                    height: t.height,
                    flags: flags::NEAREST,
                },
            ));
            x += w;
            shelf = shelf.max(h);
        }
        rects.sort_by_key(|r| r.0);
        Ok(Self {
            atlas: Atlas {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                rgba,
            },
            rects,
        })
    }

    /// Read one texture back out of the atlas.
    pub fn extract(atlas: &Atlas, r: &TextureRect) -> Vec<u8> {
        let side = usize::from(atlas.width);
        let (x, y, w, h) = (
            usize::from(r.x),
            usize::from(r.y),
            usize::from(r.width),
            usize::from(r.height),
        );
        let mut out = Vec::with_capacity(w * h * 4);
        for row in y..y + h {
            out.extend_from_slice(&atlas.rgba[(row * side + x) * 4..(row * side + x + w) * 4]);
        }
        out
    }

    /// The `textures` pack.
    pub fn pack(&self, source_cache_crc: u32) -> Result<PackWriter, ConvertError> {
        let mut w = PackWriter::new(Kind::Textures, source_cache_crc);
        w.insert(ATLAS_BASE, self.atlas.encode()?);
        for (id, r) in &self.rects {
            w.insert((*id).into(), r.encode());
        }
        Ok(w)
    }

    /// The texture-mode materials, one per texture, repeating every tile.
    pub fn materials(&self, source_cache_crc: u32) -> PackWriter {
        let mut w = PackWriter::new(Kind::Materials, source_cache_crc);
        for (id, _) in &self.rects {
            let m = texture_material(*id);
            w.insert(m.id.into(), m.encode());
        }
        w
    }
}

/// The material for texture `t`.
pub fn texture_material(t: u16) -> Material {
    Material {
        id: material_id::TEXTURE_BASE + t,
        mode: mode::TEXTURE,
        texture: t,
        period: 1,
        palette: 0,
        roughness: 255,
        flags: 0,
        alpha: 255,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(id: u16, side: u16) -> TextureImage {
        let n = usize::from(side) * usize::from(side);
        TextureImage {
            id,
            width: side,
            height: side,
            rgba: (0..n * 4)
                .map(|i| (i as u8).wrapping_add(id as u8))
                .collect(),
        }
    }

    #[test]
    fn shelf_packs_and_round_trips() {
        let mut imgs: Vec<_> = (0..47).map(|i| img(i, 128)).collect();
        imgs.extend((47..50).map(|i| img(i, 64)));
        let a = TextureAtlas::build(&imgs).unwrap();
        assert_eq!(a.rects.len(), 50);
        for (t, (id, r)) in imgs.iter().zip(&a.rects) {
            assert_eq!(t.id, *id);
            assert_eq!(TextureAtlas::extract(&a.atlas, r), t.rgba, "texture {id}");
        }
        assert!(TextureAtlas::build(&[img(0, 1024), img(1, 64)]).is_err());
    }
}
