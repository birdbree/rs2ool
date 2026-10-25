//! Equipment slots from wear-model labels.
//!
//! With the cache: reports how every wearable item's slot comes out, and
//! checks two independent signals agree with the label rule: which default
//! body parts an item covers, and that NPCs dressed in items rarely wear
//! two in one slot.
//!
//! With `RS2_ITEM_SLOTS` too (a local JSON file of
//! `[{"item": 1153, "slot": "head"}, …]`, checked by eye; it's not in the
//! repository): every item in it gets the slot it lists.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use rs2cache::Records;
use rs2cache::types::record::{Record, RecordType};
use rs2convert::slot::{self, Slot};

/// The slot the default body parts an item covers point at (hair and jaw:
/// head; torso and arms: body; hands; legs; feet), or `None` when they
/// point at more than one.
fn overlap_slot(covers: &[u8]) -> Option<Slot> {
    let slots: BTreeSet<Slot> = covers
        .iter()
        .map(|p| match p {
            0 | 1 => Slot::Head,
            2 | 3 => Slot::Body,
            4 => Slot::Hands,
            5 => Slot::Legs,
            _ => Slot::Feet,
        })
        .collect();
    match slots.len() {
        1 => slots.into_iter().next(),
        _ => None,
    }
}

#[test]
fn slots_from_wear_models() {
    let Some(cache) = common::cache() else { return };
    let records = Records::new(&cache);
    let guesses = slot::guesses(&records).unwrap();

    let mut counts = BTreeMap::<&str, (usize, usize)>::new();
    let (mut agree, mut compared, mut unplaced) = (0, 0, 0);
    for g in guesses.values() {
        match g.slot {
            Some(s) => {
                let c = counts.entry(s.name()).or_default();
                c.0 += 1;
                c.1 += usize::from(!g.wearable);
            }
            None => unplaced += 1,
        }
        // Items that cover default body parts, judged by those alone.
        if let Some(o) = overlap_slot(&g.covers).filter(|_| g.wearable) {
            compared += 1;
            agree += usize::from(g.slot == Some(o));
        }
    }
    eprintln!(
        "{} items with wear models: (slot, (items, held only)) {counts:?}, {unplaced} unplaced",
        guesses.len()
    );
    eprintln!("label rule agrees with body-part overlap for {agree} of {compared}");

    // NPCs dressed in items: their models, minus identity-kit body parts
    // (which items sometimes reuse), shouldn't hold two items in a slot.
    let mut kit_models = BTreeSet::new();
    for id in records.ids(RecordType::Idk).unwrap() {
        if let Ok(Record::Idk(k)) = records.decode(RecordType::Idk, id) {
            kit_models.extend(k.models.iter().copied());
        }
    }
    let mut slot_of_model = BTreeMap::new();
    for (&id, g) in &guesses {
        let Ok(Record::Obj(o)) = records.decode(RecordType::Obj, id) else {
            continue;
        };
        if let (Some(m), Some(s)) = (o.male_equip1, g.slot) {
            slot_of_model.insert(m, s);
        }
    }
    let (mut dressed, mut clashing) = (0, 0);
    for id in records.ids(RecordType::Npc).unwrap() {
        let Ok(Record::Npc(n)) = records.decode(RecordType::Npc, id) else {
            continue;
        };
        let worn: Vec<Slot> = n
            .models
            .iter()
            .filter(|m| !kit_models.contains(m))
            .filter_map(|m| slot_of_model.get(m).copied())
            .collect();
        if worn.len() < 2 {
            continue;
        }
        dressed += 1;
        let distinct: BTreeSet<_> = worn.iter().collect();
        clashing += usize::from(distinct.len() < worn.len());
    }
    eprintln!("{dressed} NPCs wear two or more items; {clashing} of them two in one slot");

    assert!(guesses.len() > 1000);
    assert!(
        unplaced * 50 <= guesses.len(),
        "nearly every item is placed"
    );
    assert!(
        agree * 100 >= compared * 95,
        "overlap agrees: {agree} of {compared}"
    );
    assert!(
        clashing * 10 <= dressed,
        "outfits rarely clash: {clashing} of {dressed}"
    );

    // The hand-checked items, when the local file is there.
    let Some(path) = std::env::var_os("RS2_ITEM_SLOTS") else {
        eprintln!("RS2_ITEM_SLOTS is not set: skipping the hand-checked items");
        return;
    };
    let checked: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut wrong = Vec::new();
    for c in &checked {
        let item = c["item"].as_u64().unwrap() as u32;
        let want = c["slot"].as_str().unwrap();
        let got = guesses.get(&item).and_then(|g| g.slot).map(Slot::name);
        if got != Some(want) {
            wrong.push(format!("item {item}: {got:?}, not {want}"));
        }
    }
    eprintln!(
        "hand-checked: {} of {} right",
        checked.len() - wrong.len(),
        checked.len()
    );
    assert!(wrong.is_empty(), "{wrong:?}");
}
