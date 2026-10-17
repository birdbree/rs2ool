//! Every media and title sprite entry decodes, the four fonts have 256
//! glyphs, interfaces decode fully with every layer child present, and a
//! few records chosen by their structure are spot-checked.

mod common;

use std::collections::BTreeMap;

use rs2cache::types::interface::Content;
use rs2cache::{archive, interface, sprite};

/// Whether an archive entry is a JPEG (the title screen background), the
/// one kind of entry besides `index.dat` that isn't a sprite block.
fn is_jpeg(data: &[u8]) -> bool {
    data.starts_with(&[0xff, 0xd8, 0xff])
}

#[test]
fn media_and_title_sprites() {
    let Some(cache) = common::cache() else { return };
    for n in [archive::MEDIA, archive::TITLE] {
        let a = cache.archive(n).unwrap();
        let decoded = sprite::decode_archive(&a).unwrap();
        let mut subs = 0;
        for (e, r) in &decoded {
            let s = r
                .as_ref()
                .unwrap_or_else(|err| panic!("{:?}/{}: {err}", e.name, e.hash));
            assert!(!s.sprites.is_empty());
            subs += s.sprites.len();
        }
        let unnamed = decoded.iter().filter(|(e, _)| e.name.is_none()).count();
        eprintln!(
            "archive {n}: {} sprite entries ({unnamed} unnamed), {subs} sub-sprites",
            decoded.len()
        );
        // Every entry but the index and any JPEG is a sprite entry.
        let jpegs = a.entries.iter().filter(|e| is_jpeg(&e.data)).count();
        assert_eq!(decoded.len(), a.entries.len() - 1 - jpegs);
    }
}

#[test]
fn opaque_single_sprite() {
    // The first media sheet holding one sub-sprite with no transparent
    // pixel: its size and pixel count agree, it sits inside the sheet's
    // canvas, and every pixel converts to full alpha.
    let Some(cache) = common::cache() else { return };
    let media = cache.archive(archive::MEDIA).unwrap();
    let decoded = sprite::decode_archive(&media).unwrap();
    let (e, sheet) = decoded
        .iter()
        .filter_map(|(e, r)| Some((e, r.as_ref().ok()?)))
        .find(|(_, s)| s.sprites.len() == 1 && s.sprites[0].pixels.iter().all(|&p| p != 0))
        .expect("no media sheet with one fully opaque sub-sprite");
    let s = &sheet.sprites[0];
    eprintln!(
        "{:?}/{}: {}x{} at {},{} on {}x{}",
        e.name,
        e.hash,
        s.width,
        s.height,
        s.x_offset,
        s.y_offset,
        sheet.max_width,
        sheet.max_height
    );
    assert!(s.width > 0 && s.height > 0);
    assert_eq!(s.pixels.len(), usize::from(s.width) * usize::from(s.height));
    let full = sheet.untrimmed(0).unwrap();
    assert!(full.width >= s.width && full.height >= s.height);
    let rgba = sheet.rgba(0).unwrap();
    assert_eq!(rgba.len(), s.pixels.len() * 4);
    assert!(
        rgba.chunks_exact(4).all(|px| px[3] == 255),
        "opaque pixels lost alpha"
    );
}

#[test]
fn fonts() {
    let Some(cache) = common::cache() else { return };
    let title = cache.archive(archive::TITLE).unwrap();
    for name in sprite::FONTS {
        let f = sprite::decode_font(&title, name).unwrap();
        assert_eq!(f.glyphs.sprites.len(), 256);
        assert_eq!(
            (
                f.advance.len(),
                f.draw_offset_x.len(),
                f.draw_offset_y.len()
            ),
            (256, 256, 256)
        );
        for g in &f.glyphs.sprites {
            assert_eq!(g.pixels.len(), usize::from(g.width) * usize::from(g.height));
        }
        // Space has no shape to measure: its glyph is the smallest in the
        // font, and its advance is borrowed.
        let area = |g: &rs2cache::types::sprite::Sprite| u32::from(g.width) * u32::from(g.height);
        let space = &f.glyphs.sprites[32];
        assert!(
            f.glyphs.sprites.iter().all(|g| area(g) >= area(space)),
            "{name}: a glyph is smaller than space"
        );
        let from = if name == "q8_full" { 73 } else { 105 };
        assert_eq!(f.advance[32], f.advance[from]);
        assert!(f.glyphs.sprites[usize::from(b'A')].width > 0);
    }
}

#[test]
fn interfaces() {
    let Some(cache) = common::cache() else { return };
    let a = cache.archive(archive::INTERFACE).unwrap();
    let (count, list) = interface::decode(a.get("data").unwrap()).unwrap();
    eprintln!("{} components, table size {count}", list.len());
    assert!(list.len() <= usize::from(count));
    let by_id: BTreeMap<u16, _> = list.iter().map(|i| (i.id, i)).collect();
    assert_eq!(by_id.len(), list.len(), "duplicate ids");
    for i in &list {
        if let Content::Layer { children, .. } = &i.content {
            for c in children {
                assert!(
                    by_id.contains_key(&c.id),
                    "{} has missing child {}",
                    i.id,
                    c.id
                );
            }
        }
    }
    // The first layer with an inventory child: the grid has slots, its
    // background sprite table is full, and it sits inside the layer.
    let (layer, child, inv) = list
        .iter()
        .find_map(|i| {
            let Content::Layer { children, .. } = &i.content else {
                return None;
            };
            children.iter().find_map(|c| {
                let inv = by_id[&c.id];
                matches!(inv.content, Content::Inventory { .. }).then_some((i, c, inv))
            })
        })
        .expect("no layer with an inventory child");
    eprintln!(
        "layer {} ({}x{}) holds inventory {} ({}x{} slots) at {},{}",
        layer.id, layer.width, layer.height, inv.id, inv.width, inv.height, child.x, child.y
    );
    let Content::Inventory { slot_sprites, .. } = &inv.content else {
        unreachable!()
    };
    assert_eq!(slot_sprites.len(), 20);
    assert!(
        inv.width > 0 && inv.height > 0,
        "inventory {} has no slots",
        inv.id
    );
    assert!(
        (0..i32::from(layer.width)).contains(&i32::from(child.x))
            && (0..i32::from(layer.height)).contains(&i32::from(child.y)),
        "inventory {} lies outside layer {}",
        inv.id,
        layer.id
    );
}
