//! `materials` records.
//!
//! A material is 10 bytes: `{id u16, mode u8, texId u16, period u8,
//! palette u8, roughness u8, flags u8, alpha u8}`. The palette itself is
//! stored in the same pack as record [`PALETTE_ID`]: `count u16` then
//! `count × {r, g, b u8}`.

use crate::bytes::{Reader, Writer};

use super::{RecordError, finish};

/// Record id of the palette in a `materials` pack.
pub const PALETTE_ID: u32 = 0xFFFF_FFFF;

pub mod mode {
    pub const PALETTE: u8 = 0;
    pub const TEXTURE: u8 = 1;
}

pub mod flags {
    pub const FLOOR: u8 = 1;
    pub const HIDE_UNDERLAY: u8 = 2;
    pub const OCCLUDE: u8 = 4;
    pub const SPECULAR: u8 = 8;
    pub const EMISSIVE: u8 = 16;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Material {
    pub id: u16,
    pub mode: u8,
    pub texture: u16,
    /// Texture repeat period in tiles.
    pub period: u8,
    pub palette: u8,
    pub roughness: u8,
    pub flags: u8,
    pub alpha: u8,
}

impl Material {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.u16(self.id)
            .u8(self.mode)
            .u16(self.texture)
            .u8(self.period)
            .u8(self.palette)
            .u8(self.roughness)
            .u8(self.flags)
            .u8(self.alpha);
        w.0
    }

    pub fn decode(data: &[u8]) -> Result<Self, RecordError> {
        let mut r = Reader::new(data);
        let m = Self {
            id: r.u16()?,
            mode: r.u8()?,
            texture: r.u16()?,
            period: r.u8()?,
            palette: r.u8()?,
            roughness: r.u8()?,
            flags: r.u8()?,
            alpha: r.u8()?,
        };
        finish(&r)?;
        Ok(m)
    }
}

/// The palette record: up to 65535 RGB entries.
pub fn encode_palette(rgb: &[[u8; 3]]) -> Vec<u8> {
    let mut w = Writer::default();
    w.u16(rgb.len() as u16);
    for c in rgb {
        w.bytes(c);
    }
    w.0
}

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
    fn ten_bytes_and_round_trip() {
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
        assert_eq!(b.len(), 10);
        assert_eq!(Material::decode(&b).unwrap(), m);
        let p = vec![[1, 2, 3], [250, 251, 252]];
        assert_eq!(decode_palette(&encode_palette(&p)).unwrap(), p);
    }
}
