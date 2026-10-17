//! Animation frame-pack decoder (index 2).
//!
//! The last 8 bytes are four `u16` section lengths, L1–L4. The sections
//! start at offset 0 and run back to back, with the frame base after them:
//!
//! ```text
//! section 1, at 0              frameCount u16, then per frame
//!                              {frameId u16, slotCount u8}
//! section 2, at L1 + 2         per frame, per slot 0..slotCount: mask u8
//!                              (bit 1 = dx, 2 = dy, 4 = dz follows)
//! section 3, at + L2           the signed smarts the masks announce
//! section 4, at + L3           per frame: delay u8
//! frame base, at + L4          count u8, kind u8 × count, then per slot
//!                              labelCount u8 and that many labels
//! trailer                      L1, L2, L3, L4 as u16
//! ```
//!
//! Yes, L1 leaves out section 1's own leading count, hence the `+ 2`. The
//! four sections are read in step, one frame at a time.
//!
//! Frame ids are global 16-bit ids; nothing in the id says which pack holds
//! it. [`FrameIndex`] builds that map by scanning every pack.

use std::collections::BTreeMap;

use crate::types::anim::{BaseTransform, Frame, FramePack, FrameTransform, kind};
use thiserror::Error;

use crate::io::{Cursor, IoError};

const TRAILER: usize = 8;

/// A frame pack that wouldn't decode.
#[derive(Debug, Error)]
pub enum FrameError {
    /// The file is shorter than the trailer.
    #[error("{len} bytes is too short for the 8-byte trailer")]
    TooShort {
        /// File length.
        len: usize,
    },
    /// The frame base would start past the trailer.
    #[error("sections need {need} bytes before the trailer but {have} are present")]
    SectionOverrun {
        /// Where the base would start.
        need: usize,
        /// Bytes before the trailer.
        have: usize,
    },
    /// A section or the base ran out mid-read.
    #[error("truncated: {0}")]
    Truncated(#[from] IoError),
    /// A frame covers more slots than the base has.
    #[error("frame {frame} uses {slots} base slots but the base has {base}")]
    SlotOutOfRange {
        /// Global frame id.
        frame: u16,
        /// The frame's slot count.
        slots: u8,
        /// The base's slot count.
        base: usize,
    },
    /// Strict mode only: a section wasn't read to exactly its stated length.
    #[error("section {section} ends at {end} but its length says {expected}")]
    SectionLength {
        /// Section 1–4, or 5 for the frame base.
        section: u8,
        /// Where reading actually stopped.
        end: usize,
        /// Where the trailer says it should stop.
        expected: usize,
    },
}

/// Section start offsets, from the trailer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// Section 1, frame ids and slot counts (always 0).
    pub ids: usize,
    /// Section 2, masks.
    pub masks: usize,
    /// Section 3, signed-smart values.
    pub deltas: usize,
    /// Section 4, delays.
    pub delays: usize,
    /// The frame base.
    pub base: usize,
    /// Bytes before the trailer.
    pub body_len: usize,
}

impl Layout {
    /// Read the trailer and work out where each section starts.
    pub fn read(data: &[u8]) -> Result<Self, FrameError> {
        let body_len = data
            .len()
            .checked_sub(TRAILER)
            .ok_or(FrameError::TooShort { len: data.len() })?;
        let mut c = Cursor::at(data, body_len)?;
        let lens = [c.u16()?, c.u16()?, c.u16()?, c.u16()?].map(usize::from);
        // L1 doesn't count section 1's leading frameCount u16.
        let masks = lens[0] + 2;
        let deltas = masks + lens[1];
        let delays = deltas + lens[2];
        let base = delays + lens[3];
        if base > body_len {
            return Err(FrameError::SectionOverrun {
                need: base,
                have: body_len,
            });
        }
        Ok(Self {
            ids: 0,
            masks,
            deltas,
            delays,
            base,
            body_len,
        })
    }
}

/// Decode an inflated frame pack. With `strict`, every section must be read
/// to exactly its stated length, which is handy for proving the layout.
pub fn decode_with(data: &[u8], strict: bool) -> Result<FramePack, FrameError> {
    let l = Layout::read(data)?;
    let body = &data[..l.body_len];

    // Read the base first: frames need its slot kinds to expand.
    let mut c = Cursor::at(body, l.base)?;
    let n = c.u8()?;
    let kinds = c.bytes(n.into())?.to_vec();
    let mut base = Vec::with_capacity(kinds.len());
    for kind in kinds {
        let count = c.u8()?;
        base.push(BaseTransform {
            kind,
            labels: c.bytes(count.into())?.to_vec(),
        });
    }
    let base_end = c.pos();

    let mut ids = Cursor::at(body, l.ids)?;
    let mut masks = Cursor::at(body, l.masks)?;
    let mut deltas = Cursor::at(body, l.deltas)?;
    let mut delays = Cursor::at(body, l.delays)?;
    // Walk all four sections in step, one frame at a time.
    let count = ids.u16()?;
    let mut frames = Vec::with_capacity(count.into());
    for _ in 0..count {
        let id = ids.u16()?;
        let delay = delays.u8()?;
        let slots = ids.u8()?;
        if usize::from(slots) > base.len() {
            return Err(FrameError::SlotOutOfRange {
                frame: id,
                slots,
                base: base.len(),
            });
        }
        let mut transforms = Vec::new();
        // The last slot this frame used, of any kind.
        let mut last_used: Option<usize> = None;
        let mut alpha = false;
        for slot in 0..usize::from(slots) {
            let mask = masks.u8()?;
            // Mask 0: the frame doesn't touch this slot.
            if mask == 0 {
                continue;
            }
            let k = base[slot].kind;
            if k != kind::ORIGIN {
                // Apply the nearest skipped origin before this slot, so the
                // transform pivots where the base intends. Only look back as
                // far as the slot after the last one used.
                let floor = last_used.map_or(0, |u| u + 1);
                if let Some(origin) = (floor..slot).rev().find(|&s| base[s].kind == kind::ORIGIN) {
                    transforms.push(FrameTransform {
                        transform: origin as u8,
                        dx: 0,
                        dy: 0,
                        dz: 0,
                    });
                }
            }
            // An unset component means "no change": 0, or 1.0 for scale.
            let default = if k == kind::SCALE { 128 } else { 0 };
            let mut read = |bit: u8| -> Result<i32, IoError> {
                Ok(if mask & bit != 0 {
                    deltas.signed_smart()?.into()
                } else {
                    default
                })
            };
            transforms.push(FrameTransform {
                transform: slot as u8,
                dx: read(1)?,
                dy: read(2)?,
                dz: read(4)?,
            });
            last_used = Some(slot);
            alpha |= k == kind::ALPHA;
        }
        frames.push(Frame {
            id,
            delay,
            transforms,
            alpha,
        });
    }

    if strict {
        // (section number, where reading stopped, where it should have).
        let ends = [
            (1, ids.pos(), l.masks),
            (2, masks.pos(), l.deltas),
            (3, deltas.pos(), l.delays),
            (4, delays.pos(), l.base),
            (5, base_end, l.body_len),
        ];
        for (section, end, expected) in ends {
            if end != expected {
                return Err(FrameError::SectionLength {
                    section,
                    end,
                    expected,
                });
            }
        }
    }
    Ok(FramePack { base, frames })
}

/// Decode an inflated frame pack as the original client does: trust the
/// section offsets and don't check where each section ends.
pub fn decode(data: &[u8]) -> Result<FramePack, FrameError> {
    decode_with(data, false)
}

/// Global frame id → index-2 file, built from every pack's id list.
///
/// Nothing in the cache stores this map (the original client just loads
/// every pack at startup), and the versionlist's `anim_index` isn't it.
/// [`Cache::frame_index`](crate::Cache::frame_index) builds one from every
/// present pack.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrameIndex {
    /// Which file defines each frame id. When several do, the last one
    /// scanned wins.
    pub pack_of: BTreeMap<u16, u32>,
    /// Frame ids that more than one pack defines, with every pack.
    pub duplicates: BTreeMap<u16, Vec<u32>>,
}

impl FrameIndex {
    /// Read only the id lists of `packs`, given as `(file id, inflated
    /// bytes)`. On error, returns the file that failed with its error.
    pub fn build<'a>(
        packs: impl IntoIterator<Item = (u32, &'a [u8])>,
    ) -> Result<Self, (u32, FrameError)> {
        let mut out = Self::default();
        for (file, data) in packs {
            for id in frame_ids(data).map_err(|e| (file, e))? {
                if let Some(prev) = out.pack_of.insert(id, file) {
                    out.duplicates
                        .entry(id)
                        .or_insert_with(|| vec![prev])
                        .push(file);
                }
            }
        }
        Ok(out)
    }

    /// The file that defines `frame`, if any.
    pub fn pack_for(&self, frame: u16) -> Option<u32> {
        self.pack_of.get(&frame).copied()
    }
}

/// The frame ids a pack defines, without decoding the frames.
pub fn frame_ids(data: &[u8]) -> Result<Vec<u16>, FrameError> {
    let l = Layout::read(data)?;
    let mut c = Cursor::at(&data[..l.body_len], l.ids)?;
    let n = c.u16()?;
    let mut ids = Vec::with_capacity(n.into());
    for _ in 0..n {
        ids.push(c.u16()?);
        // Skip the slot count.
        c.skip(1)?;
    }
    Ok(ids)
}

#[cfg(test)]
pub(crate) mod synth {
    //! Encode frame packs for tests. Frames are given as the masks and
    //! deltas actually stored, per base slot.

    pub struct RawFrame {
        pub id: u16,
        pub delay: u8,
        /// `(mask, deltas)` per slot, for slots `0..len`.
        pub slots: Vec<(u8, Vec<i32>)>,
    }

    pub fn encode(base: &[(u8, Vec<u8>)], frames: &[RawFrame]) -> Vec<u8> {
        let mut ids = (frames.len() as u16).to_be_bytes().to_vec();
        let (mut masks, mut deltas, mut delays) = (Vec::new(), Vec::new(), Vec::new());
        for f in frames {
            ids.extend(f.id.to_be_bytes());
            ids.push(f.slots.len() as u8);
            delays.push(f.delay);
            for (mask, ds) in &f.slots {
                masks.push(*mask);
                for &d in ds {
                    if (-64..64).contains(&d) {
                        deltas.push((d + 64) as u8);
                    } else {
                        deltas.extend(((d + 49152) as u16).to_be_bytes());
                    }
                }
            }
        }
        let mut b = vec![base.len() as u8];
        b.extend(base.iter().map(|(k, _)| *k));
        for (_, labels) in base {
            b.push(labels.len() as u8);
            b.extend(labels);
        }
        let lens = [ids.len() - 2, masks.len(), deltas.len(), delays.len()];
        let mut out = [ids, masks, deltas, delays, b].concat();
        for n in lens {
            out.extend((n as u16).to_be_bytes());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::synth::{RawFrame, encode};
    use super::*;

    fn base() -> Vec<(u8, Vec<u8>)> {
        vec![
            (kind::ORIGIN, vec![1, 2]),
            (kind::ROTATE, vec![1, 2]),
            (kind::ORIGIN, vec![3]),
            (kind::SCALE, vec![3]),
            (kind::ALPHA, vec![4]),
        ]
    }

    #[test]
    fn decodes_with_implicit_origins_and_defaults() {
        let data = encode(
            &base(),
            &[
                // All three bits on the rotate slot; the origin is skipped
                // so it is inserted with zero offset.
                RawFrame {
                    id: 700,
                    delay: 4,
                    slots: vec![(0, vec![]), (7, vec![10, -20, 300])],
                },
                // Scale with only y present: x and z default to 128. The
                // origin at slot 2 is inserted after the used origin 0.
                RawFrame {
                    id: 701,
                    delay: 3,
                    slots: vec![
                        (1, vec![5]),
                        (0, vec![]),
                        (0, vec![]),
                        (2, vec![64]),
                        (1, vec![-2]),
                    ],
                },
                // No transforms at all.
                RawFrame {
                    id: 702,
                    delay: 0,
                    slots: vec![],
                },
            ],
        );
        let p = decode_with(&data, true).unwrap();
        assert_eq!(p.base.len(), 5);
        assert_eq!(p.base[3].labels, [3]);
        let t = |transform, dx, dy, dz| FrameTransform {
            transform,
            dx,
            dy,
            dz,
        };

        assert_eq!(p.frames[0].id, 700);
        assert_eq!(p.frames[0].delay, 4);
        assert_eq!(p.frames[0].transforms, [t(0, 0, 0, 0), t(1, 10, -20, 300)]);
        assert!(!p.frames[0].alpha);

        assert_eq!(
            p.frames[1].transforms,
            [
                t(0, 5, 0, 0),
                t(2, 0, 0, 0),
                t(3, 128, 64, 128),
                t(4, -2, 0, 0)
            ]
        );
        assert!(p.frames[1].alpha);

        assert!(p.frames[2].transforms.is_empty());
        assert_eq!(frame_ids(&data).unwrap(), [700, 701, 702]);
    }

    #[test]
    fn slot_count_past_the_base_is_an_error() {
        let data = encode(
            &base()[..1],
            &[RawFrame {
                id: 1,
                delay: 1,
                slots: vec![(0, vec![]), (0, vec![])],
            }],
        );
        assert!(matches!(
            decode(&data),
            Err(FrameError::SlotOutOfRange {
                slots: 2,
                base: 1,
                ..
            })
        ));
    }

    #[test]
    fn truncation_is_an_error() {
        let data = encode(
            &base(),
            &[RawFrame {
                id: 1,
                delay: 1,
                slots: vec![(7, vec![1, 2, 3])],
            }],
        );
        for len in [0, 4, 9, data.len() - 9] {
            assert!(decode(&data[..len]).is_err(), "len {len}");
        }
    }

    #[test]
    fn frame_index_finds_packs_and_duplicates() {
        let a = encode(
            &base(),
            &[
                RawFrame {
                    id: 5,
                    delay: 1,
                    slots: vec![],
                },
                RawFrame {
                    id: 6,
                    delay: 1,
                    slots: vec![],
                },
            ],
        );
        let b = encode(
            &base(),
            &[RawFrame {
                id: 6,
                delay: 1,
                slots: vec![],
            }],
        );
        let idx = FrameIndex::build([(10, a.as_slice()), (11, b.as_slice())]).unwrap();
        assert_eq!(idx.pack_for(5), Some(10));
        assert_eq!(idx.pack_for(7), None);
        assert_eq!(idx.duplicates[&6], [10, 11]);
    }
}
