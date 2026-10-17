//! Phase D6 player kit: the default slots rebuild the converter's default
//! body, an item in an empty slot adds its equipment model, and an item in
//! a kit-backed slot replaces that slot's kit part.

use std::path::PathBuf;

use rs2explorer::kit::{SLOT_KIT, load_base};
use rs2explorer::renderable::Renderable;
use rs2explorer::sources::{Source, open_cache};
use rs2intermediate::record::{Record, RecordType};

fn faces(r: &Renderable) -> usize {
    r.model().map_or(0, |m| m.faces.len())
}

#[test]
fn kit_slots_dress_the_player() {
    let Some(dir) = std::env::var_os("RS2_CACHE").map(PathBuf::from) else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let cache = open_cache(&dir).unwrap();
    let Source::Cache { records, .. } = &cache else {
        unreachable!()
    };
    let base = load_base(records.clone()).unwrap();
    let defaults = base.defaults(false);
    for (i, s) in defaults.iter().enumerate() {
        assert_eq!(s.kit.is_some(), SLOT_KIT[i].is_some(), "slot {i}");
        assert_eq!(s.item, 0);
    }
    let dressed = base.build(&defaults, false).unwrap();
    assert!(matches!(dressed, Renderable::Creature { .. }));
    assert_eq!(
        faces(&dressed),
        base.creature.model.faces.len(),
        "the defaults are the converter's default body"
    );

    // Items with a male equipment model for the weapon and body slots.
    let equip_faces = |obj: u32| {
        rs2convert::creature::equip_model(records, obj, false)
            .unwrap()
            .map(|m| m.faces.len())
    };
    let items: Vec<u32> = records.ids(RecordType::Obj).unwrap();
    let named = |word: &str| {
        items
            .iter()
            .copied()
            .find(|&id| {
                matches!(records.decode(RecordType::Obj, id), Ok(Record::Obj(d))
                    if d.name.as_deref().is_some_and(|n| n.contains(word)) && d.male_equip1.is_some())
            })
            .unwrap()
    };
    let (sword, plate) = (named("sword"), named("platebody"));

    let mut slots = defaults;
    slots[3].item = sword;
    let armed = base.build(&slots, false).unwrap();
    assert_eq!(
        faces(&armed),
        faces(&dressed) + equip_faces(sword).unwrap(),
        "a weapon adds its model"
    );

    let mut slots = defaults;
    slots[4].item = plate;
    let torso = base
        .parts
        .iter()
        .find(|(p, _)| Some(p.idk) == defaults[4].kit)
        .map(|(_, m)| m.faces.len())
        .unwrap();
    let plated = base.build(&slots, false).unwrap();
    assert_eq!(
        faces(&plated),
        faces(&dressed) - torso + equip_faces(plate).unwrap(),
        "a body item replaces the torso"
    );
}
