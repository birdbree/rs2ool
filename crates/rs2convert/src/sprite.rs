//! Media and title sprites, and fonts, into a `sprites` pack.
//!
//! Record ids: sprite `s` (an `rs2cache` sprite id) sub-sprite `i` is
//! `s << 8 | i`; font `f` glyph `g` is `GLYPH_BASE | f << 8 | g` and its
//! metrics `METRICS_BASE | f`.

use contentpack::records::sprite::{FontMetrics, SpriteRecord};
use contentpack::{Kind, PackWriter};
use rs2cache::types::record::{Record, RecordType};
use rs2cache::types::sprite::SpriteSheet;
use rs2cache::{Records, archive};

use crate::ConvertError;

pub const GLYPH_BASE: u32 = 0x0100_0000;
pub const METRICS_BASE: u32 = 0x0200_0000;

fn records_of(sheet: &SpriteSheet, name_hash: u32) -> Vec<SpriteRecord> {
    sheet
        .sprites
        .iter()
        .enumerate()
        .map(|(i, s)| SpriteRecord {
            name_hash,
            sub_index: i as u8,
            width: s.width,
            height: s.height,
            x_offset: s.x_offset,
            y_offset: s.y_offset,
            rgba: sheet.rgba(i).unwrap_or_default(),
        })
        .collect()
}

/// Convert every sprite of the given archives (media and/or title) and,
/// when the title archive is included, every font.
pub fn pack(
    records: &Records,
    archives: &[u8],
    source_cache_crc: u32,
) -> Result<PackWriter, ConvertError> {
    let mut w = PackWriter::new(Kind::Sprites, source_cache_crc);
    for id in records.ids(RecordType::Sprite)? {
        let (n, entry) = records.sprite_source(id)?;
        if !archives.contains(&n) {
            continue;
        }
        let Record::Sprite(sheet) = records.decode(RecordType::Sprite, id)? else {
            unreachable!()
        };
        for r in records_of(&sheet, entry.hash() as u32) {
            w.insert(id << 8 | u32::from(r.sub_index), r.encode()?);
        }
    }
    if archives.contains(&archive::TITLE) {
        for f in records.ids(RecordType::Font)? {
            let Record::Font(font) = records.decode(RecordType::Font, f)? else {
                unreachable!()
            };
            let hash = archive::jag_hash(&format!("{}.dat", font.name)) as u32;
            for r in records_of(&font.glyphs, hash) {
                w.insert(GLYPH_BASE | f << 8 | u32::from(r.sub_index), r.encode()?);
            }
            let metrics = FontMetrics {
                advance: font
                    .advance
                    .clone()
                    .try_into()
                    .map_err(|_| ConvertError::Invalid("font advance count".into()))?,
                draw_offset_x: font
                    .draw_offset_x
                    .clone()
                    .try_into()
                    .map_err(|_| ConvertError::Invalid("font offsets".into()))?,
                draw_offset_y: font
                    .draw_offset_y
                    .clone()
                    .try_into()
                    .map_err(|_| ConvertError::Invalid("font offsets".into()))?,
            };
            w.insert(METRICS_BASE | f, metrics.encode());
        }
    }
    Ok(w)
}
