//! Texture pages as the client prepares them: 128 × 128 texels (64-pixel
//! textures doubled), colours through the brightness curve, masked to
//! `0xf8f8ff`, with three progressively darker copies for shading.

use contentpack::palette::gamma;
use rs2cache::types::sprite::SpriteSheet;

/// One prepared texture: 4 × 16384 texels and whether it has no
/// transparent (zero) texels.
#[derive(Debug, Clone)]
pub struct TexturePage {
    pub texels: Vec<u32>,
    pub opaque: bool,
}

impl TexturePage {
    pub fn from_sheet(sheet: &SpriteSheet, brightness: f64) -> Option<Self> {
        let s = sheet.untrimmed(0)?;
        let palette: Vec<u32> = sheet
            .palette
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                let g = gamma(c, brightness);
                if g & 0xf8f8ff == 0 && i != 0 { 1 } else { g }
            })
            .collect();
        let at = |i: usize| {
            palette
                .get(usize::from(s.pixels.get(i).copied().unwrap_or(0)))
                .copied()
                .unwrap_or(0)
        };
        let mut texels = vec![0u32; 65536];
        for y in 0..128 {
            for x in 0..128 {
                texels[x + (y << 7)] = if s.width == 64 {
                    at((x >> 1) + ((y >> 1) << 6))
                } else {
                    at(x + (y << 7))
                };
            }
        }
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
    pub pages: Vec<Option<TexturePage>>,
}

impl Textures {
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

    pub fn page(&self, id: u16) -> Option<&TexturePage> {
        self.pages.get(usize::from(id)).and_then(Option::as_ref)
    }
}
