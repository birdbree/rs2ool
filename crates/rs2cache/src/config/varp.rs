//! `varp.dat`: player variables.

use rs2intermediate::config::VarpDef;

use super::{ConfigType, opcodes};
use crate::io::{Cursor, IoError};

pub struct Varp;

impl ConfigType for Varp {
    type Def = VarpDef;
    const KIND: &'static str = "varp";
    const DAT: &'static str = "varp.dat";
    const IDX: Option<&'static str> = None;

    fn decode(id: u32, c: &mut Cursor) -> Result<VarpDef, IoError> {
        let mut d = VarpDef {
            id,
            client_code: None,
            op1: None,
            op2: None,
            op3: false,
            op4: false,
            op6: false,
            op7: None,
            flagged: false,
            op10: None,
            op12: None,
            op13: false,
            unknown_opcodes: Vec::new(),
        };
        d.unknown_opcodes = opcodes(Varp::KIND, id, c, |op, c| {
            match op {
                1 => d.op1 = Some(c.u8()?),
                2 => d.op2 = Some(c.u8()?),
                3 => d.op3 = true,
                4 => d.op4 = true,
                5 => d.client_code = Some(c.u16()?),
                6 => d.op6 = true,
                7 => d.op7 = Some(c.i32()?),
                8 | 11 => d.flagged = true,
                10 => d.op10 = Some(c.string()?),
                12 => d.op12 = Some(c.i32()?),
                13 => d.op13 = true,
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
            .u8(1)
            .op(2)
            .u8(2)
            .op(3)
            .op(4)
            .op(5)
            .u16(9)
            .op(6)
            .op(7)
            .i32(-1)
            .op(8)
            .op(10)
            .str("x")
            .op(12)
            .i32(5)
            .op(13)
            .end();
        let d = Varp::decode(0, &mut Cursor::new(&data)).unwrap();
        assert_eq!((d.op1, d.op2, d.client_code), (Some(1), Some(2), Some(9)));
        assert!(d.op3 && d.op4 && d.op6 && d.flagged && d.op13);
        assert_eq!((d.op7, d.op12), (Some(-1), Some(5)));
        assert_eq!(d.op10.as_deref(), Some("x"));
        assert!(d.unknown_opcodes.is_empty());
    }
}
