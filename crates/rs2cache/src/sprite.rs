//! Palette sprites and fonts.
//!
//! A sprite entry `<name>.dat` starts with a `u16` offset into the archive's
//! `index.dat`, followed by the pixel bytes of every sub-sprite in order. At
//! that offset `index.dat` holds `maxWidth u16, maxHeight u16,
//! paletteCount u8, rgb u24 × (paletteCount − 1)` (entry 0 is transparent),
//! then per sub-sprite `{xOffset u8, yOffset u8, width u16, height u16,
//! type u8}`; type 0 pixels are row-major, type 1 column-major.
//!
//! The cache does not store how many sub-sprites an entry has. Each entry's
//! records run until the next entry's block in `index.dat` (or its end), and
//! the decoder checks that the pixel bytes are used exactly.

use rs2intermediate::sprite::{Font, Sprite, SpriteSheet};
use thiserror::Error;

use crate::archive::{Archive, Entry};
use crate::io::{Cursor, IoError};

const RECORD: usize = 7;

#[derive(Debug, Error)]
pub enum SpriteError {
    #[error("truncated: {0}")]
    Truncated(#[from] IoError),
    #[error("archive has no index.dat")]
    NoIndex,
    #[error("sub-sprite {sprite} has pixel type {kind}; expected 0 or 1")]
    BadType { sprite: usize, kind: u8 },
    #[error("sub-sprites use {used} pixel bytes but the entry holds {have}")]
    PixelCount { used: usize, have: usize },
}

fn header(index: &[u8], at: usize) -> Result<(Cursor<'_>, u16, u16, Vec<u32>), SpriteError> {
    let mut c = Cursor::at(index, at)?;
    let max_width = c.u16()?;
    let max_height = c.u16()?;
    let n = c.u8()?;
    let mut palette = vec![0u32];
    for _ in 1..n {
        palette.push(c.u24()?);
    }
    Ok((c, max_width, max_height, palette))
}

fn sprite(c: &mut Cursor, px: &mut Cursor, n: usize) -> Result<Sprite, SpriteError> {
    let x_offset = c.u8()?;
    let y_offset = c.u8()?;
    let width = c.u16()?;
    let height = c.u16()?;
    let kind = c.u8()?;
    let (w, h) = (usize::from(width), usize::from(height));
    let stored = px.bytes(w * h)?;
    let pixels = match kind {
        0 => stored.to_vec(),
        1 => {
            let mut out = vec![0; w * h];
            for x in 0..w {
                for y in 0..h {
                    out[x + y * w] = stored[x * h + y];
                }
            }
            out
        }
        kind => return Err(SpriteError::BadType { sprite: n, kind }),
    };
    Ok(Sprite {
        x_offset,
        y_offset,
        width,
        height,
        column_major: kind == 1,
        pixels,
    })
}

/// Decode an entry with exactly `count` sub-sprites.
pub fn decode_count(data: &[u8], index: &[u8], count: usize) -> Result<SpriteSheet, SpriteError> {
    let mut px = Cursor::new(data);
    let at = px.u16()?;
    let (mut c, max_width, max_height, palette) = header(index, at.into())?;
    let sprites = (0..count)
        .map(|n| sprite(&mut c, &mut px, n))
        .collect::<Result<_, _>>()?;
    Ok(SpriteSheet {
        max_width,
        max_height,
        palette,
        sprites,
    })
}

/// Decode an entry whose records end at `block_end` in `index.dat`. Records
/// are read until the pixel bytes run out, which must happen exactly.
pub fn decode_block(
    data: &[u8],
    index: &[u8],
    block_end: usize,
) -> Result<SpriteSheet, SpriteError> {
    let mut px = Cursor::new(data);
    let at = px.u16()?;
    let (mut c, max_width, max_height, palette) = header(index, at.into())?;
    let mut sprites = Vec::new();
    while c.pos() + RECORD <= block_end && px.remaining() > 0 {
        let mut peek = c.clone();
        peek.skip(2)?;
        let need = usize::from(peek.u16()?) * usize::from(peek.u16()?);
        if need > px.remaining() {
            break;
        }
        sprites.push(sprite(&mut c, &mut px, sprites.len())?);
    }
    if px.remaining() != 0 {
        return Err(SpriteError::PixelCount {
            used: data.len() - px.remaining(),
            have: data.len(),
        });
    }
    Ok(SpriteSheet {
        max_width,
        max_height,
        palette,
        sprites,
    })
}

/// The sprite entries of an archive (every entry but `index.dat` whose
/// header offset falls inside `index.dat`), with the end of each one's block.
pub fn sprite_entries(archive: &Archive) -> Result<Vec<(&Entry, usize)>, SpriteError> {
    let index = archive.get("index.dat").ok_or(SpriteError::NoIndex)?;
    let index_hash = crate::archive::jag_hash("index.dat");
    let mut starts: Vec<(usize, &Entry)> = archive
        .entries
        .iter()
        .filter(|e| e.hash != index_hash)
        .filter_map(|e| {
            let at = usize::from(Cursor::new(&e.data).u16().ok()?);
            (at + 5 <= index.len()).then_some((at, e))
        })
        .collect();
    starts.sort_by_key(|&(at, _)| at);
    let mut out = Vec::with_capacity(starts.len());
    for (i, &(_, e)) in starts.iter().enumerate() {
        let end = starts.get(i + 1).map_or(index.len(), |&(next, _)| next);
        out.push((e, end));
    }
    Ok(out)
}

/// An archive entry and the result of decoding it as sprites.
pub type DecodedEntry<'a> = (&'a Entry, Result<SpriteSheet, SpriteError>);

/// Decode every sprite entry of an archive, by entry. Entries that do not
/// decode as sprites (such as the title screen JPEG) are returned as errors.
pub fn decode_archive(archive: &Archive) -> Result<Vec<DecodedEntry<'_>>, SpriteError> {
    let index = archive.get("index.dat").ok_or(SpriteError::NoIndex)?;
    Ok(sprite_entries(archive)?
        .into_iter()
        .map(|(e, end)| (e, decode_block(&e.data, index, end)))
        .collect())
}

/// Decode font `name` (for example `"p12_full"`) from the title archive:
/// 256 glyphs, with metrics derived as the client does.
pub fn decode_font(archive: &Archive, name: &str) -> Result<Font, SpriteError> {
    let index = archive.get("index.dat").ok_or(SpriteError::NoIndex)?;
    let data = archive
        .get(&format!("{name}.dat"))
        .ok_or(SpriteError::NoIndex)?;
    let glyphs = decode_count(data, index, 256)?;
    let mut advance = Vec::with_capacity(256);
    let mut draw_offset_x = Vec::with_capacity(256);
    for g in &glyphs.sprites {
        let (w, h) = (usize::from(g.width), usize::from(g.height));
        // A column counts as empty when its lower 6/7 holds at most h/7 ink.
        let column = |x: usize| {
            (h / 7..h)
                .map(|y| usize::from(g.pixels[x + y * w]))
                .sum::<usize>()
        };
        let mut adv = w as i32 + 2;
        let mut dx = 1;
        if w > 0 && column(0) <= h / 7 {
            adv -= 1;
            dx = 0;
        }
        if w > 0 && column(w - 1) <= h / 7 {
            adv -= 1;
        }
        advance.push(adv.clamp(0, 255) as u8);
        draw_offset_x.push(dx);
    }
    // The space advance is borrowed from 'I' for q8 and 'i' otherwise.
    advance[32] = advance[if name == "q8_full" { 73 } else { 105 }];
    let draw_offset_y = glyphs.sprites.iter().map(|g| g.y_offset as i8).collect();
    Ok(Font {
        name: name.to_owned(),
        glyphs,
        advance,
        draw_offset_x,
        draw_offset_y,
    })
}

/// Font names in the title archive.
pub const FONTS: [&str; 4] = ["p11_full", "p12_full", "b12_full", "q8_full"];

#[cfg(test)]
mod tests {
    use super::*;

    /// `(x, y, width, height, type, stored pixels)`.
    type Rec = (u8, u8, u16, u16, u8, Vec<u8>);

    fn sheet_bytes(sprites: &[Rec]) -> (Vec<u8>, Vec<u8>) {
        let mut index = vec![0, 4, 0, 3, 3];
        index.extend([0xff, 0, 0, 0, 0x80, 0]);
        let mut data = vec![0, 0];
        for (x, y, w, h, kind, px) in sprites {
            index.extend([*x, *y]);
            index.extend(w.to_be_bytes());
            index.extend(h.to_be_bytes());
            index.push(*kind);
            data.extend(px);
        }
        (data, index)
    }

    #[test]
    fn row_and_column_major() {
        let (data, index) = sheet_bytes(&[
            (1, 2, 3, 2, 0, vec![0, 1, 2, 2, 1, 0]),
            // Column-major 3×2: columns (0,1), (2,2), (1,0).
            (0, 0, 3, 2, 1, vec![0, 1, 2, 2, 1, 0]),
        ]);
        let s = decode_block(&data, &index, index.len()).unwrap();
        assert_eq!((s.max_width, s.max_height), (4, 3));
        assert_eq!(s.palette, [0, 0xff0000, 0x8000]);
        assert_eq!(s.sprites.len(), 2);
        assert_eq!(s.sprites[0].pixels, [0, 1, 2, 2, 1, 0]);
        assert_eq!((s.sprites[0].x_offset, s.sprites[0].y_offset), (1, 2));
        assert_eq!(s.sprites[1].pixels, [0, 2, 1, 1, 2, 0]);
        assert!(s.sprites[1].column_major);
        let rgba = s.rgba(0).unwrap();
        assert_eq!(&rgba[..8], &[0, 0, 0, 0, 255, 0, 0, 255]);
    }

    #[test]
    fn pixel_mismatch_and_bad_type_are_errors() {
        let (mut data, index) = sheet_bytes(&[(0, 0, 2, 2, 0, vec![1, 1, 1, 1])]);
        data.push(9);
        assert!(matches!(
            decode_block(&data, &index, index.len()),
            Err(SpriteError::PixelCount { .. })
        ));
        let (data, index) = sheet_bytes(&[(0, 0, 1, 1, 7, vec![1])]);
        assert!(matches!(
            decode_block(&data, &index, index.len()),
            Err(SpriteError::BadType { kind: 7, .. })
        ));
        assert!(decode_count(&[0, 0], &index, 2).is_err());
    }
}
