//! `items` records.
//!
//! ```text
//! id u16, nameRef u32, examineRef u32 (string indices, 0xFFFFFFFF = none)
//! flags u8, value u32
//! worldModelRef u32                    (inventory and ground model)
//! heldModelRef u32 × 2                 (none in 317; always NONE)
//! equipModelRef u32 × 2                (male, female wear model, or NONE)
//! iconZoom u16, iconPitch u16, iconYaw u16, iconRoll u16
//! iconOffsetX i16, iconOffsetY i16
//! scaleX u16, scaleY u16, scaleZ u16   (128 = 1.0)
//! stackCount u8, stacks {count u16, item u16}
//! surfaceCount u8, surfaces {from u16, to u16}   (palette index overrides)
//! wearSlot u8, hideMask u16, wearFlags u8
//! notedId u16, notedTemplate u16       (0xFFFF = none)
//! optionRefs u32 × 10                  (five ground, then five inventory)
//! ```
//!
//! String refs index the items string table (`strings` record `0x300`), and
//! model refs are record ids in a `models` pack. There's no header; the
//! decoder checks lengths and nothing else.

use crate::bytes::{Reader, Writer};

use super::{RecordError, finish};

/// The "no reference" value for any `u32` ref field.
pub const NONE: u32 = u32::MAX;

/// The "none" value for [`ItemRecord::noted`] and
/// [`ItemRecord::noted_template`].
pub const NO_ITEM: u16 = u16::MAX;

/// Bits of [`ItemRecord::flags`].
pub mod flags {
    /// Copies stack into one inventory slot.
    pub const STACKABLE: u8 = 1;
    /// Members only.
    pub const MEMBERS: u8 = 2;
}

/// Values of [`Wear::slot`]: the original client's equipment slot numbers.
/// The numbers it skips (6 arms, 8 hair, 11 jaw) are body parts only an
/// identity kit fills.
pub mod slot {
    /// Helmets and hats.
    pub const HEAD: u8 = 0;
    /// Capes.
    pub const CAPE: u8 = 1;
    /// Amulets.
    pub const AMULET: u8 = 2;
    /// Anything in the right hand, or both.
    pub const WEAPON: u8 = 3;
    /// Body armour and tops.
    pub const BODY: u8 = 4;
    /// Anything in the left hand.
    pub const SHIELD: u8 = 5;
    /// Leg armour.
    pub const LEGS: u8 = 7;
    /// Gloves.
    pub const HANDS: u8 = 9;
    /// Boots.
    pub const FEET: u8 = 10;
    /// Not worn, or not known.
    pub const NONE: u8 = 255;
}

/// Bits of [`Wear::flags`].
pub mod wear_flags {
    /// The slot and hide mask were inferred from the wear models, not
    /// stated by the source: a reader may override them.
    pub const INFERRED: u8 = 1;
    /// The item has wear models but no option that puts it on: they're
    /// only shown while it's held during an animation.
    pub const HELD_ONLY: u8 = 2;
    /// A weapon drawn on both hands.
    pub const BOTH_HANDS: u8 = 4;
}

/// Where and how an item is worn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wear {
    /// Equipment slot, see [`slot`].
    pub slot: u8,
    /// The default body parts it hides, one bit per identity-kit body part
    /// 0–6 (hair, jaw, torso, arms, hands, legs, feet).
    pub hide_mask: u16,
    /// See [`wear_flags`].
    pub flags: u8,
}

impl Default for Wear {
    /// Not worn.
    fn default() -> Self {
        Self {
            slot: slot::NONE,
            hide_mask: 0,
            flags: 0,
        }
    }
}

/// How an item's icon is drawn: the camera on its inventory model.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Icon {
    /// Distance from the model.
    pub zoom: u16,
    /// Rotation about x, 2048 per turn.
    pub pitch: u16,
    /// Rotation about y, 2048 per turn.
    pub yaw: u16,
    /// Rotation about z, 2048 per turn.
    pub roll: u16,
    /// Screen offset, x.
    pub offset_x: i16,
    /// Screen offset, y.
    pub offset_y: i16,
}

/// A decoded `items` record: one item definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemRecord {
    /// Item id, repeating the record id as a `u16`.
    pub id: u16,
    /// Name, as a string index, or [`NONE`].
    pub name: u32,
    /// Examine text, as a string index, or [`NONE`].
    pub examine: u32,
    /// See [`flags`].
    pub flags: u8,
    /// Value, in coins.
    pub value: i32,
    /// The inventory and ground model, or [`NONE`].
    pub world_model: u32,
    /// Separate held models (male, female); [`NONE`] each, as 317 has none.
    pub held_models: [u32; 2],
    /// The wear models (male, female), each a model record or [`NONE`].
    pub equip_models: [u32; 2],
    /// The icon camera.
    pub icon: Icon,
    /// Model scale per axis, 128 = 1.0.
    pub scale: [u16; 3],
    /// Stack variants as `(count, item)`: from this count up, the stack
    /// looks like that item. At most 255.
    pub stacks: Vec<(u16, u16)>,
    /// Colour overrides as `(from, to)` palette indices. At most 255.
    pub surfaces: Vec<(u16, u16)>,
    /// Where it's worn.
    pub wear: Wear,
    /// The noted (certificate) version of this item, or [`NO_ITEM`].
    pub noted: u16,
    /// For a noted item, the template it's drawn with, or [`NO_ITEM`].
    pub noted_template: u16,
    /// Options as string indices or [`NONE`]: five on the ground, then five
    /// in the inventory.
    pub options: [u32; 10],
}

fn count(n: usize, what: &str) -> Result<u8, RecordError> {
    u8::try_from(n).map_err(|_| RecordError::Invalid(format!("too many {what}")))
}

impl ItemRecord {
    /// Encode the record. Fails if a list is too long for its count field.
    pub fn encode(&self) -> Result<Vec<u8>, RecordError> {
        let mut w = Writer::default();
        w.u16(self.id)
            .u32(self.name)
            .u32(self.examine)
            .u8(self.flags)
            .u32(self.value as u32)
            .u32(self.world_model);
        for &m in self.held_models.iter().chain(&self.equip_models) {
            w.u32(m);
        }
        let i = &self.icon;
        w.u16(i.zoom)
            .u16(i.pitch)
            .u16(i.yaw)
            .u16(i.roll)
            .i16(i.offset_x)
            .i16(i.offset_y);
        for &s in &self.scale {
            w.u16(s);
        }
        w.u8(count(self.stacks.len(), "stack variants")?);
        for &(c, item) in &self.stacks {
            w.u16(c).u16(item);
        }
        w.u8(count(self.surfaces.len(), "surfaces")?);
        for &(f, t) in &self.surfaces {
            w.u16(f).u16(t);
        }
        w.u8(self.wear.slot)
            .u16(self.wear.hide_mask)
            .u8(self.wear.flags)
            .u16(self.noted)
            .u16(self.noted_template);
        for &o in &self.options {
            w.u32(o);
        }
        Ok(w.0)
    }

    /// Decode a record.
    pub fn decode(data: &[u8]) -> Result<Self, RecordError> {
        let mut r = Reader::new(data);
        let (id, name, examine) = (r.u16()?, r.u32()?, r.u32()?);
        let (flags, value, world_model) = (r.u8()?, r.u32()? as i32, r.u32()?);
        let held_models = [r.u32()?, r.u32()?];
        let equip_models = [r.u32()?, r.u32()?];
        let icon = Icon {
            zoom: r.u16()?,
            pitch: r.u16()?,
            yaw: r.u16()?,
            roll: r.u16()?,
            offset_x: r.i16()?,
            offset_y: r.i16()?,
        };
        let scale = [r.u16()?, r.u16()?, r.u16()?];
        let n = r.u8()?;
        let stacks = (0..n)
            .map(|_| Ok((r.u16()?, r.u16()?)))
            .collect::<Result<_, RecordError>>()?;
        let n = r.u8()?;
        let surfaces = (0..n)
            .map(|_| Ok((r.u16()?, r.u16()?)))
            .collect::<Result<_, RecordError>>()?;
        let wear = Wear {
            slot: r.u8()?,
            hide_mask: r.u16()?,
            flags: r.u8()?,
        };
        let (noted, noted_template) = (r.u16()?, r.u16()?);
        let mut options = [0; 10];
        for o in &mut options {
            *o = r.u32()?;
        }
        finish(&r)?;
        Ok(Self {
            id,
            name,
            examine,
            flags,
            value,
            world_model,
            held_models,
            equip_models,
            icon,
            scale,
            stacks,
            surfaces,
            wear,
            noted,
            noted_template,
            options,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ItemRecord {
        ItemRecord {
            id: 1153,
            name: 0,
            examine: 1,
            flags: flags::MEMBERS,
            value: -5,
            world_model: 0x5000_0481,
            held_models: [NONE; 2],
            equip_models: [0x6000_0902, NONE],
            icon: Icon {
                zoom: 2000,
                pitch: 512,
                yaw: 1024,
                roll: 0,
                offset_x: -3,
                offset_y: 7,
            },
            scale: [128, 140, 128],
            stacks: vec![(2, 996), (3, 997)],
            surfaces: vec![(4, 9)],
            wear: Wear {
                slot: slot::HEAD,
                hide_mask: 1 << 1,
                flags: wear_flags::INFERRED,
            },
            noted: 1154,
            noted_template: NO_ITEM,
            options: [2, NONE, NONE, NONE, NONE, NONE, 3, NONE, NONE, NONE],
        }
    }

    #[test]
    fn round_trips() {
        let r = sample();
        let bytes = r.encode().unwrap();
        assert_eq!(ItemRecord::decode(&bytes).unwrap(), r);
        // Re-encoding gives the same bytes.
        assert_eq!(ItemRecord::decode(&bytes).unwrap().encode().unwrap(), bytes);
    }

    #[test]
    fn bad_input_is_an_error() {
        let bytes = sample().encode().unwrap();
        assert!(matches!(
            ItemRecord::decode(&bytes[..bytes.len() - 1]),
            Err(RecordError::Truncated(_))
        ));
        let mut long = bytes.clone();
        long.push(0);
        assert!(matches!(
            ItemRecord::decode(&long),
            Err(RecordError::Trailing(1))
        ));
        let mut r = sample();
        r.stacks = vec![(0, 0); 256];
        assert!(r.encode().is_err());
    }

    #[test]
    fn not_worn_by_default() {
        assert_eq!(Wear::default().slot, slot::NONE);
    }
}
