//! `anims` record: one clip.
//!
//! ```text
//! magic "RANM", version u8 = 2
//! rig u32                       (rigs record the clip drives)
//! frameCount u8
//! frameStart frameCount × u16   (start of each frame, in 20 ms cycles)
//! duration u16                  (total length, in 20 ms cycles)
//! loopOffset u16                (frames from the end to replay; 65535 = none)
//! priority u8, replayMode u8
//! keyCount u32
//! keys keyCount × {bone u8, t u8 (frame index), rot u8×3, pos i16×3}
//! ```
//!
//! Each key is 11 bytes. Its bytes put `bone` before `t`, but keys are
//! sorted by `(t, bone)`, strictly, so don't let the byte order talk you
//! into sorting the other way. Rotations are Euler angles in 1/256 turn,
//! applied z, then x, then y, as the original client applies them.
//!
//! Frame `i` runs from `frameStart[i]` to `frameStart[i + 1]`, or to
//! `duration` for the last frame. A bone with no key at a frame sits at its
//! rest offset. There are no scale keys and nothing between frames.

use crate::bytes::{Reader, Writer};

use super::{RecordError, finish, magic};

/// Magic at the start of an anim record.
pub const MAGIC: &[u8; 4] = b"RANM";
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
            .u8(super::VERSION)
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
                .i16(k.pos[2]);
        }
        Ok(w.0)
    }

    /// Decode a record and check its invariants.
    pub fn decode(data: &[u8]) -> Result<Self, RecordError> {
        let mut r = Reader::new(data);
        magic(&mut r, MAGIC)?;
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
                },
                Key {
                    t: 0,
                    bone: 3,
                    rot: [10, 250, 1],
                    pos: [-5, 6, 7],
                },
                Key {
                    t: 2,
                    bone: 1,
                    rot: [1, 2, 3],
                    pos: [0, 0, 0],
                },
            ],
        };
        assert_eq!(Clip::decode(&c.encode().unwrap()).unwrap(), c);
        let mut bad = c.clone();
        bad.keys.swap(0, 1);
        assert!(bad.encode().is_err());
    }
}
