//! Phase A9 check: every media and title sprite entry decodes; the compass
//! is a single opaque 51×51 sprite; the four fonts have 256 glyphs; interfaces
//! decode fully, every layer child exists, and the inventory tab is a layer
//! holding a 4×7 inventory.

mod common;

use std::collections::BTreeMap;

use rs2cache::{archive, interface, sprite};
use rs2intermediate::interface::Content;

#[test]
fn media_and_title_sprites() {
    let Some(cache) = common::cache() else { return };
    for (n, expect) in [(archive::MEDIA, 75), (archive::TITLE, 8)] {
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
        assert_eq!(decoded.len(), expect);
    }
    let media = cache.archive(archive::MEDIA).unwrap();
    let (compass, end) = sprite::sprite_entries(&media)
        .unwrap()
        .into_iter()
        .find(|(e, _)| e.name == Some("compass.dat"))
        .unwrap();
    let c = sprite::decode_block(&compass.data, media.get("index.dat").unwrap(), end).unwrap();
    assert_eq!(c.sprites.len(), 1);
    assert_eq!((c.sprites[0].width, c.sprites[0].height), (51, 51));
    // Fully opaque: the client clips it to a disc with a mask when drawing.
    assert!(c.sprites[0].pixels.iter().all(|&p| p != 0));
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
        // The space glyph is a 1×1 sprite; its advance is borrowed.
        let space = &f.glyphs.sprites[32];
        assert_eq!((space.width, space.height), (1, 1), "{name}");
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
    let tab = by_id[&3213];
    let Content::Layer { children, .. } = &tab.content else {
        panic!("3213 is not a layer")
    };
    assert!(children.iter().any(|c| c.id == 3214));
    let inv = by_id[&3214];
    assert!(matches!(inv.content, Content::Inventory { .. }));
    assert_eq!((inv.width, inv.height), (4, 7));
}
