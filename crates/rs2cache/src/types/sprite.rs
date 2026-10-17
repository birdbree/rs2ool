//! Palette sprites (textures, media and title sprites) and fonts.

use serde::{Deserialize, Serialize};

/// One sub-sprite: palette indices in row-major order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sprite {
    pub x_offset: u8,
    pub y_offset: u8,
    pub width: u16,
    pub height: u16,
    /// Whether the pixels were stored column-major (type 1).
    pub column_major: bool,
    /// Palette index per pixel, row-major; index 0 is transparent.
    pub pixels: Vec<u8>,
}

/// Every sub-sprite stored under one archive entry, sharing a palette.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpriteSheet {
    /// The full (untrimmed) size the offsets are relative to.
    pub max_width: u16,
    pub max_height: u16,
    /// 24-bit RGB per palette index; entry 0 is the transparent colour and
    /// is always 0.
    pub palette: Vec<u32>,
    pub sprites: Vec<Sprite>,
}

impl SpriteSheet {
    /// Sub-sprite `i` placed at its offsets on a `max_width × max_height`
    /// canvas, as the client does for textures. Pixels falling outside the
    /// canvas are dropped.
    pub fn untrimmed(&self, i: usize) -> Option<Sprite> {
        let s = self.sprites.get(i)?;
        let (mw, mh) = (usize::from(self.max_width), usize::from(self.max_height));
        let mut pixels = vec![0u8; mw * mh];
        let (ox, oy) = (usize::from(s.x_offset), usize::from(s.y_offset));
        for y in 0..usize::from(s.height) {
            for x in 0..usize::from(s.width) {
                if x + ox < mw && y + oy < mh {
                    pixels[x + ox + (y + oy) * mw] = s.pixels[x + y * usize::from(s.width)];
                }
            }
        }
        Some(Sprite {
            x_offset: 0,
            y_offset: 0,
            width: self.max_width,
            height: self.max_height,
            column_major: false,
            pixels,
        })
    }

    /// RGBA8 pixels for sub-sprite `i`, row-major; palette index 0 is fully
    /// transparent and every other index opaque.
    pub fn rgba(&self, i: usize) -> Option<Vec<u8>> {
        let s = self.sprites.get(i)?;
        let mut out = Vec::with_capacity(s.pixels.len() * 4);
        for &p in &s.pixels {
            if p == 0 {
                out.extend([0, 0, 0, 0]);
            } else {
                let c = self.palette.get(usize::from(p)).copied().unwrap_or(0);
                out.extend([(c >> 16) as u8, (c >> 8) as u8, c as u8, 255]);
            }
        }
        Some(out)
    }
}

/// A bitmap font: 256 glyph sprites plus the metrics the client derives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Font {
    pub name: String,
    pub glyphs: SpriteSheet,
    /// Pen advance per character, in pixels.
    pub advance: Vec<u8>,
    /// Horizontal draw offset per glyph. The client recomputes it from the
    /// pixels (0 or 1) and ignores the stored x offset.
    pub draw_offset_x: Vec<i8>,
    /// Vertical draw offset per glyph: the stored y offset.
    pub draw_offset_y: Vec<i8>,
}
