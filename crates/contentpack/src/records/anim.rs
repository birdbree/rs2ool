//! `anims` record: one clip.
//!
//! ```text
//! magic "RANM", version u8 = 5
//! rig u32                       (rigs record the clip drives)
//! frameCount u8
//! frameStart frameCount × u16   (start of each frame, in 20 ms cycles)
//! duration u16                  (total length, in 20 ms cycles)
//! loopOffset u16                (frames from the end to replay; 65535 = none)
//! priority u8, replayMode u8
//! movingCount u8
//! moving movingCount × u8       (bones a movement clip drives, ascending)
//! keyCount u32
//! keys keyCount × {bone u8, t u8 (frame index), rot u8×3, pos i16×3, scale u16×3}
//! mixKeyCount u32
//! mixKeys mixKeyCount × key     (the same 17 bytes as a key)
//! fadeCount u32
//! fades fadeCount × {t u8, group u8, add i16, lo u8, hi u8}
//! ```
//!
//! Each key is 17 bytes. Its bytes put `bone` before `t`, but keys are
//! sorted by `(t, bone)`, strictly, so don't let the byte order talk you
//! into sorting the other way. Rotations are Euler angles in 1/256 turn,
//! applied z, then x, then y, as the original client applies them. Scale is
//! per axis, [`SCALE_ONE`] (256) being 1.0. A key's transform is translate ·
//! rotate · scale about the bone's pivot.
//!
//! `moving` lists the bones whose local transform comes from the creature's
//! movement clip, at that clip's own frame, when this clip plays while the
//! creature moves. Every other bone then takes its key from `mixKeys`
//! instead of `keys`: the pose this clip leaves once the movement has taken
//! its share, which isn't the same as the pose it plays alone. With no
//! moving bones there are no mix keys, and the clip always plays alone.
//!
//! A fade sets the opacity of the model's faces in fade group `group` at
//! frame `t` to `clamp(base + add, lo, hi)`, where `base` is the face's
//! opacity in the model (255 when it has none). Faces in a group with no
//! fade at a frame keep their own opacity. Fades are sorted by
//! `(t, group)`, strictly. A mix takes this clip's fades, not the movement
//! clip's.
//!
//! Version 4 is version 5 without the fades, version 3 is version 4
//! without the moving bones and mix keys, and version 2
//! is version 3 without `scale` (11-byte keys). Both still decode, with no
//! moving bones and, for version 2, every key at scale 1.0. None of them
//! has fades. The encoder always writes version 5.
//!
//! Frame `i` runs from `frameStart[i]` to `frameStart[i + 1]`, or to
//! `duration` for the last frame. A bone with no key at a frame sits at its
//! rest offset. There's nothing between frames.

use crate::bytes::{Reader, Writer};

use super::{RecordError, finish, header};

/// Magic at the start of an anim record.
pub const MAGIC: &[u8; 4] = b"RANM";
/// The version the encoder writes. Versions 2 (without scale, moving
/// bones or fades), 3 (without moving bones or fades) and 4 (without fades)
/// still decode.
pub const VERSION: u8 = 5;
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

/// The opacity of one fade group's faces at one frame:
/// `clamp(base + add, lo, hi)` from each face's own opacity.
///
/// Field order matters: the derived `Ord` sorts by `t`, then `group`, which
/// is the order a clip's fades have to be in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Fade {
    /// Frame index, below the clip's frame count.
    pub t: u8,
    /// The fade group, as the model's face groups number them.
    pub group: u8,
    /// Added to each face's opacity, before clamping.
    pub add: i16,
    /// The least opacity the faces end up with.
    pub lo: u8,
    /// The most; at least `lo`.
    pub hi: u8,
}

impl Fade {
    /// A face's opacity under this fade, from its own opacity in the
    /// model.
    pub fn apply(&self, opacity: u8) -> u8 {
        (i32::from(opacity) + i32::from(self.add)).clamp(i32::from(self.lo), i32::from(self.hi))
            as u8
    }
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
    /// The bones whose local transform comes from the creature's movement
    /// clip, at that clip's own frame, while this one plays as the creature
    /// moves; ascending, without repeats. Empty when the clip always plays
    /// alone.
    pub moving: Vec<u8>,
    /// Keys, sorted by `(t, bone)` with no repeats.
    pub keys: Vec<Key>,
    /// Keys for the bones not in `moving`, used instead of `keys` while
    /// the clip plays over a movement clip; sorted like `keys`. Empty when
    /// `moving` is.
    pub mix_keys: Vec<Key>,
    /// Opacity per fade group at the frames that change it, sorted by
    /// `(t, group)` with no repeats.
    pub fades: Vec<Fade>,
}

impl Clip {
    fn check(&self) -> Result<(), RecordError> {
        if self.frame_start.len() > 255 {
            return Err(RecordError::Invalid("more than 255 frames".into()));
        }
        let frames = self.frame_start.len();
        for keys in [&self.keys, &self.mix_keys] {
            // Strictly increasing (t, bone) means sorted with no duplicate
            // pairs.
            if keys
                .windows(2)
                .any(|w| (w[0].t, w[0].bone) >= (w[1].t, w[1].bone))
            {
                return Err(RecordError::Invalid(
                    "keys must be sorted by (t, bone) without repeats".into(),
                ));
            }
            if keys.iter().any(|k| usize::from(k.t) >= frames) {
                return Err(RecordError::Invalid("key frame index out of range".into()));
            }
        }
        if self
            .fades
            .windows(2)
            .any(|w| (w[0].t, w[0].group) >= (w[1].t, w[1].group))
            || self
                .fades
                .iter()
                .any(|f| usize::from(f.t) >= frames || f.lo > f.hi)
        {
            return Err(RecordError::Invalid(
                "fades must be sorted by (t, group) without repeats, in range, with lo ≤ hi".into(),
            ));
        }
        if self.moving.len() > 255 || self.moving.windows(2).any(|w| w[0] >= w[1]) {
            return Err(RecordError::Invalid(
                "moving bones must be ascending without repeats".into(),
            ));
        }
        if self.mix_keys.iter().any(|k| self.moving.contains(&k.bone))
            || (self.moving.is_empty() && !self.mix_keys.is_empty())
        {
            return Err(RecordError::Invalid(
                "mix keys are only for bones not in moving, and only with moving bones".into(),
            ));
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
            .u8(self.moving.len() as u8)
            .bytes(&self.moving)
            .u32(self.keys.len() as u32);
        put_keys(&mut w, &self.keys);
        w.u32(self.mix_keys.len() as u32);
        put_keys(&mut w, &self.mix_keys);
        w.u32(self.fades.len() as u32);
        for f in &self.fades {
            w.u8(f.t).u8(f.group).i16(f.add).u8(f.lo).u8(f.hi);
        }
        Ok(w.0)
    }

    /// Decode a record and check its invariants.
    pub fn decode(data: &[u8]) -> Result<Self, RecordError> {
        let mut r = Reader::new(data);
        let version = header(&mut r, MAGIC, &[2, 3, 4, VERSION])?;
        let rig = r.u32()?;
        // Frame count.
        let n = r.u8()?;
        let frame_start = (0..n).map(|_| r.u16()).collect::<Result<_, _>>()?;
        let duration = r.u16()?;
        let loop_offset = r.u16()?;
        let priority = r.u8()?;
        let replay_mode = r.u8()?;
        let moving = if version >= 4 {
            let m = r.u8()?;
            (0..m).map(|_| r.u8()).collect::<Result<_, _>>()?
        } else {
            Vec::new()
        };
        let keys = get_keys(&mut r, version)?;
        let mix_keys = if version >= 4 {
            get_keys(&mut r, version)?
        } else {
            Vec::new()
        };
        let mut fades = Vec::new();
        if version >= 5 {
            // Capped like the keys: a short record fails as Truncated.
            let n = r.u32()? as usize;
            fades.reserve(n.min(1 << 20));
            for _ in 0..n {
                fades.push(Fade {
                    t: r.u8()?,
                    group: r.u8()?,
                    add: r.i16()?,
                    lo: r.u8()?,
                    hi: r.u8()?,
                });
            }
        }
        finish(&r)?;
        let c = Self {
            rig,
            frame_start,
            duration,
            loop_offset,
            priority,
            replay_mode,
            moving,
            keys,
            mix_keys,
            fades,
        };
        c.check()?;
        Ok(c)
    }
}

fn put_keys(w: &mut Writer, keys: &[Key]) {
    for k in keys {
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
}

fn get_keys(r: &mut Reader, version: u8) -> Result<Vec<Key>, RecordError> {
    // The count comes straight from the data, so cap the up-front
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
    Ok(keys)
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
            moving: vec![2, 5],
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
            mix_keys: vec![Key {
                t: 1,
                bone: 3,
                rot: [4, 5, 6],
                pos: [1, -1, 2],
                scale: [SCALE_ONE; 3],
            }],
            fades: vec![
                Fade {
                    t: 0,
                    group: 4,
                    add: -96,
                    lo: 0,
                    hi: 255,
                },
                Fade {
                    t: 2,
                    group: 1,
                    add: -255,
                    lo: 0,
                    hi: 40,
                },
            ],
        };
        assert_eq!(Clip::decode(&c.encode().unwrap()).unwrap(), c);
        let mut bad = c.clone();
        bad.keys.swap(0, 1);
        assert!(bad.encode().is_err());
        let mut bad = c.clone();
        bad.moving = vec![5, 2];
        assert!(bad.encode().is_err());
        // Mix keys can't drive a moving bone, or exist without any.
        let mut bad = c.clone();
        bad.mix_keys[0].bone = 5;
        assert!(bad.encode().is_err());
        let mut bad = c.clone();
        bad.moving.clear();
        assert!(bad.encode().is_err());
        // Fades sorted, in range, and with lo ≤ hi.
        let mut bad = c.clone();
        bad.fades.swap(0, 1);
        assert!(bad.encode().is_err());
        let mut bad = c.clone();
        bad.fades[1].t = 3;
        assert!(bad.encode().is_err());
        let mut bad = c.clone();
        bad.fades[0].lo = 9;
        bad.fades[0].hi = 8;
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
        assert!(c.moving.is_empty());
        assert!(c.mix_keys.is_empty() && c.fades.is_empty());
        // It re-encodes as version 5: six bytes of scale, an empty moving
        // list, and empty mix key and fade counts longer.
        let v5 = c.encode().unwrap();
        assert_eq!((v5[4], v5.len()), (VERSION, v2.len() + 15));
        // Any other version is refused.
        let mut v9 = v2.clone();
        v9[4] = 9;
        assert!(matches!(Clip::decode(&v9), Err(RecordError::Version(9))));
    }

    #[test]
    fn versions_3_and_4_decode_without_what_came_later() {
        let c = Clip {
            rig: 3,
            frame_start: vec![0, 2],
            duration: 4,
            loop_offset: NO_LOOP,
            priority: 5,
            replay_mode: 2,
            moving: Vec::new(),
            keys: vec![Key {
                t: 1,
                bone: 1,
                rot: [1, 2, 3],
                pos: [4, 5, 6],
                scale: [300, 256, 200],
            }],
            mix_keys: Vec::new(),
            fades: Vec::new(),
        };
        // Version 4 is version 5 without the fade count at the end.
        let mut v4 = c.encode().unwrap();
        assert_eq!(v4.split_off(v4.len() - 4), [0; 4]);
        v4[4] = 4;
        assert_eq!(Clip::decode(&v4).unwrap(), c);
        // Version 3 is version 4 without the moving count, which sits just
        // before the key count, and without the mix key count at the end.
        let mut v3 = v4;
        assert_eq!(v3.split_off(v3.len() - 4), [0; 4]);
        let count = v3.len() - 17 - 4 - 1;
        assert_eq!(v3.remove(count), 0);
        v3[4] = 3;
        assert_eq!(Clip::decode(&v3).unwrap(), c);
    }
}
