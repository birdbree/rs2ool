//! `anims` record: one clip.
//!
//! ```text
//! magic "RANM", version u8 = 3
//! rig u32                       (rigs record the clip drives)
//! frameCount u8
//! frameStart frameCount × u16   (start of each frame, in 20 ms cycles)
//! duration u16                  (total length, in 20 ms cycles)
//! loopOffset u16                (frames from the end to replay; 65535 = none)
//! priority u8, replayMode u8
//! keyCount u32
//! keys keyCount × {bone u8, t u8 (frame index), rot u8×3, pos i16×3, scale u16×3}
//! ```
//!
//! Each key is 17 bytes. Its bytes put `bone` before `t`, but keys are
//! sorted by `(t, bone)`, strictly, so don't let the byte order talk you
//! into sorting the other way. Rotations are Euler angles in 1/256 turn,
//! applied z, then x, then y, as the original client applies them. Scale is
//! per axis, [`SCALE_ONE`] (256) being 1.0. A key's transform is translate ·
//! rotate · scale about the bone's pivot.
//!
//! Version 2 is the same without `scale` (11-byte keys). It still decodes,
//! every key at scale 1.0; the encoder always writes version 3.
//!
//! Frame `i` runs from `frameStart[i]` to `frameStart[i + 1]`, or to
//! `duration` for the last frame. A bone with no key at a frame sits at its
//! rest offset. There's nothing between frames.

use crate::bytes::{Reader, Writer};

use super::{RecordError, finish, header};

/// Magic at the start of an anim record.
pub const MAGIC: &[u8; 4] = b"RANM";
/// The version the encoder writes. Version 2, without scale, still
/// decodes.
pub const VERSION: u8 = 3;
/// A key scale of 1.0.
pub const SCALE_ONE: u16 = 256;
/// [`Clip::loop_offset`] when the clip doesn't loop.
pub const NO_LOOP: u16 = u16::MAX;

/// One bone's local transform at one frame.
///
/// Field order matters: the derived `Ord` sorts by `t`, then `bone`, which
/// is the order a clip's keys have to be in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key {
    /// Frame index, below the clip's frame count.
    pub t: u8,
    /// Bone index in the rig. Not checked against the rig.
    pub bone: u8,
    /// `[rx, ry, rz]` in 1/256 turn, applied z, then x, then y.
    pub rot: [u8; 3],
    /// Translation added to the bone's rest offset, in pack space.
    pub pos: [i16; 3],
    /// Scale per pack-space axis, [`SCALE_ONE`] being 1.0.
    pub scale: [u16; 3],
}

/// A decoded `anims` record: one clip for one rig.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clip {
    /// Id of the `rigs` record this clip drives.
    pub rig: u32,
    /// Start of each frame, in 20 ms cycles. At most 255 frames. Nothing
    /// checks that it's increasing.
    pub frame_start: Vec<u16>,
    /// Total length, in 20 ms cycles.
    pub duration: u16,
    /// Number of frames from the end to replay when looping, or [`NO_LOOP`].
    pub loop_offset: u16,
    /// Priority, copied from the source sequence.
    pub priority: u8,
    /// Replay mode, copied from the source sequence.
    pub replay_mode: u8,
    /// Keys, sorted by `(t, bone)` with no repeats.
    pub keys: Vec<Key>,
}

impl Clip {
    fn check(&self) -> Result<(), RecordError> {
        if self.frame_start.len() > 255 {
            return Err(RecordError::Invalid("more than 255 frames".into()));
        }
        // Strictly increasing (t, bone) means sorted with no duplicate pairs.
        if self
            .keys
            .windows(2)
            .any(|w| (w[0].t, w[0].bone) >= (w[1].t, w[1].bone))
        {
            return Err(RecordError::Invalid(
                "keys must be sorted by (t, bone) without repeats".into(),
            ));
        }
        if self
            .keys
            .iter()
            .any(|k| usize::from(k.t) >= self.frame_start.len())
        {
            return Err(RecordError::Invalid("key frame index out of range".into()));
        }
        Ok(())
    }

    /// Encode the record, after checking its invariants.
    pub fn encode(&self) -> Result<Vec<u8>, RecordError> {
        self.check()?;
        let mut w = Writer::default();
        w.bytes(MAGIC)
            .u8(VERSION)
            .u32(self.rig)
            .u8(self.frame_start.len() as u8);
        for &s in &self.frame_start {
            w.u16(s);
        }
        w.u16(self.duration)
            .u16(self.loop_offset)
            .u8(self.priority)
            .u8(self.replay_mode)
            .u32(self.keys.len() as u32);
        for k in &self.keys {
            // On disk it's bone first, then t.
            w.u8(k.bone)
                .u8(k.t)
                .bytes(&k.rot)
                .i16(k.pos[0])
                .i16(k.pos[1])
                .i16(k.pos[2])
                .u16(k.scale[0])
                .u16(k.scale[1])
                .u16(k.scale[2]);
        }
        Ok(w.0)
    }

    /// Decode a record and check its invariants.
    pub fn decode(data: &[u8]) -> Result<Self, RecordError> {
        let mut r = Reader::new(data);
        let version = header(&mut r, MAGIC, &[2, VERSION])?;
        let rig = r.u32()?;
        // Frame count.
        let n = r.u8()?;
        let frame_start = (0..n).map(|_| r.u16()).collect::<Result<_, _>>()?;
        let duration = r.u16()?;
        let loop_offset = r.u16()?;
        let priority = r.u8()?;
        let replay_mode = r.u8()?;
        // Key count. It comes straight from the data, so cap the up-front
        // allocation and let a short record fail as Truncated instead.
        let k = r.u32()? as usize;
        let mut keys = Vec::with_capacity(k.min(1 << 20));
        for _ in 0..k {
            let bone = r.u8()?;
            let t = r.u8()?;
            keys.push(Key {
                t,
                bone,
                rot: [r.u8()?, r.u8()?, r.u8()?],
                pos: [r.i16()?, r.i16()?, r.i16()?],
                scale: if version == 2 {
                    [SCALE_ONE; 3]
                } else {
                    [r.u16()?, r.u16()?, r.u16()?]
                },
            });
        }
        finish(&r)?;
        let c = Self {
            rig,
            frame_start,
            duration,
            loop_offset,
            priority,
            replay_mode,
            keys,
        };
        c.check()?;
        Ok(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let c = Clip {
            rig: 1,
            frame_start: vec![0, 4, 9],
            duration: 12,
            loop_offset: NO_LOOP,
            priority: 5,
            replay_mode: 2,
            keys: vec![
                Key {
                    t: 0,
                    bone: 0,
                    rot: [0, 0, 0],
                    pos: [0, 0, 0],
                    scale: [SCALE_ONE; 3],
                },
                Key {
                    t: 0,
                    bone: 3,
                    rot: [10, 250, 1],
                    pos: [-5, 6, 7],
                    scale: [128, 512, 300],
                },
                Key {
                    t: 2,
                    bone: 1,
                    rot: [1, 2, 3],
                    pos: [0, 0, 0],
                    scale: [SCALE_ONE; 3],
                },
            ],
        };
        assert_eq!(Clip::decode(&c.encode().unwrap()).unwrap(), c);
        let mut bad = c.clone();
        bad.keys.swap(0, 1);
        assert!(bad.encode().is_err());
    }

    #[test]
    fn version_2_decodes_at_scale_one() {
        // A version 2 record: one 11-byte key with no scale.
        let mut v2 = MAGIC.to_vec();
        v2.push(2);
        v2.extend(7u32.to_le_bytes());
        v2.push(1); // one frame
        v2.extend(0u16.to_le_bytes());
        v2.extend(4u16.to_le_bytes()); // duration
        v2.extend(NO_LOOP.to_le_bytes());
        v2.extend([5, 0]); // priority, replay mode
        v2.extend(1u32.to_le_bytes());
        v2.extend([2, 0, 1, 2, 3]); // bone 2, t 0, rot
        for p in [4i16, -5, 6] {
            v2.extend(p.to_le_bytes());
        }
        let c = Clip::decode(&v2).unwrap();
        assert_eq!(c.keys[0].bone, 2);
        assert_eq!(c.keys[0].pos, [4, -5, 6]);
        assert_eq!(c.keys[0].scale, [SCALE_ONE; 3]);
        // It re-encodes as version 3, six bytes longer.
        let v3 = c.encode().unwrap();
        assert_eq!((v3[4], v3.len()), (VERSION, v2.len() + 6));
        // Any other version is refused.
        let mut v9 = v2.clone();
        v9[4] = 9;
        assert!(matches!(Clip::decode(&v9), Err(RecordError::Version(9))));
    }
}
