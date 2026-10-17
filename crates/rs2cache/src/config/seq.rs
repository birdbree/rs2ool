//! `seq.dat`: animation sequences.

use crate::types::config::{SeqDef, SeqFrame};

use super::{ConfigType, opcodes, opt_u16};
use crate::io::{Cursor, IoError};

pub struct Seq;

impl ConfigType for Seq {
    type Def = SeqDef;
    const KIND: &'static str = "seq";
    const DAT: &'static str = "seq.dat";
    const IDX: Option<&'static str> = None;

    fn decode(id: u32, c: &mut Cursor) -> Result<SeqDef, IoError> {
        let mut d = SeqDef {
            id,
            frames: Vec::new(),
            loop_offset: None,
            interleave_labels: None,
            stretches: false,
            priority: 5,
            offhand_override: None,
            mainhand_override: None,
            max_loops: 99,
            animating_precedence: 0,
            walking_precedence: 0,
            replay_mode: 2,
            unknown_opcodes: Vec::new(),
        };
        let (mut animating, mut walking) = (None, None);
        d.unknown_opcodes = opcodes(Seq::KIND, id, c, |op, c| {
            match op {
                1 => {
                    let n = c.u8()?;
                    d.frames = (0..n)
                        .map(|_| {
                            Ok(SeqFrame {
                                frame: c.u16()?,
                                secondary: opt_u16(c)?,
                                delay: c.u16()?,
                            })
                        })
                        .collect::<Result<_, IoError>>()?;
                }
                // The client stores this without mapping 65535 to -1.
                2 => d.loop_offset = Some(c.u16()?),
                3 => {
                    let n = c.u8()?;
                    d.interleave_labels = Some(c.bytes(n.into())?.to_vec());
                }
                4 => d.stretches = true,
                5 => d.priority = c.u8()?,
                6 => d.offhand_override = Some(c.u16()?),
                7 => d.mainhand_override = Some(c.u16()?),
                8 => d.max_loops = c.u8()?,
                9 => animating = Some(c.u8()?),
                10 => walking = Some(c.u8()?),
                11 => d.replay_mode = c.u8()?,
                12 => c.skip(4)?,
                _ => return Ok(false),
            }
            Ok(true)
        })?;
        // Without explicit precedences, interleaved sequences take 2.
        let default = if d.interleave_labels.is_some() { 2 } else { 0 };
        d.animating_precedence = animating.unwrap_or(default);
        d.walking_precedence = walking.unwrap_or(default);
        Ok(d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::synth::Ops;

    #[test]
    fn every_opcode() {
        let data = Ops::default()
            .op(1)
            .u8(2)
            .u16(100)
            .u16(65535)
            .u16(5)
            .u16(101)
            .u16(200)
            .u16(0)
            .op(2)
            .u16(1)
            .op(3)
            .u8(2)
            .u8(4)
            .u8(9)
            .op(4)
            .op(5)
            .u8(10)
            .op(6)
            .u16(1)
            .op(7)
            .u16(2)
            .op(8)
            .u8(1)
            .op(10)
            .u8(1)
            .op(11)
            .u8(0)
            .op(12)
            .i32(-1)
            .end();
        let d = Seq::decode(0, &mut Cursor::new(&data)).unwrap();
        assert_eq!(
            d.frames,
            [
                SeqFrame {
                    frame: 100,
                    secondary: None,
                    delay: 5
                },
                SeqFrame {
                    frame: 101,
                    secondary: Some(200),
                    delay: 0
                },
            ]
        );
        assert_eq!(d.loop_offset, Some(1));
        assert_eq!(d.interleave_labels.as_deref(), Some(&[4, 9][..]));
        assert!(d.stretches);
        assert_eq!((d.priority, d.max_loops, d.replay_mode), (10, 1, 0));
        assert_eq!(
            (d.offhand_override, d.mainhand_override),
            (Some(1), Some(2))
        );
        assert_eq!((d.animating_precedence, d.walking_precedence), (2, 1));
        assert!(d.unknown_opcodes.is_empty());
    }

    #[test]
    fn defaults() {
        let d = Seq::decode(0, &mut Cursor::new(&[0])).unwrap();
        assert!(d.frames.is_empty());
        assert_eq!((d.priority, d.max_loops, d.replay_mode), (5, 99, 2));
        assert_eq!((d.animating_precedence, d.walking_precedence), (0, 0));
    }
}
