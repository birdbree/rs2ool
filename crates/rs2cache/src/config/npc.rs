//! `npc.dat`.

use crate::types::config::NpcDef;

use super::{ConfigType, action, morph, opcodes, recolours};
use crate::io::{Cursor, IoError};

pub struct Npc;

impl ConfigType for Npc {
    type Def = NpcDef;
    const KIND: &'static str = "npc";
    const DAT: &'static str = "npc.dat";
    const IDX: Option<&'static str> = Some("npc.idx");

    fn decode(id: u32, c: &mut Cursor) -> Result<NpcDef, IoError> {
        let mut d = NpcDef {
            id,
            models: Vec::new(),
            name: None,
            examine: None,
            size: 1,
            stand_anim: None,
            walk_anim: None,
            walk_back_anim: None,
            walk_left_anim: None,
            walk_right_anim: None,
            actions: Default::default(),
            recolours: Vec::new(),
            chat_head_models: Vec::new(),
            draw_minimap_dot: true,
            combat_level: None,
            scale_xz: 128,
            scale_y: 128,
            visible: false,
            ambient: 0,
            contrast: 0,
            head_icon: None,
            turn_speed: 32,
            morph: None,
            clickable: true,
            unknown_opcodes: Vec::new(),
        };
        d.unknown_opcodes = opcodes(Npc::KIND, id, c, |op, c| {
            match op {
                1 => {
                    let n = c.u8()?;
                    d.models = (0..n).map(|_| c.u16()).collect::<Result<_, _>>()?;
                }
                2 => d.name = Some(c.string()?),
                3 => d.examine = Some(c.string()?),
                12 => d.size = c.i8()?,
                13 => d.stand_anim = Some(c.u16()?),
                14 => d.walk_anim = Some(c.u16()?),
                17 => {
                    d.walk_anim = Some(c.u16()?);
                    d.walk_back_anim = Some(c.u16()?);
                    d.walk_left_anim = Some(c.u16()?);
                    d.walk_right_anim = Some(c.u16()?);
                }
                30..=34 => d.actions[usize::from(op - 30)] = action(c)?,
                40 => d.recolours = recolours(c)?,
                60 => {
                    let n = c.u8()?;
                    d.chat_head_models = (0..n).map(|_| c.u16()).collect::<Result<_, _>>()?;
                }
                90..=92 => c.skip(2)?,
                93 => d.draw_minimap_dot = false,
                95 => d.combat_level = Some(c.u16()?),
                97 => d.scale_xz = c.u16()?,
                98 => d.scale_y = c.u16()?,
                99 => d.visible = true,
                100 => d.ambient = c.i8()?,
                101 => d.contrast = i16::from(c.i8()?) * 5,
                102 => d.head_icon = Some(c.u16()?),
                103 => d.turn_speed = c.u16()?,
                106 => d.morph = Some(morph(c)?),
                107 => d.clickable = false,
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
            .u8(2)
            .u16(215)
            .u16(246)
            .op(2)
            .str("Man")
            .op(3)
            .str("One of many citizens.")
            .op(12)
            .u8(2)
            .op(13)
            .u16(808)
            .op(14)
            .u16(819)
            .op(17)
            .u16(819)
            .u16(820)
            .u16(821)
            .u16(822)
            .op(30)
            .str("Talk-to")
            .op(32)
            .str("Hidden")
            .op(40)
            .u8(1)
            .u16(1)
            .u16(2)
            .op(60)
            .u8(1)
            .u16(50)
            .op(90)
            .u16(1)
            .op(91)
            .u16(2)
            .op(92)
            .u16(3)
            .op(93)
            .op(95)
            .u16(2)
            .op(97)
            .u16(100)
            .op(98)
            .u16(110)
            .op(99)
            .op(100)
            .u8(10)
            .op(101)
            .u8(0xff)
            .op(102)
            .u16(1)
            .op(103)
            .u16(64)
            .op(106)
            .u16(5)
            .u16(65535)
            .u8(0)
            .u16(2)
            .op(107)
            .end();
        let d = Npc::decode(1, &mut Cursor::new(&data)).unwrap();
        assert_eq!(d.models, [215, 246]);
        assert_eq!(d.name.as_deref(), Some("Man"));
        assert_eq!(d.size, 2);
        assert_eq!((d.stand_anim, d.walk_anim), (Some(808), Some(819)));
        assert_eq!(
            (d.walk_back_anim, d.walk_left_anim, d.walk_right_anim),
            (Some(820), Some(821), Some(822))
        );
        assert_eq!(d.actions[0].as_deref(), Some("Talk-to"));
        assert_eq!(d.actions[2], None);
        assert_eq!(d.recolours.len(), 1);
        assert_eq!(d.chat_head_models, [50]);
        assert!(!d.draw_minimap_dot && d.visible && !d.clickable);
        assert_eq!(d.combat_level, Some(2));
        assert_eq!((d.scale_xz, d.scale_y), (100, 110));
        assert_eq!((d.ambient, d.contrast), (10, -5));
        assert_eq!((d.head_icon, d.turn_speed), (Some(1), 64));
        assert_eq!(d.morph.unwrap().children, [Some(2)]);
        assert!(d.unknown_opcodes.is_empty());
    }

    #[test]
    fn unknown_opcode_is_logged_and_skipped() {
        let data = Ops::default().op(200).op(95).u16(3).end();
        let d = Npc::decode(0, &mut Cursor::new(&data)).unwrap();
        assert_eq!(d.unknown_opcodes, [200]);
        assert_eq!(d.combat_level, Some(3));
    }
}
