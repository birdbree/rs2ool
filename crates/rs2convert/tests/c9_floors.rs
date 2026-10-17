//! Phase C9 check: one floor material per `flo.dat` entry; textured floors
//! reference existing texture materials; the manifest maps every floor.

mod common;

use rs2cache::Records;
use rs2convert::palette::Palette;
use rs2convert::{floor, material_id};
use rs2intermediate::record::RecordType;
use rs2pack::PackReader;
use rs2pack::records::material::{Material, flags, mode};
use rs2pack::records::strings;

#[test]
fn one_material_per_floor() {
    let Some(cache) = common::cache() else { return };
    let records = Records::new(&cache);
    let flos = records.ids(RecordType::Flo).unwrap();
    let palette = Palette::from_rgb(vec![[0, 0, 0], [255, 255, 255]]);
    let (m, s) = floor::packs(&records, &palette, 0).unwrap();
    let (_, mb) = m.bake();
    let r = PackReader::from_bytes(&mb).unwrap();
    assert_eq!(r.manifest.record_count as usize, flos.len());
    assert_eq!(
        r.manifest.extra["floors"].as_object().unwrap().len(),
        flos.len()
    );
    let mut textured = 0;
    for id in &flos {
        let mat = Material::decode(
            &r.get(u32::from(material_id::FLOOR_BASE) + id)
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        assert!(mat.flags & flags::FLOOR != 0);
        if mat.mode == mode::TEXTURE {
            assert!(mat.texture < 50, "floor {id} uses texture {}", mat.texture);
            textured += 1;
        }
    }
    let (_, sb) = s.bake();
    let names = strings::decode(
        &PackReader::from_bytes(&sb)
            .unwrap()
            .get(floor::STRINGS_DOMAIN << 8)
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(names.len(), flos.len());
    eprintln!(
        "{} floors, {textured} textured, names like {:?}",
        flos.len(),
        names
            .iter()
            .filter(|n| !n.is_empty())
            .take(3)
            .collect::<Vec<_>>()
    );
}
