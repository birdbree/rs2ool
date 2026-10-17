//! `idk.dat`: identity-kit parts (player body parts).

use crate::types::config::IdkDef;

use super::{ConfigType, opcodes, paired_recolours};
use crate::io::{Cursor, IoError};

/// [`ConfigType`] for `idk.dat` (identity-kit parts).
pub struct Idk;

impl ConfigType for Idk {
    type Def = IdkDef;
    const KIND: &'static str = "idk";
    const DAT: &'static str = "idk.dat";
    const IDX: Option<&'static str> = None;

    fn decode(id: u32, c: &mut Cursor) -> Result<IdkDef, IoError> {
        let mut d = IdkDef {
            id,
            body_part: None,
            models: Vec::new(),
            selectable: true,
            recolours: Vec::new(),
            chat_head_models: [None; 5],
            unknown_opcodes: Vec::new(),
        };
        // The original client's recolour arrays have six slots each.
        let (mut from, mut to) = ([0u16; 6], [0u16; 6]);
        d.unknown_opcodes = opcodes(Idk::KIND, id, c, |op, c| {
            match op {
                1 => d.body_part = Some(c.u8()?),
                2 => {
                    let n = c.u8()?;
                    d.models = (0..n).map(|_| c.u16()).collect::<Result<_, _>>()?;
                }
                3 => d.selectable = false,
                40..=45 => from[usize::from(op - 40)] = c.u16()?,
                50..=55 => to[usize::from(op - 50)] = c.u16()?,
                60..=64 => d.chat_head_models[usize::from(op - 60)] = Some(c.u16()?),
                // In range for the original client's opcode test but past
                // the end of its arrays; the value is still a u16.
                46..=49 | 56..=59 | 65..=69 => {
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
    use crate::types::config::Recolour;

    #[test]
    fn every_opcode() {
        let data = Ops::default()
            .op(1)
            .u8(7)
            .op(2)
            .u8(2)
            .u16(270)
            .u16(271)
            .op(3)
            .op(40)
            .u16(6798)
            .op(50)
            .u16(8741)
            .op(41)
            .u16(0)
            .op(51)
            .u16(9)
            .op(60)
            .u16(57)
            .op(64)
            .u16(58)
            .op(47)
            .u16(1)
            .end();
        let d = Idk::decode(3, &mut Cursor::new(&data)).unwrap();
        assert_eq!(d.body_part, Some(7));
        assert_eq!(d.models, [270, 271]);
        assert!(!d.selectable);
        assert_eq!(
            d.recolours,
            [Recolour {
                from: 6798,
                to: 8741
            }]
        );
        assert_eq!(d.chat_head_models, [Some(57), None, None, None, Some(58)]);
        assert_eq!(d.unknown_opcodes, [47]);
    }
}
