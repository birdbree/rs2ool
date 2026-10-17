//! Palette sprites and fonts.
//!
//! Textures, media sprites, title sprites and fonts all share this layout:
//!
//! ```text
//! <name>.dat
//!   offset        u16   where this entry's block starts in index.dat
//!   pixels        u8[]  every sub-sprite's palette indices, in order
//!
//! index.dat, at offset
//!   maxWidth      u16   canvas the sub-sprite offsets are relative to
//!   maxHeight     u16
//!   paletteCount  u8
//!   rgb           u24 × (paletteCount − 1)   entry 0 (transparent) isn't stored
//!   per sub-sprite, 7 bytes:
//!     xOffset u8, yOffset u8, width u16, height u16,
//!     type u8     0 = row-major pixels, 1 = column-major
//! ```
//!
//! The cache doesn't store how many sub-sprites an entry has; the original
//! client just knows how many it wants. So the decoder works it out: an
//! entry's records run until the next entry's block in `index.dat` (or its
//! end), and the pixel bytes must be used up exactly.

use crate::types::sprite::{Font, Sprite, SpriteSheet};
use thiserror::Error;

use crate::archive::{Archive, Entry};
use crate::io::{Cursor, IoError};

// Bytes per sub-sprite record in index.dat.
const RECORD: usize = 7;

/// A sprite entry that wouldn't decode.
#[derive(Debug, Error)]
pub enum SpriteError {
    /// The header, a record or the pixels ran out.
    #[error("truncated: {0}")]
    Truncated(#[from] IoError),
    /// The archive has no `index.dat`. Also returned by [`decode_font`]
    /// when the font entry itself is missing.
    #[error("archive has no index.dat")]
    NoIndex,
    /// A sub-sprite's pixel order byte isn't 0 or 1.
    #[error("sub-sprite {sprite} has pixel type {kind}; expected 0 or 1")]
    BadType {
        /// Sub-sprite number.
        sprite: usize,
        /// The byte found.
        kind: u8,
    },
    /// Pixel bytes were left over after the last sub-sprite.
    #[error("sub-sprites use {used} pixel bytes but the entry holds {have}")]
    PixelCount {
        /// Bytes accounted for, offset included.
        used: usize,
        /// The entry's size.
        have: usize,
    },
}

// The block header at `at`: canvas size and palette. Returns a cursor left
// at the first sub-sprite record.
fn header(index: &[u8], at: usize) -> Result<(Cursor<'_>, u16, u16, Vec<u32>), SpriteError> {
    let mut c = Cursor::at(index, at)?;
    let max_width = c.u16()?;
    let max_height = c.u16()?;
    let n = c.u8()?;
    // Entry 0 is transparent and not stored; fill it in as 0.
    let mut palette = vec![0u32];
    for _ in 1..n {
        palette.push(c.u24()?);
    }
    Ok((c, max_width, max_height, palette))
}

// One sub-sprite: record from `c`, pixels from `px`. `n` is only for errors.
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
            // Stored column by column (`x * h + y`); transpose to row-major.
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

/// Decode an entry with exactly `count` sub-sprites, given the archive's
/// `index.dat`. Leftover pixel bytes aren't checked.
pub fn decode_count(data: &[u8], index: &[u8], count: usize) -> Result<SpriteSheet, SpriteError> {
    // The first two bytes point into index.dat; the pixels follow them.
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
/// are read while a whole one fits before `block_end` and its pixels fit in
/// what's left; afterwards the pixel bytes must be used up exactly. So a
/// zero-size sub-sprite at the very end of a block is never counted.
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
        // Peek at the record's width × height without consuming it.
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

/// The sprite entries of an archive, with the end of each one's block in
/// `index.dat`.
///
/// That's every entry but `index.dat` whose first `u16` leaves room for the
/// 5-byte block header inside `index.dat`. Entries that only pass this by
/// luck (such as a JPEG) show up here and fail later in
/// [`decode_block`]. Returned in block order.
pub fn sprite_entries(archive: &Archive) -> Result<Vec<(&Entry, usize)>, SpriteError> {
    let index = archive.get("index.dat").ok_or(SpriteError::NoIndex)?;
    let index_hash = crate::archive::jag_hash("index.dat");
    let mut starts: Vec<(usize, &Entry)> = archive
        .entries
        .iter()
        .filter(|e| e.hash != index_hash)
        .filter_map(|e| {
            let at = usize::from(Cursor::new(&e.data).u16().ok()?);
            // 5 = maxWidth + maxHeight + paletteCount.
            (at + 5 <= index.len()).then_some((at, e))
        })
        .collect();
    starts.sort_by_key(|&(at, _)| at);
    let mut out = Vec::with_capacity(starts.len());
    // Each block runs up to the next one's start, the last to the end.
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
/// 256 glyphs, with metrics derived from the pixels as the original client
/// does.
///
/// Advance starts at `width + 2` with an x offset of 1. If the leftmost
/// column is near-empty, the advance loses 1 and the offset becomes 0; if
/// the rightmost is, the advance loses another 1. Space (32) then borrows
/// the advance of `I` (73) for `q8_full` and of `i` (105) for the others.
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
        // Sum of palette indices down column `x`, skipping the top seventh.
        // It counts as empty when that's at most h/7. Yes, it adds up palette
        // values rather than counting inked pixels; the original client does
        // too.
        let column = |x: usize| {
            (h / 7..h)
                .map(|y| usize::from(g.pixels[x + y * w]))
                .sum::<usize>()
        };
        // Pen advance and x draw offset, trimmed below for empty edges.
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

/// Font names in the title archive, in font-id order (also the font index
/// in an interface text style).
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
