//! `rigs` record.
//!
//! ```text
//! magic "RRIG", version u8 = 2
//! boneCount u8
//! bones      boneCount × {nameHash u32, parent u8 (255 = none), restPos i16×3, restRot u8×3}
//! labelBone  256 × u8   (bone for each vertex label; 255 = unbound)
//! ```
//!
//! Each bone is 14 bytes, so a record is `6 + 14 × boneCount + 256` bytes.
//! Bones are ordered so every parent precedes its children, which also rules
//! out cycles. Skinning is rigid: a vertex follows the one bone its label is
//! bound to, with no weights.

use crate::bytes::{Reader, Writer};

use super::{RecordError, finish, magic};

/// Magic at the start of a rig record.
pub const MAGIC: &[u8; 4] = b"RRIG";
/// [`Bone::parent`] for a bone with no parent.
pub const NO_PARENT: u8 = 255;
/// [`Rig::label_bone`] entry for a label that isn't bound to any bone.
pub const UNBOUND: u8 = 255;

/// One bone of a rig.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bone {
    /// 32-bit FNV-1a hash of the bone's name, over its UTF-8 bytes.
    pub name_hash: u32,
    /// Index of the parent bone, always lower than this bone's own;
    /// [`NO_PARENT`] for none.
    pub parent: u8,
    /// Pivot relative to the parent's pivot (or the origin, with no parent),
    /// in pack space.
    pub rest_pos: [i16; 3],
    /// Rest rotation. Stored, but the tools always write zeros and the pose
    /// maths ignores it.
    pub rest_rot: [u8; 3],
}

/// A decoded `rigs` record: the bone hierarchy and the label-to-bone table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rig {
    /// Bones, parents first. At most 254.
    pub bones: Vec<Bone>,
    /// Bone index for each vertex label 0–255, or [`UNBOUND`].
    pub label_bone: [u8; 256],
}

impl Rig {
    fn check(&self) -> Result<(), RecordError> {
        if self.bones.len() > 254 {
            return Err(RecordError::Invalid("more than 254 bones".into()));
        }
        for (i, b) in self.bones.iter().enumerate() {
            // A parent must come strictly earlier, which also rules out cycles.
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

    /// Encode the record, after checking its invariants.
    pub fn encode(&self) -> Result<Vec<u8>, RecordError> {
        self.check()?;
        let mut w = Writer::default();
        w.bytes(MAGIC).u8(super::VERSION).u8(self.bones.len() as u8);
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

    /// Decode a record and check its invariants.
    pub fn decode(data: &[u8]) -> Result<Self, RecordError> {
        let mut r = Reader::new(data);
        magic(&mut r, MAGIC)?;
        // Bone count.
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
