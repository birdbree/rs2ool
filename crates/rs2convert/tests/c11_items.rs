//! Every `obj.dat` definition converts to an `items` record that
//! round-trips, its model refs resolve in the models pack, its wear models
//! are there exactly when it has them, its strings resolve, its wear field
//! matches the slot inference, and converting twice gives the same packs.

mod common;

use contentpack::PackReader;
use contentpack::records::item::{self, ItemRecord};
use contentpack::records::strings;
use rs2cache::Records;
use rs2cache::types::record::{Record, RecordType};
use rs2convert::palette::Palette;
use rs2convert::{creature, item as convert_item, slot};

#[test]
fn every_item_converts() {
    let Some(cache) = common::cache() else { return };
    let records = Records::new(&cache);
    let ids = records.ids(RecordType::Obj).unwrap();
    let palette = Palette::from_rgb(vec![[0, 0, 0], [255, 255, 255]]);
    let packs = convert_item::convert(&records, &ids, &palette, 0).unwrap();
    let items = PackReader::from_bytes(&packs.items.bake().1).unwrap();
    let models = PackReader::from_bytes(&packs.models.bake().1).unwrap();
    let table = PackReader::from_bytes(&packs.strings.bake().1).unwrap();
    let table = strings::decode(
        &table
            .get(convert_item::STRINGS_DOMAIN << 8)
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(items.manifest.record_count as usize, ids.len());

    let guesses = slot::guesses(&records).unwrap();
    let (mut worn, mut inferred) = (0, 0);
    for &id in &ids {
        let bytes = items.get(id).unwrap().unwrap();
        let rec = ItemRecord::decode(&bytes).unwrap();
        assert_eq!(rec.encode().unwrap(), bytes, "item {id} round-trips");
        assert_eq!(u32::from(rec.id), id);
        let Record::Obj(d) = records.decode(RecordType::Obj, id).unwrap() else {
            unreachable!()
        };
        // Model refs resolve; wear models exist exactly when the item has
        // them for that body type.
        if rec.world_model != item::NONE {
            assert!(models.contains(rec.world_model), "item {id} world model");
        }
        assert_eq!(rec.held_models, [item::NONE; 2]);
        for (body, &r) in rec.equip_models.iter().enumerate() {
            let has = creature::equip_model(&records, id, body == 1)
                .unwrap()
                .is_some();
            assert_eq!(r != item::NONE, has, "item {id} body {body}");
            if has {
                assert!(models.contains(r));
            }
        }
        // Strings resolve, and the name is the definition's.
        for r in [rec.name, rec.examine].into_iter().chain(rec.options) {
            assert!(
                r == item::NONE || (r as usize) < table.len(),
                "item {id} string {r}"
            );
        }
        if let Some(n) = &d.name {
            assert_eq!(&table[rec.name as usize], n);
        }
        // The wear field is the slot inference, flagged as such.
        assert_eq!(
            rec.wear,
            convert_item::wear(guesses.get(&id)),
            "item {id} wear"
        );
        worn += usize::from(rec.wear.slot != item::slot::NONE);
        inferred += usize::from(rec.wear.flags & item::wear_flags::INFERRED != 0);
    }
    eprintln!(
        "{} items, {} model records, {} strings; {worn} with a slot, {inferred} with wear models",
        ids.len(),
        models.manifest.record_count,
        table.len()
    );
    assert!(worn > 1000);

    // Deterministic: a second run bakes the same bytes.
    let again = convert_item::convert(&records, &ids, &palette, 0).unwrap();
    assert_eq!(again.items.bake().0, packs.items.bake().0);
    assert_eq!(again.models.bake().0, packs.models.bake().0);
}
