//! `objects` and `kits` records.
//!
//! Object:
//! ```text
//! id u16, nameRef u32 (string index, 0xFFFFFFFF = none)
//! width u8, height u8, layer u8, placementFlags u16
//! surfaceCount u8, surfaces {from u16, to u16}           (palette index overrides)
//! modelCount u8, models {type u8, modelRef u32}          (model record per placement type)
//! stateCount u16, states {modelRef u32, collisionMask u16, lightRef u32, soundRef u32, optionRefs u32 × 5}
//! transitionCount u16, transitions {from u16, to u16, animRef u32, ticks u16}
//! (a morph table can name up to 256 children, so states need 16 bits)
//! wear u8 (must be 0)
//! ```
//!
//! String refs index the objects string table (`strings` record `0x200`),
//! and model refs are record ids in a `models` pack. Neither has a header;
//! the decoder checks lengths and `wear`, and nothing else.
//!
//! Kit: `{materialId u16, kind u8, count u16, entries {key u16, modelRef u32}}`.
//! Entries keep the order they were given in, and keys may repeat.

use crate::bytes::{Reader, Writer};

use super::{RecordError, finish};

/// The "no reference" value for any `u32` ref field.
pub const NONE: u32 = u32::MAX;

/// Values of [`ObjectRecord::layer`]: which slot of a tile the object takes.
pub mod layer {
    /// Walls: location types 0–3 and 9.
    pub const WALL: u8 = 0;
    /// Wall decorations: location types 4–8.
    pub const WALL_DECORATION: u8 = 1;
    /// Free-standing objects: location types 10 and 11.
    pub const FURNITURE: u8 = 2;
    /// Roofs: location types 12–21.
    pub const ROOF: u8 = 3;
    /// Floor decorations: location type 22.
    pub const FLOOR_DECORATION: u8 = 4;
}

/// Bits of [`ObjectRecord::placement`]. Each mirrors a flag of the 317
/// location definition the object came from. Bits 8–15 are unused.
pub mod placement {
    /// The model's adjusted to the terrain height under it.
    pub const ADJUST_TO_TERRAIN: u16 = 1;
    /// The definition's obstructs-ground flag.
    pub const OBSTRUCTS_GROUND: u16 = 2;
    /// The definition's occludes flag.
    pub const OCCLUDES: u16 = 4;
    /// The object casts a shadow.
    pub const CASTS_SHADOW: u16 = 8;
    /// The definition's support-items value is non-zero.
    pub const SUPPORTS_ITEMS: u16 = 16;
    /// The definition's hollow flag (a hollow definition is never solid or
    /// impenetrable).
    pub const HOLLOW: u16 = 32;
    /// The model's flat shaded.
    pub const FLAT_SHADED: u16 = 64;
    /// The object has actions.
    pub const INTERACTIVE: u16 = 128;
}

/// Bits of [`State::collision`].
pub mod collision {
    /// The definition's solid flag.
    pub const SOLID: u16 = 1;
    /// The definition's impenetrable flag.
    pub const IMPENETRABLE: u16 = 2;
    /// Bits 4–7 hold the low four bits of the 317 blocked-directions mask,
    /// so it's `mask << BLOCKED_SHIFT`.
    pub const BLOCKED_SHIFT: u16 = 4;
}

/// One state an object can be in. State 0 is the object itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct State {
    /// Model record for this state, or [`NONE`].
    pub model: u32,
    /// Collision bits, see [`collision`].
    pub collision: u16,
    /// Light ref, or [`NONE`]. The tools always write [`NONE`].
    pub light: u32,
    /// Sound ref, or [`NONE`]. The tools always write [`NONE`].
    pub sound: u32,
    /// Five option strings, each a string index or [`NONE`] for an empty
    /// slot.
    pub options: [u32; 5],
}

/// A move from one state to another, with an animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transition {
    /// State index it starts from. Not checked against the state count.
    pub from: u16,
    /// State index it ends in. Not checked either.
    pub to: u16,
    /// Animation ref. The tools write a cache sequence id here, not an
    /// `anims` clip id, so don't go looking for it in an anims pack.
    pub anim: u32,
    /// Length in ticks. The tools write 0.
    pub ticks: u16,
}

/// A decoded `objects` record: one placeable world object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectRecord {
    /// Object id, repeating the record id as a `u16`.
    pub id: u16,
    /// Name, as a string index, or [`NONE`].
    pub name: u32,
    /// Footprint along x, in tiles.
    pub width: u8,
    /// Footprint along the map's other horizontal axis, in tiles.
    pub height: u8,
    /// Which slot of the tile it takes, see [`layer`]. Any value decodes.
    pub layer: u8,
    /// Placement bits, see [`placement`].
    pub placement: u16,
    /// Colour overrides as `(from, to)` palette indices. At most 255.
    pub surfaces: Vec<(u16, u16)>,
    /// `(location type 0–22, model record)` pairs, one per placement type.
    /// At most 255.
    pub models: Vec<(u8, u32)>,
    /// States, object first. At most 65535.
    pub states: Vec<State>,
    /// Transitions between states. At most 65535.
    pub transitions: Vec<Transition>,
}

// Counts that have to fit their u16 / u8 field, or the record's invalid.
fn count16(n: usize, what: &str) -> Result<u16, RecordError> {
    u16::try_from(n).map_err(|_| RecordError::Invalid(format!("too many {what}")))
}

fn count(n: usize, what: &str) -> Result<u8, RecordError> {
    u8::try_from(n).map_err(|_| RecordError::Invalid(format!("too many {what}")))
}

impl ObjectRecord {
    /// Encode the record. Fails if a list is too long for its count field.
    pub fn encode(&self) -> Result<Vec<u8>, RecordError> {
        let mut w = Writer::default();
        w.u16(self.id)
            .u32(self.name)
            .u8(self.width)
            .u8(self.height)
            .u8(self.layer)
            .u16(self.placement);
        w.u8(count(self.surfaces.len(), "surfaces")?);
        for &(f, t) in &self.surfaces {
            w.u16(f).u16(t);
        }
        w.u8(count(self.models.len(), "models")?);
        for &(t, m) in &self.models {
            w.u8(t).u32(m);
        }
        w.u16(count16(self.states.len(), "states")?);
        for s in &self.states {
            w.u32(s.model).u16(s.collision).u32(s.light).u32(s.sound);
            for &o in &s.options {
                w.u32(o);
            }
        }
        w.u16(count16(self.transitions.len(), "transitions")?);
        for t in &self.transitions {
            w.u16(t.from).u16(t.to).u32(t.anim).u16(t.ticks);
        }
        // wear: always none.
        w.u8(0);
        Ok(w.0)
    }

    /// Decode a record. Rejects a non-zero `wear` byte.
    pub fn decode(data: &[u8]) -> Result<Self, RecordError> {
        let mut r = Reader::new(data);
        let (id, name) = (r.u16()?, r.u32()?);
        let (width, height, layer) = (r.u8()?, r.u8()?, r.u8()?);
        let placement = r.u16()?;
        let n = r.u8()?;
        let surfaces = (0..n)
            .map(|_| Ok((r.u16()?, r.u16()?)))
            .collect::<Result<_, RecordError>>()?;
        let n = r.u8()?;
        let models = (0..n)
            .map(|_| Ok((r.u8()?, r.u32()?)))
            .collect::<Result<_, RecordError>>()?;
        let n = r.u16()?;
        let mut states = Vec::new();
        for _ in 0..n {
            let (model, collision, light, sound) = (r.u32()?, r.u16()?, r.u32()?, r.u32()?);
            let mut options = [0; 5];
            for o in &mut options {
                *o = r.u32()?;
            }
            states.push(State {
                model,
                collision,
                light,
                sound,
                options,
            });
        }
        let n = r.u16()?;
        let transitions = (0..n)
            .map(|_| {
                Ok(Transition {
                    from: r.u16()?,
                    to: r.u16()?,
                    anim: r.u32()?,
                    ticks: r.u16()?,
                })
            })
            .collect::<Result<_, RecordError>>()?;
        if r.u8()? != 0 {
            return Err(RecordError::Invalid("wear data is not supported".into()));
        }
        finish(&r)?;
        Ok(Self {
            id,
            name,
            width,
            height,
            layer,
            placement,
            surfaces,
            models,
            states,
            transitions,
        })
    }
}

/// A decoded `kits` record: a table of model variants under one material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KitRecord {
    /// Material id the kit belongs to.
    pub material: u16,
    /// What sort of kit, see [`kit_kind`].
    pub kind: u8,
    /// `(key, model record)` pairs, in the order given. Keys may repeat.
    /// The tools' wall kits use `key = type << 2 | rotation`.
    pub variants: Vec<(u16, u32)>,
}

/// Values of [`KitRecord::kind`].
pub mod kit_kind {
    /// Wall pieces.
    pub const WALL: u8 = 0;
    /// Roof pieces.
    pub const ROOF: u8 = 1;
    /// Floor pieces.
    pub const FLOOR: u8 = 2;
}

impl KitRecord {
    /// Encode the record.
    ///
    /// The count is written as the low 16 bits of the variant count without
    /// a check, so keep a kit under 65536 entries.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.u16(self.material)
            .u8(self.kind)
            .u16(self.variants.len() as u16);
        for &(k, m) in &self.variants {
            w.u16(k).u32(m);
        }
        w.0
    }

    /// Decode a record.
    pub fn decode(data: &[u8]) -> Result<Self, RecordError> {
        let mut r = Reader::new(data);
        let (material, kind) = (r.u16()?, r.u8()?);
        let n = r.u16()?;
        let variants = (0..n)
            .map(|_| Ok((r.u16()?, r.u32()?)))
            .collect::<Result<_, RecordError>>()?;
        finish(&r)?;
        Ok(Self {
            material,
            kind,
            variants,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let o = ObjectRecord {
            id: 1276,
            name: 3,
            width: 1,
            height: 1,
            layer: layer::FURNITURE,
            placement: placement::CASTS_SHADOW,
            surfaces: vec![(4, 9)],
            models: vec![(10, 0x4000_0010)],
            states: vec![State {
                model: 0x4000_0010,
                collision: collision::SOLID,
                light: NONE,
                sound: NONE,
                options: [1, NONE, NONE, NONE, NONE],
            }],
            transitions: vec![Transition {
                from: 0,
                to: 0,
                anim: 808,
                ticks: 0,
            }],
        };
        assert_eq!(ObjectRecord::decode(&o.encode().unwrap()).unwrap(), o);
        let k = KitRecord {
            material: 2000,
            kind: kit_kind::WALL,
            variants: vec![(1, 5), (2, 6)],
        };
        assert_eq!(KitRecord::decode(&k.encode()).unwrap(), k);
    }
}
