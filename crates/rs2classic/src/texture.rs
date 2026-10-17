//! Texture pages as the original client prepares them: 128 × 128 texels
//! (64-pixel textures doubled), colours through the brightness curve,
//! masked to `0xf8f8ff`, with three progressively darker copies for
//! shading.
//!
//! ```text
//! texels[0     .. 16384]  t                    (full)
//! texels[16384 .. 32768]  t − t>>3             (7/8)
//! texels[32768 .. 49152]  t − t>>2             (3/4)
//! texels[49152 .. 65536]  t − t>>2 − t>>3      (5/8)
//! each copy: index = x + y·128, masked to 0xf8f8ff
//! ```
//!
//! The mask clears the low three bits of red and green, so the textured
//! filler can shift a whole texel right by up to three bits without one
//! channel spilling into the next.

use contentpack::palette::gamma;
use rs2cache::types::sprite::SpriteSheet;

/// The single colour the original client uses for a texture where it can't
/// draw the texture itself, such as a textured floor on the minimap: the
/// mean of the palette after the brightness curve (entry 0, the transparent
/// one, included), then a further brightness curve of 1.4 on the mean. 0 is
/// reserved for transparency, so a black result comes out as 1.
pub fn average_colour(sheet: &SpriteSheet, brightness: f64) -> u32 {
    let n = sheet.palette.len().max(1) as u32;
    let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
    for (i, &c) in sheet.palette.iter().enumerate() {
        // The same per-entry rule as the texture pages: anything but entry
        // 0 that would mask to transparent becomes 1.
        let c = match gamma(c, brightness) {
            g if g & 0xf8f8ff == 0 && i != 0 => 1,
            g => g,
        };
        r += (c >> 16) & 0xff;
        g += (c >> 8) & 0xff;
        b += c & 0xff;
    }
    match gamma(((r / n) << 16) | ((g / n) << 8) | (b / n), 1.4) {
        0 => 1,
        c => c,
    }
}

/// One prepared texture: 4 × 16384 texels and whether it has no
/// transparent (zero) texels.
#[derive(Debug, Clone)]
pub struct TexturePage {
    /// The four copies back to back, `0xRRGGBB`. 0 means transparent.
    pub texels: Vec<u32>,
    /// True when no texel is 0, so the filler can skip the transparency
    /// test.
    pub opaque: bool,
}

impl TexturePage {
    /// Prepare a page from a texture's sprite sheet (its untrimmed first
    /// sprite) at `brightness`. `None` if the sheet has no sprite.
    pub fn from_sheet(sheet: &SpriteSheet, brightness: f64) -> Option<Self> {
        let s = sheet.untrimmed(0)?;
        let palette: Vec<u32> = sheet
            .palette
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                // Only palette index 0 may end up transparent: any other
                // colour that would mask to 0 becomes 1 instead.
                let g = gamma(c, brightness);
                if g & 0xf8f8ff == 0 && i != 0 { 1 } else { g }
            })
            .collect();
        // Colour of sprite pixel i (0 past the end or for a bad index).
        let at = |i: usize| {
            palette
                .get(usize::from(s.pixels.get(i).copied().unwrap_or(0)))
                .copied()
                .unwrap_or(0)
        };
        let mut texels = vec![0u32; 65536];
        // Fill copy 0. `y << 7` is y·128; a 64-wide texture is read at
        // half resolution, doubling each pixel both ways.
        for y in 0..128 {
            for x in 0..128 {
                texels[x + (y << 7)] = if s.width == 64 {
                    at((x >> 1) + ((y >> 1) << 6))
                } else {
                    at(x + (y << 7))
                };
            }
        }
        // Mask copy 0 and derive the three darker copies from it.
        let mut opaque = true;
        for i in 0..16384 {
            let t = texels[i] & 0xf8f8ff;
            texels[i] = t;
            if t == 0 {
                opaque = false;
            }
            texels[16384 + i] = t.wrapping_sub(t >> 3) & 0xf8f8ff;
            texels[32768 + i] = t.wrapping_sub(t >> 2) & 0xf8f8ff;
            texels[49152 + i] = t.wrapping_sub(t >> 2).wrapping_sub(t >> 3) & 0xf8f8ff;
        }
        Some(Self { texels, opaque })
    }
}

/// Prepared pages for texture ids 0–49.
#[derive(Debug, Clone, Default)]
pub struct Textures {
    /// Page per texture id; `None` where the texture is missing or empty.
    pub pages: Vec<Option<TexturePage>>,
}

impl Textures {
    /// Prepare every sheet at `brightness`. `sheets[i]` is texture `i`.
    pub fn from_sheets(sheets: &[Option<SpriteSheet>], brightness: f64) -> Self {
        Self {
            pages: sheets
                .iter()
                .map(|s| {
                    s.as_ref()
                        .and_then(|s| TexturePage::from_sheet(s, brightness))
                })
                .collect(),
        }
    }

    /// The page for texture `id`, if it's loaded.
    pub fn page(&self, id: u16) -> Option<&TexturePage> {
        self.pages.get(usize::from(id)).and_then(Option::as_ref)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(palette: Vec<u32>) -> SpriteSheet {
        SpriteSheet {
            max_width: 0,
            max_height: 0,
            palette,
            sprites: Vec::new(),
        }
    }

    #[test]
    fn average_colour_is_the_palette_mean_through_two_curves() {
        // At brightness 1.0 the first curve leaves the palette alone, so the
        // mean of [0, red, blue] is (85, 0, 85) before the 1.4 curve.
        let s = sheet(vec![0, 0xff0000, 0x0000ff]);
        assert_eq!(average_colour(&s, 1.0), gamma((85 << 16) | 85, 1.4));
    }

    #[test]
    fn a_black_average_is_never_transparent() {
        assert_eq!(average_colour(&sheet(vec![0, 0]), 0.8), 1);
    }
}
