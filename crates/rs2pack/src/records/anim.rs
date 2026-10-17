//! `anims` record: one clip.
//!
//! ```text
//! magic "RANM", version u8 = 1
//! rig u32                       (rigs record the clip drives)
//! frameCount u8
//! frameStart frameCount × u16   (start of each frame, in 20 ms client cycles)
//! duration u16                  (client cycles)
//! loopOffset u16                (frames from the end to replay; 65535 = none)
//! priority u8, replayMode u8
//! keyCount u32
//! keys keyCount × {bone u8, t u8 (frame index), rot u8×3, pos i16×3}
//! ```
//!
//! Keys are sorted by `(t, bone)`. Rotations are Euler angles in 1/256 turn,
//! applied z, then x, then y, as the 317 client applies them.

use crate::bytes::{Reader, Writer};

use super::{RecordError, finish, magic};

pub const MAGIC: &[u8; 4] = b"RANM";
pub const NO_LOOP: u16 = u16::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key {
    pub t: u8,
    pub bone: u8,
    pub rot: [u8; 3],
    pub pos: [i16; 3],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clip {
    pub rig: u32,
    pub frame_start: Vec<u16>,
    pub duration: u16,
    pub loop_offset: u16,
    pub priority: u8,
    pub replay_mode: u8,
    pub keys: Vec<Key>,
}

impl Clip {
    fn check(&self) -> Result<(), RecordError> {
        if self.frame_start.len() > 255 {
            return Err(RecordError::Invalid("more than 255 frames".into()));
        }
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

    pub fn encode(&self) -> Result<Vec<u8>, RecordError> {
        self.check()?;
        let mut w = Writer::default();
        w.bytes(MAGIC)
            .u8(1)
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
            w.u8(k.bone)
                .u8(k.t)
                .bytes(&k.rot)
                .i16(k.pos[0])
                .i16(k.pos[1])
                .i16(k.pos[2]);
        }
        Ok(w.0)
    }

    pub fn decode(data: &[u8]) -> Result<Self, RecordError> {
        let mut r = Reader::new(data);
        magic(&mut r, MAGIC)?;
        let rig = r.u32()?;
        let n = r.u8()?;
        let frame_start = (0..n).map(|_| r.u16()).collect::<Result<_, _>>()?;
        let duration = r.u16()?;
        let loop_offset = r.u16()?;
        let priority = r.u8()?;
        let replay_mode = r.u8()?;
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
