//! `obj.dat`: items.

use crate::types::config::{ObjDef, StackVariant};

use super::{ConfigType, action, opcodes, recolours};
use crate::io::{Cursor, IoError};

/// [`ConfigType`] for `obj.dat` (items).
pub struct Obj;

impl ConfigType for Obj {
    type Def = ObjDef;
    const KIND: &'static str = "obj";
    const DAT: &'static str = "obj.dat";
    const IDX: Option<&'static str> = Some("obj.idx");

    fn decode(id: u32, c: &mut Cursor) -> Result<ObjDef, IoError> {
        let mut d = ObjDef {
            id,
            inventory_model: 0,
            name: None,
            examine: None,
            icon_zoom: 2000,
            icon_pitch: 0,
            icon_yaw: 0,
            icon_roll: 0,
            icon_offset_x: 0,
            icon_offset_y: 0,
            stackable: false,
            value: 1,
            members: false,
            male_equip1: None,
            male_offset: 0,
            male_equip2: None,
            male_equip3: None,
            female_equip1: None,
            female_offset: 0,
            female_equip2: None,
            female_equip3: None,
            male_chat_head1: None,
            male_chat_head2: None,
            female_chat_head1: None,
            female_chat_head2: None,
            ground_actions: Default::default(),
            inventory_actions: Default::default(),
            recolours: Vec::new(),
            noted_id: None,
            noted_template: None,
            stack_variants: Vec::new(),
            scale_x: 128,
            scale_y: 128,
            scale_z: 128,
            ambient: 0,
            contrast: 0,
            team: 0,
            unknown_opcodes: Vec::new(),
        };
        d.unknown_opcodes = opcodes(Obj::KIND, id, c, |op, c| {
            match op {
                1 => d.inventory_model = c.u16()?,
                2 => d.name = Some(c.string()?),
                3 => d.examine = Some(c.string()?),
                4 => d.icon_zoom = c.u16()?,
                5 => d.icon_pitch = c.u16()?,
                6 => d.icon_yaw = c.u16()?,
                7 => d.icon_offset_x = c.i16()?,
                8 => d.icon_offset_y = c.i16()?,
                // Read and thrown away.
                10 => c.skip(2)?,
                11 => d.stackable = true,
                12 => d.value = c.i32()?,
                16 => d.members = true,
                23 => {
                    d.male_equip1 = Some(c.u16()?);
                    d.male_offset = c.i8()?;
                }
                24 => d.male_equip2 = Some(c.u16()?),
                25 => {
                    d.female_equip1 = Some(c.u16()?);
                    d.female_offset = c.i8()?;
                }
                26 => d.female_equip2 = Some(c.u16()?),
                30..=34 => d.ground_actions[usize::from(op - 30)] = action(c)?,
                35..=39 => d.inventory_actions[usize::from(op - 35)] = Some(c.string()?),
                40 => d.recolours = recolours(c)?,
                78 => d.male_equip3 = Some(c.u16()?),
                79 => d.female_equip3 = Some(c.u16()?),
                90 => d.male_chat_head1 = Some(c.u16()?),
                91 => d.female_chat_head1 = Some(c.u16()?),
                92 => d.male_chat_head2 = Some(c.u16()?),
                93 => d.female_chat_head2 = Some(c.u16()?),
                95 => d.icon_roll = c.u16()?,
                97 => d.noted_id = Some(c.u16()?),
                98 => d.noted_template = Some(c.u16()?),
                100..=109 => {
                    let slot = op - 100;
                    let v = StackVariant {
                        slot,
                        item: c.u16()?,
                        count: c.u16()?,
                    };
                    // A repeated slot replaces the earlier one; keep the list
                    // sorted by slot whatever order the opcodes come in.
                    d.stack_variants.retain(|s| s.slot != slot);
                    d.stack_variants.push(v);
                    d.stack_variants.sort_by_key(|s| s.slot);
                }
                110 => d.scale_x = c.u16()?,
                111 => d.scale_y = c.u16()?,
                112 => d.scale_z = c.u16()?,
                113 => d.ambient = c.i8()?,
                // Multiplied by 5, as the original client does.
                114 => d.contrast = i16::from(c.i8()?) * 5,
                115 => d.team = c.u8()?,
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
            .u16(2635)
            .op(2)
            .str("Item")
            .op(3)
            .str("A nice hat from a cracker.")
            .op(4)
            .u16(440)
            .op(5)
            .u16(76)
            .op(6)
            .u16(1852)
            .op(7)
            .u16(1)
            .op(8)
            .u16(0xfffe)
            .op(10)
            .u16(9)
            .op(11)
            .op(12)
            .i32(1)
            .op(16)
            .op(23)
            .u16(187)
            .u8(0xfa)
            .op(24)
            .u16(10)
            .op(25)
            .u16(363)
            .u8(4)
            .op(26)
            .u16(11)
            .op(30)
            .str("hidden")
            .op(33)
            .str("Take")
            .op(35)
            .str("Wear")
            .op(39)
            .str("hidden")
            .op(40)
            .u8(1)
            .u16(926)
            .u16(5)
            .op(78)
            .u16(12)
            .op(79)
            .u16(13)
            .op(90)
            .u16(20)
            .op(91)
            .u16(21)
            .op(92)
            .u16(22)
            .op(93)
            .u16(23)
            .op(95)
            .u16(30)
            .op(97)
            .u16(1039)
            .op(98)
            .u16(799)
            .op(101)
            .u16(500)
            .u16(2)
            .op(100)
            .u16(501)
            .u16(3)
            .op(110)
            .u16(1)
            .op(111)
            .u16(2)
            .op(112)
            .u16(3)
            .op(113)
            .u8(0x80)
            .op(114)
            .u8(2)
            .op(115)
            .u8(4)
            .end();
        let d = Obj::decode(1038, &mut Cursor::new(&data)).unwrap();
        assert_eq!(d.inventory_model, 2635);
        assert_eq!(d.name.as_deref(), Some("Item"));
        assert_eq!(
            (d.icon_zoom, d.icon_pitch, d.icon_yaw, d.icon_roll),
            (440, 76, 1852, 30)
        );
        assert_eq!((d.icon_offset_x, d.icon_offset_y), (1, -2));
        assert!(d.stackable && d.members);
        assert_eq!(d.value, 1);
        assert_eq!(
            (d.male_equip1, d.male_offset, d.male_equip2),
            (Some(187), -6, Some(10))
        );
        assert_eq!(
            (d.female_equip1, d.female_offset, d.female_equip2),
            (Some(363), 4, Some(11))
        );
        assert_eq!((d.male_equip3, d.female_equip3), (Some(12), Some(13)));
        assert_eq!(d.ground_actions[0], None);
        assert_eq!(d.ground_actions[3].as_deref(), Some("Take"));
        assert_eq!(d.inventory_actions[0].as_deref(), Some("Wear"));
        assert_eq!(d.inventory_actions[4].as_deref(), Some("hidden"));
        assert_eq!(
            (
                d.male_chat_head1,
                d.female_chat_head1,
                d.male_chat_head2,
                d.female_chat_head2
            ),
            (Some(20), Some(21), Some(22), Some(23))
        );
        assert_eq!((d.noted_id, d.noted_template), (Some(1039), Some(799)));
        assert_eq!(
            d.stack_variants
                .iter()
                .map(|s| (s.slot, s.item))
                .collect::<Vec<_>>(),
            [(0, 501), (1, 500)]
        );
        assert_eq!((d.scale_x, d.scale_y, d.scale_z), (1, 2, 3));
        assert_eq!((d.ambient, d.contrast, d.team), (-128, 10, 4));
        assert!(d.unknown_opcodes.is_empty());
    }
}
