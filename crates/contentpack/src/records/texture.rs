//! `textures` records.
//!
//! ```text
//! texture rectangle (record id = texture id, 11 bytes)
//!   atlas u16, x u16, y u16, width u16, height u16, flags u8
//!
//! atlas image (record id = ATLAS_BASE | atlas)
//!   width u16, height u16
//!   width × height × {r, g, b, a u8}    row-major, row 0 first
//! ```
//!
//! A rectangle's `x`, `y`, `width` and `height` are in atlas pixels. Nothing
//! checks that the atlas exists or that the rectangle fits inside it.

use crate::bytes::{Reader, Writer};

use super::{RecordError, finish};

/// Atlas images use record ids `ATLAS_BASE | atlas`.
pub const ATLAS_BASE: u32 = 0x8000_0000;

/// Bits of [`TextureRect::flags`]. Bits 1–7 are unused.
pub mod flags {
    /// Sample with nearest filtering.
    pub const NEAREST: u8 = 1;
}

/// Where one texture sits in an atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureRect {
    /// Atlas number; the image is record `ATLAS_BASE | atlas`.
    pub atlas: u16,
    /// First column in the atlas, in pixels.
    pub x: u16,
    /// First row in the atlas, in pixels.
    pub y: u16,
    /// Width in pixels.
    pub width: u16,
    /// Height in pixels.
    pub height: u16,
    /// Sampling bits, see [`flags`].
    pub flags: u8,
}

impl TextureRect {
    /// Encode the record: always 11 bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.u16(self.atlas)
            .u16(self.x)
            .u16(self.y)
            .u16(self.width)
            .u16(self.height)
            .u8(self.flags);
        w.0
    }

    /// Decode a record. It has to be exactly 11 bytes.
    pub fn decode(data: &[u8]) -> Result<Self, RecordError> {
        let mut r = Reader::new(data);
        let t = Self {
            atlas: r.u16()?,
            x: r.u16()?,
            y: r.u16()?,
            width: r.u16()?,
            height: r.u16()?,
            flags: r.u8()?,
        };
        finish(&r)?;
        Ok(t)
    }
}

/// An atlas image: a plain RGBA8 bitmap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Atlas {
    /// Width in pixels.
    pub width: u16,
    /// Height in pixels.
    pub height: u16,
    /// `width × height × 4` bytes of RGBA, row-major.
    pub rgba: Vec<u8>,
}

impl Atlas {
    /// Encode the image. Fails if `rgba` isn't exactly `width × height × 4`
    /// bytes.
    pub fn encode(&self) -> Result<Vec<u8>, RecordError> {
        if self.rgba.len() != usize::from(self.width) * usize::from(self.height) * 4 {
            return Err(RecordError::Invalid("atlas pixel count".into()));
        }
        let mut w = Writer::default();
        w.u16(self.width).u16(self.height).bytes(&self.rgba);
        Ok(w.0)
    }

    /// Decode an image.
    pub fn decode(data: &[u8]) -> Result<Self, RecordError> {
        let mut r = Reader::new(data);
        let (width, height) = (r.u16()?, r.u16()?);
        let rgba = r
            .bytes(usize::from(width) * usize::from(height) * 4)?
            .to_vec();
        finish(&r)?;
        Ok(Self {
            width,
            height,
            rgba,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let t = TextureRect {
            atlas: 0,
            x: 128,
            y: 64,
            width: 64,
            height: 64,
            flags: flags::NEAREST,
        };
        assert_eq!(TextureRect::decode(&t.encode()).unwrap(), t);
        let a = Atlas {
            width: 2,
            height: 1,
            rgba: vec![1, 2, 3, 4, 5, 6, 7, 8],
        };
        assert_eq!(Atlas::decode(&a.encode().unwrap()).unwrap(), a);
    }
}
