//! The texture atlas round-trips every texture pixel-exact, and every
//! sprite round-trips through its pack record.

mod common;

use contentpack::PackReader;
use contentpack::records::sprite::{FontMetrics, SpriteRecord};
use contentpack::records::texture::{ATLAS_BASE, Atlas, TextureRect};
use rs2cache::types::record::{Record, RecordType};
use rs2cache::{Records, archive};
use rs2convert::sprite;
use rs2convert::texture::{TextureAtlas, images};

#[test]
fn atlas_round_trips_pixel_exact() {
    let Some(cache) = common::cache() else { return };
    let records = Records::new(&cache);
    let imgs = images(&records).unwrap();
    let (_, bytes) = TextureAtlas::build(&imgs).unwrap().pack(0).unwrap().bake();
    let r = PackReader::from_bytes(&bytes).unwrap();
    let atlas = Atlas::decode(&r.get(ATLAS_BASE).unwrap().unwrap()).unwrap();
    for t in &imgs {
        let rect = TextureRect::decode(&r.get(t.id.into()).unwrap().unwrap()).unwrap();
        assert_eq!((rect.width, rect.height), (t.width, t.height));
        assert_eq!(
            TextureAtlas::extract(&atlas, &rect),
            t.rgba,
            "texture {}",
            t.id
        );
    }
}

#[test]
fn sprites_round_trip_and_opaque_sprite_stays_opaque() {
    let Some(cache) = common::cache() else { return };
    let records = Records::new(&cache);
    let (_, bytes) = sprite::pack(&records, &[archive::MEDIA, archive::TITLE], 0)
        .unwrap()
        .bake();
    let r = PackReader::from_bytes(&bytes).unwrap();
    let mut subs = 0;
    // The first media sheet holding one sub-sprite with no transparent
    // (palette index 0) pixel.
    let mut opaque = None;
    for id in records.ids(RecordType::Sprite).unwrap() {
        let Record::Sprite(sheet) = records.decode(RecordType::Sprite, id).unwrap() else {
            unreachable!()
        };
        let (archive_id, entry) = records.sprite_source(id).unwrap();
        for (i, s) in sheet.sprites.iter().enumerate() {
            let rec = SpriteRecord::decode(&r.get(id << 8 | i as u32).unwrap().unwrap()).unwrap();
            assert_eq!(rec.name_hash, entry.hash() as u32);
            assert_eq!(usize::from(rec.sub_index), i, "sprite {id}.{i} index");
            assert_eq!(
                (rec.width, rec.height, rec.x_offset, rec.y_offset),
                (s.width, s.height, s.x_offset, s.y_offset),
                "sprite {id}.{i} size and offsets"
            );
            assert_eq!(rec.rgba, sheet.rgba(i).unwrap(), "sprite {id}.{i}");
            subs += 1;
        }
        if opaque.is_none()
            && archive_id == archive::MEDIA
            && sheet.sprites.len() == 1
            && !sheet.sprites[0].pixels.is_empty()
            && sheet.sprites[0].pixels.iter().all(|&p| p != 0)
        {
            opaque = Some(id);
        }
    }
    let id = opaque.expect("no media sheet with one fully opaque sub-sprite");
    let rec = SpriteRecord::decode(&r.get(id << 8).unwrap().unwrap()).unwrap();
    eprintln!("opaque sprite {id}: {}x{}", rec.width, rec.height);
    assert_eq!(
        rec.rgba.len(),
        usize::from(rec.width) * usize::from(rec.height) * 4
    );
    assert!(
        rec.rgba.chunks(4).all(|p| p[3] == 255),
        "sprite {id} has no transparent pixel in the cache, so none in the pack"
    );
    let fonts = rs2cache::sprite::FONTS.len() as u32;
    for f in 0..fonts {
        for g in 0..256 {
            assert!(r.get(sprite::GLYPH_BASE | f << 8 | g).is_some());
        }
        FontMetrics::decode(&r.get(sprite::METRICS_BASE | f).unwrap().unwrap()).unwrap();
    }
    eprintln!("{subs} sprites and {fonts} fonts round-trip");
}
