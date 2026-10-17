//! `sprites` records.
//!
//! ```text
//! sprite
//!   nameHash u32, subIndex u8, width u16, height u16, xOffset u8, yOffset u8
//!   width × height × {r, g, b, a u8}    row-major, row 0 first
//!
//! font metrics (768 bytes, indexed by character code)
//!   advance     256 × u8
//!   drawOffsetX 256 × i8
//!   drawOffsetY 256 × i8
//! ```
//!
//! `nameHash` is the jag hash of the archive entry the sprite came from. A
//! sprite record is one sub-sprite of a sheet or one glyph of a font; each
//! font also gets one metrics record.

use crate::bytes::{Reader, Writer};

use super::{RecordError, finish};

/// A decoded sprite record: one image with its draw offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteRecord {
    /// Jag hash of the archive entry the sprite came from (for a glyph, the
    /// font's entry).
    pub name_hash: u32,
    /// Position in its sheet. Matches the low byte of the record id.
    pub sub_index: u8,
    /// Width in pixels.
    pub width: u16,
    /// Height in pixels.
    pub height: u16,
    /// Horizontal draw offset in pixels, as stored in the cache sheet.
    pub x_offset: u8,
    /// Vertical draw offset in pixels, as stored in the cache sheet.
    pub y_offset: u8,
    /// `width × height × 4` bytes of RGBA, row-major. Pixels with cache
    /// palette index 0 have alpha 0.
    pub rgba: Vec<u8>,
}

impl SpriteRecord {
    /// Encode the record. Fails if `rgba` isn't exactly
    /// `width × height × 4` bytes.
    pub fn encode(&self) -> Result<Vec<u8>, RecordError> {
        if self.rgba.len() != usize::from(self.width) * usize::from(self.height) * 4 {
            return Err(RecordError::Invalid("sprite pixel count".into()));
        }
        let mut w = Writer::default();
        w.u32(self.name_hash)
            .u8(self.sub_index)
            .u16(self.width)
            .u16(self.height)
            .u8(self.x_offset)
            .u8(self.y_offset)
            .bytes(&self.rgba);
        Ok(w.0)
    }

    /// Decode a record.
    pub fn decode(data: &[u8]) -> Result<Self, RecordError> {
        let mut r = Reader::new(data);
        let name_hash = r.u32()?;
        let sub_index = r.u8()?;
        let (width, height) = (r.u16()?, r.u16()?);
        let (x_offset, y_offset) = (r.u8()?, r.u8()?);
        let rgba = r
            .bytes(usize::from(width) * usize::from(height) * 4)?
            .to_vec();
        finish(&r)?;
        Ok(Self {
            name_hash,
            sub_index,
            width,
            height,
            x_offset,
            y_offset,
            rgba,
        })
    }
}

/// A font's metrics record: three tables indexed by character code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontMetrics {
    /// Pen advance per character, in pixels.
    pub advance: [u8; 256],
    /// Horizontal draw offset per glyph: the 0-or-1 offset the original
    /// client works out from the glyph's pixels, not the stored x offset.
    /// For a glyph it can disagree with [`SpriteRecord::x_offset`]; this is
    /// the one that places it correctly.
    pub draw_offset_x: [i8; 256],
    /// Vertical draw offset per glyph, as stored in the font.
    pub draw_offset_y: [i8; 256],
}

impl FontMetrics {
    /// Encode the record: always 768 bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.bytes(&self.advance);
        for &x in &self.draw_offset_x {
            w.i8(x);
        }
        for &y in &self.draw_offset_y {
            w.i8(y);
        }
        w.0
    }

    /// Decode a record. It has to be exactly 768 bytes.
    pub fn decode(data: &[u8]) -> Result<Self, RecordError> {
        let mut r = Reader::new(data);
        let advance = r.bytes(256)?.try_into().expect("256");
        let mut draw_offset_x = [0i8; 256];
        for x in &mut draw_offset_x {
            *x = r.i8()?;
        }
        let mut draw_offset_y = [0i8; 256];
        for y in &mut draw_offset_y {
            *y = r.i8()?;
        }
        finish(&r)?;
        Ok(Self {
            advance,
            draw_offset_x,
            draw_offset_y,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let s = SpriteRecord {
            name_hash: 9,
            sub_index: 2,
            width: 2,
            height: 1,
            x_offset: 3,
            y_offset: 4,
            rgba: vec![1; 8],
        };
        assert_eq!(SpriteRecord::decode(&s.encode().unwrap()).unwrap(), s);
        let mut m = FontMetrics {
            advance: [3; 256],
            draw_offset_x: [0; 256],
            draw_offset_y: [-1; 256],
        };
        m.advance[65] = 7;
        assert_eq!(FontMetrics::decode(&m.encode()).unwrap(), m);
    }
}
