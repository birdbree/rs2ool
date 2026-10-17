//! Every `loc.dat` definition converts, footprints carry over, every
//! definition with a morph table has more than one state, and wall
//! definitions produce kits entries.

mod common;

use contentpack::PackReader;
use contentpack::records::object::{KitRecord, ObjectRecord, layer};
use rs2cache::Records;
use rs2cache::types::record::{Record, RecordType};
use rs2convert::object;
use rs2convert::palette::Palette;

#[test]
fn every_object_converts() {
    let Some(cache) = common::cache() else { return };
    let records = Records::new(&cache);
    let ids = records.ids(RecordType::Loc).unwrap();
    let palette = Palette::from_rgb(vec![[0, 0, 0], [255, 255, 255]]);
    let packs = object::convert(&records, &ids, &palette, 0).unwrap();
    let objects = PackReader::from_bytes(&packs.objects.bake().1).unwrap();
    let kits = PackReader::from_bytes(&packs.kits.bake().1).unwrap();
    assert_eq!(objects.manifest.record_count as usize, ids.len());

    let get = |id: u32| ObjectRecord::decode(&objects.get(id).unwrap().unwrap()).unwrap();
    let def = |id: u32| {
        let Record::Loc(d) = records.decode(RecordType::Loc, id).unwrap() else {
            unreachable!()
        };
        d
    };

    // The first named centrepiece (every model type 10, or untyped) whose
    // footprint isn't 1x1.
    let (big_id, big) = ids
        .iter()
        .map(|&id| (id, def(id)))
        .find(|(_, d)| {
            d.name.as_deref().is_some_and(|n| !n.is_empty())
                && !d.models.is_empty()
                && d.models.iter().all(|m| matches!(m.kind, None | Some(10)))
                && (d.size_x, d.size_y) != (1, 1)
        })
        .expect("no named centrepiece loc with a footprint larger than 1x1");
    let rec = get(big_id);
    eprintln!(
        "loc {big_id}: layer {}, footprint {}x{}",
        rec.layer, rec.width, rec.height
    );
    assert_eq!(rec.layer, layer::FURNITURE);
    assert_eq!((rec.width, rec.height), (big.size_x, big.size_y));

    let mut morphs = 0;
    for &id in &ids {
        let d = def(id);
        let rec = get(id);
        assert_eq!(
            (rec.width, rec.height),
            (d.size_x, d.size_y),
            "loc {id} footprint"
        );
        if d.morph
            .as_ref()
            .is_some_and(|m| m.children.iter().any(Option::is_some))
        {
            morphs += 1;
            assert!(
                rec.states.len() > 1,
                "morph loc {id} has {} states",
                rec.states.len()
            );
        }
        if d.models.iter().any(|m| matches!(m.kind, Some(0..=3 | 9))) {
            let k = KitRecord::decode(
                &kits
                    .get(id)
                    .unwrap_or_else(|| panic!("wall {id} has no kit"))
                    .unwrap(),
            )
            .unwrap();
            assert!(!k.variants.is_empty());
            // A material that exists: the palette material `pack materials` writes.
            assert_eq!(k.material, rs2convert::material_id::PALETTE);
        }
    }
    eprintln!(
        "{} objects, {morphs} with morph tables, {} kits",
        ids.len(),
        kits.manifest.record_count
    );
    // Every definition with a morph table gets more than one state.
    assert!(morphs > 0);
    assert!(kits.manifest.record_count > 0);
}
