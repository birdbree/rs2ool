//! `flo.dat`: floor (underlay and overlay) types.

use rs2intermediate::config::FloDef;

use super::{ConfigType, opcodes};
use crate::io::{Cursor, IoError};

pub struct Flo;

impl ConfigType for Flo {
    type Def = FloDef;
    const KIND: &'static str = "flo";
    const DAT: &'static str = "flo.dat";
    const IDX: Option<&'static str> = None;

    fn decode(id: u32, c: &mut Cursor) -> Result<FloDef, IoError> {
        let mut d = FloDef {
            id,
            rgb: None,
            texture: None,
            hide_underlay: false,
            occlude: true,
            name: None,
            secondary_rgb: None,
            unknown_opcodes: Vec::new(),
        };
        d.unknown_opcodes = opcodes(Flo::KIND, id, c, |op, c| {
            match op {
                1 => d.rgb = Some(c.u24()?),
                2 => d.texture = Some(c.u8()?),
                3 => d.hide_underlay = true,
                5 => d.occlude = false,
                6 => d.name = Some(c.string()?),
                7 => d.secondary_rgb = Some(c.u24()?),
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
            .u24(0x35_6a_1f)
            .op(2)
            .u8(24)
            .op(3)
            .op(5)
            .op(6)
            .str("grass")
            .op(7)
            .u24(0x10_20_30)
            .end();
        let d = Flo::decode(0, &mut Cursor::new(&data)).unwrap();
        assert_eq!(d.rgb, Some(0x35_6a_1f));
        assert_eq!(d.texture, Some(24));
        assert!(d.hide_underlay && !d.occlude);
        assert_eq!(d.name.as_deref(), Some("grass"));
        assert_eq!(d.secondary_rgb, Some(0x10_20_30));
    }
}
