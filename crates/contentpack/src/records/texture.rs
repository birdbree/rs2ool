//! `textures` records.
//!
//! A texture entry (record id = texture id) is
//! `{atlas u16, x u16, y u16, width u16, height u16, flags u8}`. An atlas
//! image is record `ATLAS_BASE | atlas`: `{width u16, height u16}` then
//! `width × height` RGBA8 pixels, row-major.

use crate::bytes::{Reader, Writer};

use super::{RecordError, finish};

/// Atlas images use record ids `ATLAS_BASE | atlas`.
pub const ATLAS_BASE: u32 = 0x8000_0000;

pub mod flags {
    /// Sample with nearest filtering.
    pub const NEAREST: u8 = 1;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureRect {
    pub atlas: u16,
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
    pub flags: u8,
}

impl TextureRect {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Atlas {
    pub width: u16,
    pub height: u16,
    pub rgba: Vec<u8>,
}

impl Atlas {
    pub fn encode(&self) -> Result<Vec<u8>, RecordError> {
        if self.rgba.len() != usize::from(self.width) * usize::from(self.height) * 4 {
            return Err(RecordError::Invalid("atlas pixel count".into()));
        }
        let mut w = Writer::default();
        w.u16(self.width).u16(self.height).bytes(&self.rgba);
        Ok(w.0)
    }

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
