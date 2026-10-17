//! `loc.dat`: world objects.

use rs2intermediate::config::{LocDef, LocModel};

use super::{ConfigType, action, morph, opcodes, opt_u16, recolours};
use crate::io::{Cursor, IoError};

pub struct Loc;

impl ConfigType for Loc {
    type Def = LocDef;
    const KIND: &'static str = "loc";
    const DAT: &'static str = "loc.dat";
    const IDX: Option<&'static str> = Some("loc.idx");

    fn decode(id: u32, c: &mut Cursor) -> Result<LocDef, IoError> {
        let mut d = LocDef {
            id,
            models: Vec::new(),
            name: None,
            examine: None,
            size_x: 1,
            size_y: 1,
            solid: true,
            impenetrable: true,
            has_actions: false,
            adjust_to_terrain: false,
            flat_shading: false,
            occludes: false,
            animation: None,
            decor_displacement: 16,
            ambient: 0,
            contrast: 0,
            actions: Default::default(),
            recolours: Vec::new(),
            minimap_function: None,
            mirrored: false,
            casts_shadow: true,
            scale_x: 128,
            scale_y: 128,
            scale_z: 128,
            mapscene: None,
            blocked_directions: 0,
            offset_x: 0,
            offset_y: 0,
            offset_z: 0,
            obstructs_ground: false,
            hollow: false,
            support_items: 0,
            morph: None,
            unknown_opcodes: Vec::new(),
        };
        let mut interactive: Option<u8> = None;
        let mut support_items: Option<u8> = None;
        let mut has_action_strings = false;
        d.unknown_opcodes = opcodes(Loc::KIND, id, c, |op, c| {
            match op {
                1 => {
                    let n = c.u8()?;
                    d.models = (0..n)
                        .map(|_| {
                            Ok(LocModel {
                                model: c.u16()?,
                                kind: Some(c.u8()?),
                            })
                        })
                        .collect::<Result<_, IoError>>()?;
                }
                2 => d.name = Some(c.string()?),
                3 => d.examine = Some(c.string()?),
                5 => {
                    let n = c.u8()?;
                    d.models = (0..n)
                        .map(|_| {
                            Ok(LocModel {
                                model: c.u16()?,
                                kind: None,
                            })
                        })
                        .collect::<Result<_, IoError>>()?;
                }
                14 => d.size_x = c.u8()?,
                15 => d.size_y = c.u8()?,
                17 => d.solid = false,
                18 => d.impenetrable = false,
                19 => interactive = Some(c.u8()?),
                21 => d.adjust_to_terrain = true,
                22 => d.flat_shading = true,
                23 => d.occludes = true,
                24 => d.animation = opt_u16(c)?,
                28 => d.decor_displacement = c.u8()?,
                29 => d.ambient = c.i8()?,
                39 => d.contrast = c.i8()?,
                30..=34 => {
                    has_action_strings = true;
                    d.actions[usize::from(op - 30)] = action(c)?;
                }
                40 => d.recolours = recolours(c)?,
                60 => d.minimap_function = Some(c.u16()?),
                62 => d.mirrored = true,
                64 => d.casts_shadow = false,
                65 => d.scale_x = c.u16()?,
                66 => d.scale_y = c.u16()?,
                67 => d.scale_z = c.u16()?,
                68 => d.mapscene = Some(c.u16()?),
                69 => d.blocked_directions = c.u8()?,
                70 => d.offset_x = c.i16()?,
                71 => d.offset_y = c.i16()?,
                72 => d.offset_z = c.i16()?,
                73 => d.obstructs_ground = true,
                74 => d.hollow = true,
                75 => support_items = Some(c.u8()?),
                77 => d.morph = Some(morph(c)?),
                _ => return Ok(false),
            }
            Ok(true)
        })?;
        // The client's post-decode derivations.
        d.has_actions = match interactive {
            Some(v) => v == 1,
            None => {
                let typed_as_10 = d
                    .models
                    .first()
                    .is_some_and(|m| m.kind.is_none_or(|k| k == 10));
                typed_as_10 || has_action_strings
            }
        };
        if d.hollow {
            d.solid = false;
            d.impenetrable = false;
        }
        d.support_items = support_items.unwrap_or(u8::from(d.solid));
        Ok(d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::synth::Ops;
    use rs2intermediate::config::Recolour;

    #[test]
    fn every_opcode() {
        let data = Ops::default()
            .op(1)
            .u8(2)
            .u16(100)
            .u8(0)
            .u16(101)
            .u8(10)
            .op(2)
            .str("Door")
            .op(3)
            .str("A door.")
            .op(14)
            .u8(2)
            .op(15)
            .u8(3)
            .op(17)
            .op(18)
            .op(19)
            .u8(1)
            .op(21)
            .op(22)
            .op(23)
            .op(24)
            .u16(65535)
            .op(28)
            .u8(8)
            .op(29)
            .u8(0xfb)
            .op(39)
            .u8(7)
            .op(30)
            .str("Open")
            .op(31)
            .str("hidden")
            .op(40)
            .u8(1)
            .u16(10)
            .u16(20)
            .op(60)
            .u16(5)
            .op(62)
            .op(64)
            .op(65)
            .u16(64)
            .op(66)
            .u16(96)
            .op(67)
            .u16(200)
            .op(68)
            .u16(9)
            .op(69)
            .u8(0b1010)
            .op(70)
            .u16(0xfff0)
            .op(71)
            .u16(4)
            .op(72)
            .u16(8)
            .op(73)
            .op(75)
            .u8(1)
            .op(77)
            .u16(65535)
            .u16(12)
            .u8(1)
            .u16(1500)
            .u16(65535)
            .end();
        let d = Loc::decode(7, &mut Cursor::new(&data)).unwrap();
        assert_eq!(
            d.models,
            [
                LocModel {
                    model: 100,
                    kind: Some(0)
                },
                LocModel {
                    model: 101,
                    kind: Some(10)
                }
            ]
        );
        assert_eq!(d.name.as_deref(), Some("Door"));
        assert_eq!(d.examine.as_deref(), Some("A door."));
        assert_eq!((d.size_x, d.size_y), (2, 3));
        assert!(!d.solid && !d.impenetrable && d.has_actions);
        assert!(d.adjust_to_terrain && d.flat_shading && d.occludes);
        assert_eq!(d.animation, None);
        assert_eq!((d.decor_displacement, d.ambient, d.contrast), (8, -5, 7));
        assert_eq!(d.actions[0].as_deref(), Some("Open"));
        assert_eq!(d.actions[1], None);
        assert_eq!(d.recolours, [Recolour { from: 10, to: 20 }]);
        assert_eq!(d.minimap_function, Some(5));
        assert!(d.mirrored && !d.casts_shadow);
        assert_eq!((d.scale_x, d.scale_y, d.scale_z), (64, 96, 200));
        assert_eq!(d.mapscene, Some(9));
        assert_eq!(d.blocked_directions, 0b1010);
        assert_eq!((d.offset_x, d.offset_y, d.offset_z), (-16, 4, 8));
        assert!(d.obstructs_ground && !d.hollow);
        assert_eq!(d.support_items, 1);
        let m = d.morph.unwrap();
        assert_eq!((m.varbit, m.varp), (None, Some(12)));
        assert_eq!(m.children, [Some(1500), None]);
        assert!(d.unknown_opcodes.is_empty());
    }

    #[test]
    fn derived_fields() {
        // Untyped models count as type 10 and make the object interactive;
        // hollow clears solidity; support_items follows solidity.
        let data = Ops::default().op(5).u8(1).u16(3).op(74).end();
        let d = Loc::decode(0, &mut Cursor::new(&data)).unwrap();
        assert!(d.has_actions);
        assert!(!d.solid && !d.impenetrable);
        assert_eq!(d.support_items, 0);

        let data = Ops::default().op(1).u8(1).u16(3).u8(0).end();
        let d = Loc::decode(0, &mut Cursor::new(&data)).unwrap();
        assert!(!d.has_actions);
        assert_eq!(d.support_items, 1);
    }

    #[test]
    fn unknown_opcode_is_logged_and_skipped() {
        let data = Ops::default().op(99).op(14).u8(4).end();
        let d = Loc::decode(0, &mut Cursor::new(&data)).unwrap();
        assert_eq!(d.unknown_opcodes, [99]);
        assert_eq!(d.size_x, 4);
    }

    #[test]
    fn truncation_is_an_error() {
        let data = Ops::default().op(2).str("x").0;
        assert!(Loc::decode(0, &mut Cursor::new(&data)).is_err());
        assert!(Loc::decode(0, &mut Cursor::new(&[1, 2, 0])).is_err());
    }
}
