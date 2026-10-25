//! `spotanim.dat`: graphic effects.

use crate::types::config::SpotAnimDef;

use super::{ConfigType, opcodes, paired_recolours};
use crate::io::{Cursor, IoError};

/// [`ConfigType`] for `spotanim.dat` (graphic effects).
pub struct SpotAnim;

impl ConfigType for SpotAnim {
    type Def = SpotAnimDef;
    const KIND: &'static str = "spotanim";
    const DAT: &'static str = "spotanim.dat";
    const IDX: Option<&'static str> = None;

    fn decode(id: u32, c: &mut Cursor) -> Result<SpotAnimDef, IoError> {
        let mut d = SpotAnimDef {
            id,
            model: None,
            sequence: None,
            scale_xy: 128,
            scale_z: 128,
            rotation: 0,
            ambient: 0,
            contrast: 0,
            recolours: Vec::new(),
            unknown_opcodes: Vec::new(),
        };
        // Six recolour slots each, paired up once the stream ends.
        let (mut from, mut to) = ([0u16; 6], [0u16; 6]);
        d.unknown_opcodes = opcodes(SpotAnim::KIND, id, c, |op, c| {
            match op {
                // References keep 65535 as an id, as the original client does.
                1 => d.model = Some(c.u16()?),
                2 => d.sequence = Some(c.u16()?),
                4 => d.scale_xy = c.u16()?,
                5 => d.scale_z = c.u16()?,
                6 => d.rotation = c.u16()?,
                7 => d.ambient = c.u8()?,
                8 => d.contrast = c.u8()?,
                40..=45 => from[usize::from(op - 40)] = c.u16()?,
                50..=55 => to[usize::from(op - 50)] = c.u16()?,
                // Inside the original client's opcode ranges but past its
                // six slots: read the u16 to stay aligned, then report it.
                46..=49 | 56..=59 => {
                    c.skip(2)?;
                    return Ok(false);
                }
                _ => return Ok(false),
            }
            Ok(true)
        })?;
        d.recolours = paired_recolours(&from, &to);
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
            .u16(2000)
            .op(2)
            .u16(80)
            .op(4)
            .u16(64)
            .op(5)
            .u16(32)
            .op(6)
            .u16(90)
            .op(7)
            .u8(30)
            .op(8)
            .u8(40)
            .op(40)
            .u16(5)
            .op(50)
            .u16(6)
            .end();
        let d = SpotAnim::decode(0, &mut Cursor::new(&data)).unwrap();
        assert_eq!((d.model, d.sequence), (Some(2000), Some(80)));
        assert_eq!((d.scale_xy, d.scale_z, d.rotation), (64, 32, 90));
        assert_eq!((d.ambient, d.contrast), (30, 40));
        assert_eq!(d.recolours.len(), 1);
    }

    #[test]
    fn references_keep_65535() {
        let data = Ops::default().op(1).u16(65535).op(2).u16(65535).end();
        let d = SpotAnim::decode(0, &mut Cursor::new(&data)).unwrap();
        assert_eq!((d.model, d.sequence), (Some(65535), Some(65535)));
    }
}
