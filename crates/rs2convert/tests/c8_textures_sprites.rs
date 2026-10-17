//! Phase C8 check: the texture atlas round-trips every texture pixel-exact,
//! every sprite round-trips through its pack record, and the compass is a
//! single opaque 51×51 sprite.

mod common;

use rs2cache::{Records, archive};
use rs2convert::sprite;
use rs2convert::texture::{TextureAtlas, images};
use rs2intermediate::record::{Record, RecordType};
use rs2pack::PackReader;
use rs2pack::records::sprite::{FontMetrics, SpriteRecord};
use rs2pack::records::texture::{ATLAS_BASE, Atlas, TextureRect};

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
fn sprites_round_trip_and_compass() {
    let Some(cache) = common::cache() else { return };
    let records = Records::new(&cache);
    let (_, bytes) = sprite::pack(&records, &[archive::MEDIA, archive::TITLE], 0)
        .unwrap()
        .bake();
    let r = PackReader::from_bytes(&bytes).unwrap();
    let mut subs = 0;
    for id in records.ids(RecordType::Sprite).unwrap() {
        let Record::Sprite(sheet) = records.decode(RecordType::Sprite, id).unwrap() else {
            unreachable!()
        };
        let (_, entry) = records.sprite_source(id).unwrap();
        for i in 0..sheet.sprites.len() {
            let rec = SpriteRecord::decode(&r.get(id << 8 | i as u32).unwrap().unwrap()).unwrap();
            assert_eq!(rec.name_hash, entry.hash() as u32);
            assert_eq!(rec.rgba, sheet.rgba(i).unwrap(), "sprite {id}.{i}");
            subs += 1;
        }
        if entry.to_string() == "compass.dat" {
            let rec = SpriteRecord::decode(&r.get(id << 8).unwrap().unwrap()).unwrap();
            assert_eq!((rec.width, rec.height), (51, 51));
            assert!(
                rec.rgba.chunks(4).all(|p| p[3] == 255),
                "compass is fully opaque in 317"
            );
        }
    }
    for f in 0..4 {
        for g in 0..256 {
            assert!(r.get(sprite::GLYPH_BASE | f << 8 | g).is_some());
        }
        FontMetrics::decode(&r.get(sprite::METRICS_BASE | f).unwrap().unwrap()).unwrap();
    }
    eprintln!("{subs} sprites and 4 fonts round-trip");
}
