//! `materials` records.
//!
//! ```text
//! material (11 bytes)
//!   id u16, mode u8, texId u16, period u8, palette u16,
//!   roughness u8, flags u8, alpha u8
//!
//! palette (record PALETTE_ID)
//!   count u16
//!   count × {r, g, b u8}
//! ```
//!
//! No magic, no version. The palette shares the pack with the materials at
//! the one id a `u16` material id can never reach. A palette index is a
//! position in its entries, from 0; model faces, palette-mode materials and
//! object surfaces all carry one.

use crate::bytes::{Reader, Writer};

use super::{RecordError, finish};

/// Record id of the palette in a `materials` pack.
pub const PALETTE_ID: u32 = 0xFFFF_FFFF;

/// Values of [`Material::mode`].
pub mod mode {
    /// Flat colour: [`palette`](super::Material::palette) picks it.
    pub const PALETTE: u8 = 0;
    /// Textured: [`texture`](super::Material::texture) names a texture rectangle.
    pub const TEXTURE: u8 = 1;
}

/// Bits of [`Material::flags`]. Bits 5–7 are unused.
pub mod flags {
    /// A floor material.
    pub const FLOOR: u8 = 1;
    /// Copied from the floor definition's hide-underlay flag.
    pub const HIDE_UNDERLAY: u8 = 2;
    /// Copied from the floor definition's occlude flag.
    pub const OCCLUDE: u8 = 4;
    /// Specular. Defined, but the tools never set it.
    pub const SPECULAR: u8 = 8;
    /// Emissive. Defined, but the tools never set it.
    pub const EMISSIVE: u8 = 16;
}

/// A decoded material record.
///
/// The decoder only checks the length: any mode or flags go, and `id`
/// isn't compared with the record id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Material {
    /// Material id. The tools write the record id here too.
    pub id: u16,
    /// How the surface is coloured, see [`mode`].
    pub mode: u8,
    /// Texture id, for texture mode; 0 otherwise.
    pub texture: u16,
    /// Texture repeat period in tiles.
    pub period: u8,
    /// Palette index, for palette mode.
    pub palette: u16,
    /// Roughness, 0–255. The tools always write 255.
    pub roughness: u8,
    /// Bit set, see [`flags`].
    pub flags: u8,
    /// Opacity; 255 is opaque.
    pub alpha: u8,
}

impl Material {
    /// Encode the record: always 11 bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.u16(self.id)
            .u8(self.mode)
            .u16(self.texture)
            .u8(self.period)
            .u16(self.palette)
            .u8(self.roughness)
            .u8(self.flags)
            .u8(self.alpha);
        w.0
    }

    /// Decode a record. It has to be exactly 11 bytes.
    pub fn decode(data: &[u8]) -> Result<Self, RecordError> {
        let mut r = Reader::new(data);
        let m = Self {
            id: r.u16()?,
            mode: r.u8()?,
            texture: r.u16()?,
            period: r.u8()?,
            palette: r.u16()?,
            roughness: r.u8()?,
            flags: r.u8()?,
            alpha: r.u8()?,
        };
        finish(&r)?;
        Ok(m)
    }
}

/// Encode the palette record: up to 65535 RGB entries.
///
/// The count is the low 16 bits of `rgb.len()`, unchecked, so don't pass
/// more than 65535.
pub fn encode_palette(rgb: &[[u8; 3]]) -> Vec<u8> {
    let mut w = Writer::default();
    w.u16(rgb.len() as u16);
    for c in rgb {
        w.bytes(c);
    }
    w.0
}

/// Decode the palette record into its `[r, g, b]` entries.
pub fn decode_palette(data: &[u8]) -> Result<Vec<[u8; 3]>, RecordError> {
    let mut r = Reader::new(data);
    let n = r.u16()?;
    let out = (0..n)
        .map(|_| Ok([r.u8()?, r.u8()?, r.u8()?]))
        .collect::<Result<_, RecordError>>()?;
    finish(&r)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eleven_bytes_and_round_trip() {
        let m = Material {
            id: 300,
            mode: mode::TEXTURE,
            texture: 7,
            period: 1,
            palette: 9,
            roughness: 255,
            flags: flags::FLOOR | flags::OCCLUDE,
            alpha: 128,
        };
        let b = m.encode();
        assert_eq!(b.len(), 11);
        assert_eq!(Material::decode(&b).unwrap(), m);
        let p = vec![[1, 2, 3], [250, 251, 252]];
        assert_eq!(decode_palette(&encode_palette(&p)).unwrap(), p);
    }
}
