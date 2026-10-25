//! Items (`obj.dat`): an `items` record per definition, its inventory
//! model and wear models as model records, and the names, examine texts
//! and options in one string table.
//!
//! ```text
//! inventory model   WORLD_MODEL | item
//! wear model        EQUIP_MODEL | item << 1 | body   (body 0 male, 1 female)
//! strings           STRINGS_DOMAIN << 8 | locale      (locale 0)
//! ```
//!
//! 317 doesn't record which slot an item is worn in, so the record carries
//! the slot and hide mask inferred from the wear models ([`crate::slot`]),
//! flagged as inferred.

use contentpack::records::item::{self, Icon, ItemRecord, NO_ITEM, Wear, flags, wear_flags};
use contentpack::records::strings;
use contentpack::{Kind, PackWriter};
use rayon::prelude::*;
use rs2cache::Records;
use rs2cache::types::config::ObjDef;
use rs2cache::types::record::{Record, RecordType};

use crate::creature::{equip_model, recolour};
use crate::object::Strings;
use crate::palette::Palette;
use crate::slot::{self, Slot, SlotGuess};
use crate::{ConvertError, model};

/// Base of the inventory model record ids: `WORLD_MODEL | item`.
pub const WORLD_MODEL: u32 = 0x5000_0000;
/// Base of the wear model record ids: `EQUIP_MODEL | item << 1 | body`.
pub const EQUIP_MODEL: u32 = 0x6000_0000;
/// String-table domain for items (records `domain << 8 | locale`).
pub const STRINGS_DOMAIN: u32 = 3;

/// The packs produced for a set of items.
pub struct ItemPacks {
    /// Each item's inventory model and wear models.
    pub models: PackWriter,
    /// One record per item.
    pub items: PackWriter,
    /// The string table (names, examine texts and options), as one record.
    pub strings: PackWriter,
}

/// The record's slot number for an inferred slot.
fn slot_number(s: Slot) -> u8 {
    match s {
        Slot::Head => item::slot::HEAD,
        Slot::Cape => item::slot::CAPE,
        Slot::Amulet => item::slot::AMULET,
        Slot::Weapon => item::slot::WEAPON,
        Slot::Body => item::slot::BODY,
        Slot::Shield => item::slot::SHIELD,
        Slot::Legs => item::slot::LEGS,
        Slot::Hands => item::slot::HANDS,
        Slot::Feet => item::slot::FEET,
    }
}

/// Where an item is worn, from its inferred slot. Not worn when it has no
/// wear model.
pub fn wear(guess: Option<&SlotGuess>) -> Wear {
    let Some(g) = guess else {
        return Wear::default();
    };
    let mut f = wear_flags::INFERRED;
    if !g.wearable {
        f |= wear_flags::HELD_ONLY;
    }
    if g.both_hands {
        f |= wear_flags::BOTH_HANDS;
    }
    Wear {
        slot: g.slot.map_or(item::slot::NONE, slot_number),
        hide_mask: g.covers.iter().fold(0, |m, &p| m | 1 << p),
        flags: f,
    }
}

/// One item's converted models: its inventory model and its two wear
/// models, each encoded, or `None`.
type Models = (Option<Vec<u8>>, [Option<Vec<u8>>; 2]);

/// Convert one item's models.
fn models_of(
    records: &Records,
    id: u32,
    d: &ObjDef,
    palette: &Palette,
) -> Result<Models, ConvertError> {
    // The inventory model, recoloured. Its scale stays in the record.
    let world = match records.decode(RecordType::Model, d.inventory_model.into()) {
        Ok(Record::Model(mut m)) => {
            recolour(&mut m, &d.recolours);
            Some(model::convert(&m, palette)?.encode()?)
        }
        _ => None,
    };
    let mut equip = [None, None];
    for (body, slot) in equip.iter_mut().enumerate() {
        if let Some(m) = equip_model(records, id, body == 1)? {
            *slot = Some(model::convert(&m, palette)?.encode()?);
        }
    }
    Ok((world, equip))
}

/// Convert `ids`.
///
/// An inventory model that won't decode leaves the record's world model
/// as none; a definition or wear model that won't decode is an error.
pub fn convert(
    records: &Records,
    ids: &[u32],
    palette: &Palette,
    source_cache_crc: u32,
) -> Result<ItemPacks, ConvertError> {
    let kit = slot::kit_labels(records)?;
    let defs: Vec<(u32, ObjDef)> = ids
        .iter()
        .map(|&id| match records.decode(RecordType::Obj, id)? {
            Record::Obj(d) => Ok((id, d)),
            _ => unreachable!(),
        })
        .collect::<Result<_, ConvertError>>()?;
    let converted: Vec<Models> = defs
        .par_iter()
        .map(|(id, d)| models_of(records, *id, d, palette))
        .collect::<Result<_, _>>()?;

    let mut packs = ItemPacks {
        models: PackWriter::new(Kind::Models, source_cache_crc),
        items: PackWriter::new(Kind::Items, source_cache_crc),
        strings: PackWriter::new(Kind::Strings, source_cache_crc),
    };
    let mut strings = Strings::default();
    for ((id, d), (world, equip)) in defs.iter().zip(converted) {
        let world_ref = world.map(|bytes| {
            packs.models.insert(WORLD_MODEL | id, bytes);
            WORLD_MODEL | id
        });
        let mut equip_refs = [item::NONE; 2];
        for (body, bytes) in equip.into_iter().enumerate() {
            if let Some(bytes) = bytes {
                let r = EQUIP_MODEL | id << 1 | body as u32;
                packs.models.insert(r, bytes);
                equip_refs[body] = r;
            }
        }
        let mut options = [item::NONE; 10];
        for (o, a) in options
            .iter_mut()
            .zip(d.ground_actions.iter().chain(&d.inventory_actions))
        {
            *o = strings.add(a.as_deref());
        }
        let mut f = 0;
        if d.stackable {
            f |= flags::STACKABLE;
        }
        if d.members {
            f |= flags::MEMBERS;
        }
        let rec = ItemRecord {
            id: *id as u16,
            name: strings.add(d.name.as_deref()),
            examine: strings.add(d.examine.as_deref()),
            flags: f,
            value: d.value,
            world_model: world_ref.unwrap_or(item::NONE),
            held_models: [item::NONE; 2],
            equip_models: equip_refs,
            icon: Icon {
                zoom: d.icon_zoom,
                pitch: d.icon_pitch,
                yaw: d.icon_yaw,
                roll: d.icon_roll,
                offset_x: d.icon_offset_x,
                offset_y: d.icon_offset_y,
            },
            scale: [d.scale_x, d.scale_y, d.scale_z],
            stacks: d.stack_variants.iter().map(|v| (v.count, v.item)).collect(),
            surfaces: d
                .recolours
                .iter()
                .map(|r| (palette.index_of(r.from), palette.index_of(r.to)))
                .collect(),
            wear: wear(slot::guess(records, d, &kit).as_ref()),
            noted: d.noted_id.unwrap_or(NO_ITEM),
            noted_template: d.noted_template.unwrap_or(NO_ITEM),
            options,
        };
        packs.items.insert(*id, rec.encode()?);
    }
    packs
        .strings
        .insert(STRINGS_DOMAIN << 8, strings::encode(&strings.list));
    Ok(packs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wear_from_a_guess() {
        let g = SlotGuess {
            slot: Some(Slot::Body),
            both_hands: false,
            labels: [2, 8, 17].into(),
            wearable: true,
            covers: vec![2, 3],
        };
        let w = wear(Some(&g));
        assert_eq!(w.slot, item::slot::BODY);
        assert_eq!(w.hide_mask, 0b1100);
        assert_eq!(w.flags, wear_flags::INFERRED);
        let held = SlotGuess {
            slot: Some(Slot::Weapon),
            both_hands: true,
            wearable: false,
            covers: vec![],
            ..g
        };
        let w = wear(Some(&held));
        assert_eq!(w.slot, item::slot::WEAPON);
        assert_eq!(
            w.flags,
            wear_flags::INFERRED | wear_flags::HELD_ONLY | wear_flags::BOTH_HANDS
        );
        assert_eq!(wear(None), Wear::default());
    }
}
