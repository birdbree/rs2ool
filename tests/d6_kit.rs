//! The player kit's default slots rebuild the converter's default body, an
//! item in an empty slot adds its equipment model, and an item in a
//! kit-backed slot replaces that slot's kit part.

#![cfg(feature = "explorer")]

use std::path::PathBuf;

use rs2cache::types::record::{Record, RecordType};
use rs2ool::explorer::kit::{SLOT_KIT, load_base};
use rs2ool::explorer::renderable::Renderable;
use rs2ool::explorer::sources::{Source, open_cache};

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

    // In 317 an item's definition doesn't say which slot it goes in; the
    // kit's slot logic alone decides whether it adds to the body or
    // replaces a kit part. So any item with a male equipment model that
    // has faces will do for an item-only slot, and for a kit-backed slot
    // one whose model differs in size from the kit part it replaces, so
    // the replacement shows in the face count.
    let equip_faces = |obj: u32| {
        rs2convert::creature::equip_model(records, obj, false)
            .unwrap()
            .map(|m| m.faces.len())
    };
    let (weapon_slot, body_slot) = (3, 4);
    assert!(SLOT_KIT[weapon_slot].is_none() && SLOT_KIT[body_slot].is_some());
    let torso = base
        .parts
        .iter()
        .find(|(p, _)| Some(p.idk) == defaults[body_slot].kit)
        .map(|(_, m)| m.faces.len())
        .unwrap();
    assert!(torso > 0, "the default torso draws");
    let mut worn = records
        .ids(RecordType::Obj)
        .unwrap()
        .into_iter()
        .filter(|&id| {
            matches!(records.decode(RecordType::Obj, id), Ok(Record::Obj(d))
                if d.male_equip1.is_some())
        })
        .filter_map(|id| Some((id, equip_faces(id)?)))
        .filter(|&(_, f)| f > 0);
    let (held, _) = worn.next().expect("an item with a male equipment model");
    let (body, _) = worn
        .find(|&(_, f)| f != torso)
        .expect("a second item whose model differs in size from the torso");

    let mut slots = defaults;
    slots[weapon_slot].item = held;
    let armed = base.build(&slots, false).unwrap();
    assert_eq!(
        faces(&armed),
        faces(&dressed) + equip_faces(held).unwrap(),
        "an item in an item-only slot adds its model"
    );

    let mut slots = defaults;
    slots[body_slot].item = body;
    let replaced = base.build(&slots, false).unwrap();
    assert_eq!(
        faces(&replaced),
        faces(&dressed) - torso + equip_faces(body).unwrap(),
        "an item in a kit-backed slot replaces the kit part"
    );

    // "Wear in its slot": every item whose wear models point at a slot goes
    // in the kit view's slot of that name; the rest say why not.
    use rs2ool::explorer::kit::SLOTS;
    for (&item, g) in &base.guesses {
        match (g.slot, base.slot_of(item)) {
            (Some(s), Ok(i)) => assert_eq!(SLOTS[i], s.name(), "item {item}"),
            (None, Err(_)) => {}
            (s, r) => panic!("item {item}: inferred {s:?}, placed {r:?}"),
        }
        assert!(base.explain(item).starts_with(&format!("item {item}")));
    }
    assert!(
        base.slot_of(u32::MAX).is_err(),
        "an unknown item can't be placed"
    );
}
