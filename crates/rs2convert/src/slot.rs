//! Which equipment slot an item goes in, inferred from its wear models.
//!
//! Item definitions don't say. Wear models are built on the player's
//! skeleton, though, so their vertex labels say where they attach: a
//! helmet carries head labels, boots foot labels, a cape its own run of
//! back labels, and a held item the hand or held-object labels. The
//! default body's identity-kit parts use the same labels, so the same
//! comparison says which kit parts an item covers.
//!
//! Every guess carries its evidence. `docs/conversion/inference.md` has
//! the label groups and how well the rules do.

use std::collections::BTreeSet;

use rs2cache::Records;
use rs2cache::types::config::ObjDef;
use rs2cache::types::record::{Record, RecordType};

use crate::ConvertError;

/// The equipment slots a worn model can show in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Slot {
    /// Helmets, hats, hoods and masks.
    Head,
    /// Capes and cloaks.
    Cape,
    /// Amulets, necklaces and pendants.
    Amulet,
    /// Anything held in the right hand, or in both.
    Weapon,
    /// Body armour, tops and robes.
    Body,
    /// Anything held in the left hand.
    Shield,
    /// Leg armour, trousers and skirts.
    Legs,
    /// Gloves and gauntlets.
    Hands,
    /// Boots.
    Feet,
}

impl Slot {
    /// The slot's name, lowercase, as the explorer's kit view names it.
    pub fn name(self) -> &'static str {
        match self {
            Slot::Head => "head",
            Slot::Cape => "cape",
            Slot::Amulet => "amulet",
            Slot::Weapon => "weapon",
            Slot::Body => "body",
            Slot::Shield => "shield",
            Slot::Legs => "legs",
            Slot::Hands => "hands",
            Slot::Feet => "feet",
        }
    }
}

/// Head labels: the skull and face.
const HEAD: &[u8] = &[1, 3, 49];
/// The neck and upper chest, all an amulet touches.
const NECK: &[u8] = &[2, 8];
/// The labels a cape hangs from.
const CAPE: std::ops::RangeInclusive<u8> = 9..=16;
/// The arms, shoulder to wrist.
const ARMS: std::ops::RangeInclusive<u8> = 17..=26;
/// The forearms gloves reach up.
const FOREARMS: &[u8] = &[19, 22];
/// The right and left hands.
const RIGHT_HAND: u8 = 27;
/// See [`RIGHT_HAND`].
const LEFT_HAND: u8 = 28;
/// The lower torso and waist.
const WAIST: u8 = 4;
/// The legs, hip to ankle.
const LEGS: std::ops::RangeInclusive<u8> = 29..=44;
/// The feet.
const FEET: std::ops::RangeInclusive<u8> = 45..=48;
/// The held-object attachment labels and up: what a weapon hangs from.
const HELD: u8 = 50;

/// The label of vertices that belong to no group.
const UNLABELLED: u8 = 255;

/// The inventory options that put an item on.
const WEAR_OPTIONS: &[&str] = &["wear", "wield", "equip"];

/// An item's inferred slot, with its evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotGuess {
    /// The slot, if the labels point at one.
    pub slot: Option<Slot>,
    /// For the weapon slot: drawn on both hands (claws, for one). Most
    /// two-handed weapons hang from the held-object point like any other
    /// weapon, so this doesn't detect them.
    pub both_hands: bool,
    /// The wear models' vertex labels.
    pub labels: BTreeSet<u8>,
    /// The item has an option that puts it on ("Wear", "Wield" or
    /// "Equip"). Without one, its wear models are only shown while it's
    /// held during an animation, such as a tool in use.
    pub wearable: bool,
    /// The default body's identity-kit body parts (0–6) it covers: those
    /// whose labels it has nearly all of (see [`covers`]).
    pub covers: Vec<u8>,
}

/// The slot a set of wear-model labels points at, and whether it's drawn
/// on both hands. The first rule that fits wins:
///
/// 1. cape labels: cape;
/// 2. foot labels: feet;
/// 3. leg labels: legs;
/// 4. both hands with a forearm, and nothing above the forearm: hands;
/// 5. arm labels or the waist: body;
/// 6. head labels: head;
/// 7. nothing but the neck and upper chest: amulet;
/// 8. the left hand only: shield;
/// 9. the right hand, both hands or the held-object labels: weapon.
pub fn slot_for(labels: &BTreeSet<u8>) -> (Option<Slot>, bool) {
    let any = |r: &dyn Fn(u8) -> bool| labels.iter().any(|&l| r(l));
    let has = |l: u8| labels.contains(&l);
    let both_hands = has(RIGHT_HAND) && has(LEFT_HAND);
    let upper_arm = labels
        .iter()
        .any(|l| ARMS.contains(l) && !FOREARMS.contains(l));
    let slot = if any(&|l| CAPE.contains(&l)) {
        Slot::Cape
    } else if any(&|l| FEET.contains(&l)) {
        Slot::Feet
    } else if any(&|l| LEGS.contains(&l)) {
        Slot::Legs
    } else if both_hands && FOREARMS.iter().any(|f| has(*f)) && !upper_arm {
        Slot::Hands
    } else if upper_arm || any(&|l| ARMS.contains(&l)) && !both_hands || has(WAIST) {
        Slot::Body
    } else if HEAD.iter().any(|h| has(*h)) {
        Slot::Head
    } else if !labels.is_empty() && labels.iter().all(|l| NECK.contains(l)) {
        Slot::Amulet
    } else if has(LEFT_HAND) && !has(RIGHT_HAND) && !any(&|l| l >= HELD && l != UNLABELLED) {
        Slot::Shield
    } else if has(RIGHT_HAND) || both_hands || any(&|l| l >= HELD && l != UNLABELLED) {
        return (Some(Slot::Weapon), both_hands);
    } else {
        return (None, false);
    };
    (Some(slot), false)
}

/// The identity-kit body parts an item with `labels` covers: each part
/// whose own labels (from `kit`, one set per body part 0–6) the item has
/// at least 80% of. Labels a kit part shares with most others (0 and
/// 255, which mark unassigned vertices) don't count.
pub fn covers(labels: &BTreeSet<u8>, kit: &[BTreeSet<u8>]) -> Vec<u8> {
    kit.iter()
        .enumerate()
        .filter_map(|(part, own)| {
            let own: Vec<u8> = own
                .iter()
                .copied()
                .filter(|l| ![0, 255].contains(l))
                .collect();
            let have = own.iter().filter(|l| labels.contains(l)).count();
            (!own.is_empty() && have * 5 >= own.len() * 4).then_some(part as u8)
        })
        .collect()
}

/// The labels of a model, or none if it won't decode.
fn model_labels(records: &Records, model: u16) -> BTreeSet<u8> {
    match records.decode(RecordType::Model, model.into()) {
        Ok(Record::Model(m)) => m.vertex_labels.unwrap_or_default().into_iter().collect(),
        _ => BTreeSet::new(),
    }
}

/// Each default-body identity-kit part's labels, for body parts 0–6 (the
/// first body type): every label any of that part's kit models uses.
pub fn kit_labels(records: &Records) -> Result<Vec<BTreeSet<u8>>, ConvertError> {
    let mut out = vec![BTreeSet::new(); 7];
    for id in records.ids(RecordType::Idk)? {
        let Ok(Record::Idk(k)) = records.decode(RecordType::Idk, id) else {
            continue;
        };
        if let Some(p) = k.body_part.filter(|&p| p < 7) {
            for &m in &k.models {
                out[usize::from(p)].extend(model_labels(records, m));
            }
        }
    }
    Ok(out)
}

/// An item's inferred slot, from its male wear models (the female ones
/// are built on the same skeleton). `None` when it has no wear model.
pub fn guess(records: &Records, item: &ObjDef, kit: &[BTreeSet<u8>]) -> Option<SlotGuess> {
    let first = item.male_equip1?;
    let mut labels = model_labels(records, first);
    for m in [item.male_equip2, item.male_equip3].into_iter().flatten() {
        labels.extend(model_labels(records, m));
    }
    let (slot, both_hands) = slot_for(&labels);
    let wearable = item
        .inventory_actions
        .iter()
        .flatten()
        .any(|a| WEAR_OPTIONS.contains(&a.to_lowercase().as_str()));
    let covers = covers(&labels, kit);
    Some(SlotGuess {
        slot,
        both_hands,
        labels,
        wearable,
        covers,
    })
}

/// Every item with a wear model, with its inferred slot, by item id.
pub fn guesses(
    records: &Records,
) -> Result<std::collections::BTreeMap<u32, SlotGuess>, ConvertError> {
    let kit = kit_labels(records)?;
    let mut out = std::collections::BTreeMap::new();
    for id in records.ids(RecordType::Obj)? {
        if let Ok(Record::Obj(o)) = records.decode(RecordType::Obj, id)
            && let Some(g) = guess(records, &o, &kit)
        {
            out.insert(id, g);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(labels: &[u8]) -> (Option<Slot>, bool) {
        slot_for(&labels.iter().copied().collect())
    }

    #[test]
    fn labels_point_at_slots() {
        assert_eq!(slot(&[1]), (Some(Slot::Head), false));
        assert_eq!(slot(&[1, 2, 3, 49]), (Some(Slot::Head), false));
        assert_eq!(slot(&[2, 8]), (Some(Slot::Amulet), false));
        assert_eq!(slot(&[8]), (Some(Slot::Amulet), false));
        assert_eq!(slot(&[2, 8, 9, 10, 11]), (Some(Slot::Cape), false));
        assert_eq!(
            slot(&[0, 32, 38, 45, 46, 47, 48]),
            (Some(Slot::Feet), false)
        );
        assert_eq!(slot(&[19, 22, 27, 28]), (Some(Slot::Hands), false));
        assert_eq!(slot(&[2, 4, 5, 8]), (Some(Slot::Body), false));
        assert_eq!(slot(&[2, 8, 17, 18, 19, 20, 21]), (Some(Slot::Body), false));
        assert_eq!(slot(&[4, 5, 29, 30, 31]), (Some(Slot::Legs), false));
        assert_eq!(slot(&[28]), (Some(Slot::Shield), false));
        assert_eq!(slot(&[28, 255]), (Some(Slot::Shield), false));
        assert_eq!(slot(&[27]), (Some(Slot::Weapon), false));
        assert_eq!(slot(&[50]), (Some(Slot::Weapon), false));
        assert_eq!(slot(&[27, 28]), (Some(Slot::Weapon), true));
        assert_eq!(slot(&[255]), (None, false));
        assert_eq!(slot(&[]), (None, false));
    }

    #[test]
    fn covering_needs_nearly_all_of_a_parts_labels() {
        let kit: Vec<BTreeSet<u8>> =
            vec![[1, 2, 3, 69].into(), [1, 3].into(), [17, 18, 19, 20].into()];
        // A full helm has 1, 2 and 3 of the hair part (3 of 4: 75%) and
        // all of the jaw.
        assert_eq!(covers(&[1, 2, 3].into(), &kit), [1]);
        assert_eq!(covers(&[1, 2, 3, 69].into(), &kit), [0, 1]);
        assert_eq!(covers(&[17, 18, 19, 20, 2].into(), &kit), [2]);
    }
}
