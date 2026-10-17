//! Phase C10 check: every `loc.dat` definition converts; object 1276 (a
//! tree) is furniture with its definition's footprint; every definition
//! with a morph table has more than one state; wall definitions produce
//! kits entries.

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
    let tree = get(1276);
    let Record::Loc(def) = records.decode(RecordType::Loc, 1276).unwrap() else {
        unreachable!()
    };
    eprintln!(
        "1276 {:?}: layer {}, footprint {}x{}",
        def.name, tree.layer, tree.width, tree.height
    );
    assert_eq!(def.name.as_deref(), Some("Tree"));
    assert_eq!(tree.layer, layer::FURNITURE);
    assert_eq!((tree.width, tree.height), (def.size_x, def.size_y));

    let mut morphs = 0;
    let mut names = std::collections::BTreeMap::new();
    for &id in &ids {
        let Record::Loc(d) = records.decode(RecordType::Loc, id).unwrap() else {
            unreachable!()
        };
        let rec = get(id);
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
            *names.entry(d.name.clone().unwrap_or_default()).or_insert(0) += 1;
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
        }
    }
    let mut common: Vec<_> = names.into_iter().collect();
    common.sort_by_key(|c| std::cmp::Reverse(c.1));
    eprintln!(
        "{} objects, {morphs} with morph tables (most common: {:?}), {} kits",
        ids.len(),
        &common[..common.len().min(8)],
        kits.manifest.record_count
    );
    // No 317 door has a morph table (the server swaps door objects), so the
    // plan's door example is checked as "every morph definition".
    assert!(morphs > 0);
    assert!(kits.manifest.record_count > 0);
}
