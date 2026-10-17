//! `sprites` records.
//!
//! A sprite is `{nameHash u32, subIndex u8, width u16, height u16,
//! xOffset u8, yOffset u8}` followed by `width × height` RGBA8 pixels.
//! `nameHash` is the jag hash of the archive entry the sprite came from. A
//! font adds a metrics record: `advance u8 × 256, drawOffsetX i8 × 256,
//! drawOffsetY i8 × 256`.

use crate::bytes::{Reader, Writer};

use super::{RecordError, finish};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteRecord {
    pub name_hash: u32,
    pub sub_index: u8,
    pub width: u16,
    pub height: u16,
    pub x_offset: u8,
    pub y_offset: u8,
    pub rgba: Vec<u8>,
}

impl SpriteRecord {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontMetrics {
    pub advance: [u8; 256],
    pub draw_offset_x: [i8; 256],
    pub draw_offset_y: [i8; 256],
}

impl FontMetrics {
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
