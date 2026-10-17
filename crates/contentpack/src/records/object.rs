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
//! wear u8 (0 = none)
//! ```
//! Kit: `{materialId u16, kind u8, count u16, entries {key u16, modelRef u32}}`.

use crate::bytes::{Reader, Writer};

use super::{RecordError, finish};

pub const NONE: u32 = u32::MAX;

pub mod layer {
    pub const WALL: u8 = 0;
    pub const WALL_DECORATION: u8 = 1;
    pub const FURNITURE: u8 = 2;
    pub const ROOF: u8 = 3;
    pub const FLOOR_DECORATION: u8 = 4;
}

pub mod placement {
    pub const ADJUST_TO_TERRAIN: u16 = 1;
    pub const OBSTRUCTS_GROUND: u16 = 2;
    pub const OCCLUDES: u16 = 4;
    pub const CASTS_SHADOW: u16 = 8;
    pub const SUPPORTS_ITEMS: u16 = 16;
    pub const HOLLOW: u16 = 32;
    pub const FLAT_SHADED: u16 = 64;
    pub const INTERACTIVE: u16 = 128;
}

pub mod collision {
    pub const SOLID: u16 = 1;
    pub const IMPENETRABLE: u16 = 2;
    /// Bits 4–7 hold the 317 blocked-directions mask.
    pub const BLOCKED_SHIFT: u16 = 4;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct State {
    pub model: u32,
    pub collision: u16,
    pub light: u32,
    pub sound: u32,
    pub options: [u32; 5],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transition {
    pub from: u16,
    pub to: u16,
    pub anim: u32,
    pub ticks: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectRecord {
    pub id: u16,
    pub name: u32,
    pub width: u8,
    pub height: u8,
    pub layer: u8,
    pub placement: u16,
    pub surfaces: Vec<(u16, u16)>,
    pub models: Vec<(u8, u32)>,
    pub states: Vec<State>,
    pub transitions: Vec<Transition>,
}

fn count16(n: usize, what: &str) -> Result<u16, RecordError> {
    u16::try_from(n).map_err(|_| RecordError::Invalid(format!("too many {what}")))
}

fn count(n: usize, what: &str) -> Result<u8, RecordError> {
    u8::try_from(n).map_err(|_| RecordError::Invalid(format!("too many {what}")))
}

impl ObjectRecord {
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
        w.u8(0);
        Ok(w.0)
    }

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KitRecord {
    pub material: u16,
    pub kind: u8,
    pub variants: Vec<(u16, u32)>,
}

pub mod kit_kind {
    pub const WALL: u8 = 0;
    pub const ROOF: u8 = 1;
    pub const FLOOR: u8 = 2;
}

impl KitRecord {
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
