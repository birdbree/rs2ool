//! Phase A7 check: the 50 textures decode as single sprites that pad to
//! 64×64 or 128×128, with visible content.

mod common;

use rs2cache::{archive, sprite};

#[test]
fn fifty_textures() {
    let Some(cache) = common::cache() else { return };
    let a = cache.archive(archive::TEXTURES).unwrap();
    let index = a.get("index.dat").unwrap();
    let blocks = sprite::sprite_entries(&a).unwrap();
    assert_eq!(blocks.len(), 50);
    let mut sizes = std::collections::BTreeMap::new();
    for id in 0..50 {
        let name = format!("{id}.dat");
        let (entry, end) = blocks
            .iter()
            .find(|(e, _)| e.name == Some(name.as_str()))
            .unwrap();
        let sheet = sprite::decode_block(&entry.data, index, *end)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(sheet.sprites.len(), 1, "{name}");
        // Textures may be stored trimmed; the client pads them back.
        let s = &sheet.untrimmed(0).unwrap();
        assert!(
            matches!((s.width, s.height), (64, 64) | (128, 128)),
            "{name}: {}x{}",
            s.width,
            s.height
        );
        let distinct: std::collections::BTreeSet<_> = s.pixels.iter().collect();
        if distinct.len() == 1 {
            eprintln!(
                "{name}: one colour, index {:?}, palette {:x?}, {}x{} at {},{}",
                distinct,
                sheet.palette,
                sheet.sprites[0].width,
                sheet.sprites[0].height,
                sheet.sprites[0].x_offset,
                sheet.sprites[0].y_offset
            );
        }
        // Visible content: some pixel is opaque.
        assert!(
            s.pixels.iter().any(|&p| p != 0),
            "{name} is fully transparent"
        );
        *sizes.entry((s.width, s.height)).or_insert(0) += 1;
        assert_eq!(
            sheet.rgba(0).unwrap().len(),
            sheet.sprites[0].pixels.len() * 4
        );
    }
    eprintln!("texture sizes {sizes:?}");
}
