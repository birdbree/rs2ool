//! `rigs` record.
//!
//! ```text
//! magic "RRIG", version u8 = 1
//! boneCount u8
//! bones      boneCount × {nameHash u32, parent u8 (255 = none), restPos i16×3, restRot u8×3}
//! labelBone  256 × u8   (bone for each vertex label; 255 = unbound)
//! ```
//!
//! Bones are ordered so every parent precedes its children.

use crate::bytes::{Reader, Writer};

use super::{RecordError, finish, magic};

pub const MAGIC: &[u8; 4] = b"RRIG";
pub const NO_PARENT: u8 = 255;
pub const UNBOUND: u8 = 255;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bone {
    pub name_hash: u32,
    pub parent: u8,
    pub rest_pos: [i16; 3],
    pub rest_rot: [u8; 3],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rig {
    pub bones: Vec<Bone>,
    pub label_bone: [u8; 256],
}

impl Rig {
    fn check(&self) -> Result<(), RecordError> {
        if self.bones.len() > 254 {
            return Err(RecordError::Invalid("more than 254 bones".into()));
        }
        for (i, b) in self.bones.iter().enumerate() {
            if b.parent != NO_PARENT && usize::from(b.parent) >= i {
                return Err(RecordError::Invalid(format!(
                    "bone {i}'s parent {} does not precede it",
                    b.parent
                )));
            }
        }
        if self
            .label_bone
            .iter()
            .any(|&b| b != UNBOUND && usize::from(b) >= self.bones.len())
        {
            return Err(RecordError::Invalid("label bound to a missing bone".into()));
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>, RecordError> {
        self.check()?;
        let mut w = Writer::default();
        w.bytes(MAGIC).u8(1).u8(self.bones.len() as u8);
        for b in &self.bones {
            w.u32(b.name_hash)
                .u8(b.parent)
                .i16(b.rest_pos[0])
                .i16(b.rest_pos[1])
                .i16(b.rest_pos[2])
                .bytes(&b.rest_rot);
        }
        w.bytes(&self.label_bone);
        Ok(w.0)
    }

    pub fn decode(data: &[u8]) -> Result<Self, RecordError> {
        let mut r = Reader::new(data);
        magic(&mut r, MAGIC)?;
        let n = r.u8()?;
        let mut bones = Vec::with_capacity(n.into());
        for _ in 0..n {
            bones.push(Bone {
                name_hash: r.u32()?,
                parent: r.u8()?,
                rest_pos: [r.i16()?, r.i16()?, r.i16()?],
                rest_rot: [r.u8()?, r.u8()?, r.u8()?],
            });
        }
        let label_bone = r.bytes(256)?.try_into().expect("256 bytes");
        finish(&r)?;
        let rig = Self { bones, label_bone };
        rig.check()?;
        Ok(rig)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_checks_order() {
        let mut label_bone = [UNBOUND; 256];
        label_bone[3] = 1;
        let rig = Rig {
            bones: vec![
                Bone {
                    name_hash: 1,
                    parent: NO_PARENT,
                    rest_pos: [0, 0, 0],
                    rest_rot: [0; 3],
                },
                Bone {
                    name_hash: 2,
                    parent: 0,
                    rest_pos: [0, 100, 5],
                    rest_rot: [1, 2, 3],
                },
            ],
            label_bone,
        };
        assert_eq!(Rig::decode(&rig.encode().unwrap()).unwrap(), rig);
        let mut bad = rig.clone();
        bad.bones[0].parent = 1;
        assert!(bad.encode().is_err());
    }
}
