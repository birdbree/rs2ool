//! `varbit.dat`: bit ranges of player variables.

use crate::types::config::VarbitDef;

use super::{ConfigType, opcodes};
use crate::io::{Cursor, IoError};

pub struct Varbit;

impl ConfigType for Varbit {
    type Def = VarbitDef;
    const KIND: &'static str = "varbit";
    const DAT: &'static str = "varbit.dat";
    const IDX: Option<&'static str> = None;

    fn decode(id: u32, c: &mut Cursor) -> Result<VarbitDef, IoError> {
        let mut d = VarbitDef {
            id,
            varp: 0,
            lsb: 0,
            msb: 0,
            flags_varp: false,
            op3: None,
            op4: None,
            op10: None,
            unknown_opcodes: Vec::new(),
        };
        d.unknown_opcodes = opcodes(Varbit::KIND, id, c, |op, c| {
            match op {
                1 => {
                    d.varp = c.u16()?;
                    d.lsb = c.u8()?;
                    d.msb = c.u8()?;
                }
                2 => d.flags_varp = true,
                3 => d.op3 = Some(c.i32()?),
                4 => d.op4 = Some(c.i32()?),
                10 => d.op10 = Some(c.string()?),
                _ => return Ok(false),
            }
            Ok(true)
        })?;
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
            .u16(300)
            .u8(4)
            .u8(7)
            .op(2)
            .op(3)
            .i32(1)
            .op(4)
            .i32(2)
            .op(10)
            .str("name")
            .end();
        let d = Varbit::decode(0, &mut Cursor::new(&data)).unwrap();
        assert_eq!((d.varp, d.lsb, d.msb), (300, 4, 7));
        assert!(d.flags_varp);
        assert_eq!((d.op3, d.op4), (Some(1), Some(2)));
        assert_eq!(d.op10.as_deref(), Some("name"));
    }
}
