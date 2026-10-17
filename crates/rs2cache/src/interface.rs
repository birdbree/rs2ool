//! Interface decoder (interface archive, entry `data`).
//!
//! `count u16`, then components back to back until the data ends; the count
//! isn't used to stop. Each starts with `id u16`, except that 65535 in that
//! spot introduces a new root: `parent u16, id u16`. The parent holds for
//! every component after it until the next marker.
//!
//! ```text
//! type          u8       selects the type-specific block
//! actionType    u8
//! contentType   u16
//! width         u16
//! height        u16
//! alpha         u8
//! hoverPopup    opt16
//! conditions    count u8, then {op u8, value u16} × count
//! scripts       count u8, then {len u16, u16 × len} × count
//! type block    see Content
//! spell block   string, string, u16      if actionType == 2 or type == 2
//! text          string                   type 8 only, here rather than above
//! tooltip       string                   if actionType is 1, 4, 5 or 6
//! ```
//!
//! `opt16` is one byte `hi`: 0 means none, otherwise one more byte `lo`
//! follows and the value is `(hi − 1) << 8 | lo`. Flags are bytes where only
//! 1 is true. This is the same order the original client reads in.

use crate::types::interface::{Child, Condition, Content, Interface, SpriteRef, TextStyle};
use thiserror::Error;

use crate::io::{Cursor, IoError};

/// The interface data ran out in the middle of a component.
#[derive(Debug, Error)]
#[error("interface {id:?} at offset {offset}: {source}")]
pub struct InterfaceError {
    /// The component being read, once its id is known.
    pub id: Option<u16>,
    /// Where that component started.
    pub offset: usize,
    /// The failed read.
    pub source: IoError,
}

/// `(hi − 1) << 8 | lo` when the first byte is non-zero, else none.
fn opt_packed(c: &mut Cursor) -> Result<Option<u16>, IoError> {
    let hi = c.u8()?;
    if hi == 0 {
        return Ok(None);
    }
    Ok(Some((u16::from(hi) - 1) << 8 | u16::from(c.u8()?)))
}

// `name,index`, split at the last comma. Anything that doesn't parse (an
// empty string included) is no sprite.
fn sprite_ref(s: String) -> Option<SpriteRef> {
    let (name, index) = s.rsplit_once(',')?;
    Some(SpriteRef {
        name: name.to_owned(),
        index: index.parse().ok()?,
    })
}

// A string where empty means none.
fn opt_string(c: &mut Cursor) -> Result<Option<String>, IoError> {
    let s = c.string()?;
    Ok((!s.is_empty()).then_some(s))
}

fn actions(c: &mut Cursor) -> Result<[Option<String>; 5], IoError> {
    let mut out: [Option<String>; 5] = Default::default();
    for a in &mut out {
        *a = opt_string(c)?;
    }
    Ok(out)
}

fn style(c: &mut Cursor) -> Result<TextStyle, IoError> {
    Ok(TextStyle {
        centered: c.u8()? == 1,
        font: c.u8()?,
        shadow: c.u8()? == 1,
    })
}

fn component(c: &mut Cursor, id: u16, parent: Option<u16>) -> Result<Interface, IoError> {
    let kind = c.u8()?;
    let action_type = c.u8()?;
    let content_type = c.u16()?;
    let width = c.u16()?;
    let height = c.u16()?;
    let alpha = c.u8()?;
    let hover_popup = opt_packed(c)?;
    // Conditions and scripts pair up by position.
    let n = c.u8()?;
    let mut ops = Vec::with_capacity(n.into());
    for _ in 0..n {
        ops.push(Condition {
            op: c.u8()?,
            value: c.u16()?,
        });
    }
    let n = c.u8()?;
    let mut scripts = Vec::with_capacity(n.into());
    for _ in 0..n {
        let len = c.u16()?;
        scripts.push((0..len).map(|_| c.u16()).collect::<Result<_, _>>()?);
    }
    // Enabled, hover and enabled-hover colours, in that order.
    let colours =
        |c: &mut Cursor| -> Result<[i32; 3], IoError> { Ok([c.i32()?, c.i32()?, c.i32()?]) };
    let content = match kind {
        0 => {
            let scroll_max = c.u16()?;
            let hidden = c.u8()? == 1;
            let n = c.u16()?;
            let children = (0..n)
                .map(|_| {
                    Ok(Child {
                        id: c.u16()?,
                        x: c.i16()?,
                        y: c.i16()?,
                    })
                })
                .collect::<Result<_, IoError>>()?;
            Content::Layer {
                scroll_max,
                hidden,
                children,
            }
        }
        1 => {
            let a = c.u16()?;
            let b = c.u8()?;
            let style = style(c)?;
            Content::Unused {
                a,
                b,
                style,
                colour: c.i32()?,
            }
        }
        2 => {
            let swappable = c.u8()? == 1;
            let is_inventory = c.u8()? == 1;
            let usable = c.u8()? == 1;
            let replaceable = c.u8()? == 1;
            let pad_x = c.u8()?;
            let pad_y = c.u8()?;
            // Always 20 entries, each led by a present flag.
            let mut slot_sprites = Vec::with_capacity(20);
            for _ in 0..20 {
                slot_sprites.push(if c.u8()? == 1 {
                    let (x, y) = (c.i16()?, c.i16()?);
                    Some((x, y, sprite_ref(c.string()?)))
                } else {
                    None
                });
            }
            Content::Inventory {
                swappable,
                is_inventory,
                usable,
                replaceable,
                pad_x,
                pad_y,
                slot_sprites,
                actions: actions(c)?,
            }
        }
        3 => {
            let filled = c.u8()? == 1;
            let colour = c.i32()?;
            let [enabled_colour, hover_colour, enabled_hover_colour] = colours(c)?;
            Content::Rectangle {
                filled,
                colour,
                enabled_colour,
                hover_colour,
                enabled_hover_colour,
            }
        }
        4 => {
            let style = style(c)?;
            let text = c.string()?;
            let enabled_text = c.string()?;
            let colour = c.i32()?;
            let [enabled_colour, hover_colour, enabled_hover_colour] = colours(c)?;
            Content::Text {
                style,
                text,
                enabled_text,
                colour,
                enabled_colour,
                hover_colour,
                enabled_hover_colour,
            }
        }
        5 => Content::Sprite {
            sprite: sprite_ref(c.string()?),
            enabled_sprite: sprite_ref(c.string()?),
        },
        6 => Content::Model {
            model: opt_packed(c)?,
            enabled_model: opt_packed(c)?,
            animation: opt_packed(c)?,
            enabled_animation: opt_packed(c)?,
            zoom: c.u16()?,
            rotation_x: c.u16()?,
            rotation_y: c.u16()?,
        },
        7 => Content::ItemText {
            style: style(c)?,
            colour: c.i32()?,
            pad_x: c.i16()?,
            pad_y: c.i16()?,
            is_inventory: c.u8()? == 1,
            actions: actions(c)?,
        },
        // Type 8's text comes after the spell block below.
        _ => Content::Other,
    };
    let (mut selected_action, mut spell_name, mut spell_usable_on) = (None, None, None);
    if action_type == 2 || kind == 2 {
        selected_action = Some(c.string()?);
        spell_name = Some(c.string()?);
        spell_usable_on = Some(c.u16()?);
    }
    let content = if kind == 8 {
        Content::Tooltip { text: c.string()? }
    } else {
        content
    };
    let tooltip = if matches!(action_type, 1 | 4 | 5 | 6) {
        let t = c.string()?;
        Some(if t.is_empty() {
            match action_type {
                1 => "Ok",
                6 => "Continue",
                _ => "Select",
            }
            .to_owned()
        } else {
            t
        })
    } else {
        None
    };
    Ok(Interface {
        id,
        parent,
        kind,
        action_type,
        content_type,
        width,
        height,
        alpha,
        hover_popup,
        conditions: ops,
        scripts,
        content,
        selected_action,
        spell_name,
        spell_usable_on,
        tooltip,
    })
}

/// Decode every interface component. Returns the declared count (the size
/// of the original client's id table) and the components in file order.
pub fn decode(data: &[u8]) -> Result<(u16, Vec<Interface>), InterfaceError> {
    let mut c = Cursor::new(data);
    let err = |id, offset| move |source| InterfaceError { id, offset, source };
    let count = c.u16().map_err(err(None, 0))?;
    let mut parent = None;
    let mut out = Vec::new();
    while c.remaining() > 0 {
        let start = c.pos();
        let mut id = c.u16().map_err(err(None, start))?;
        // 65535 is a root marker, not an id.
        if id == u16::MAX {
            parent = Some(c.u16().map_err(err(None, start))?);
            id = c.u16().map_err(err(None, start))?;
        }
        out.push(component(&mut c, id, parent).map_err(err(Some(id), start))?);
    }
    Ok((count, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(out: &mut Vec<u8>, kind: u8, action: u8) {
        out.extend([kind, action]);
        out.extend([0, 0, 0, 10, 0, 20, 0]);
        // Hover popup (2-1)<<8 | 5, one condition, one 2-word script.
        out.extend([2, 5]);
        out.extend([1, 3, 0, 9]);
        out.extend([1, 0, 2, 0, 1, 0, 0]);
    }

    #[test]
    fn decodes_a_layer_and_children() {
        let mut d = vec![0, 3];
        d.extend([0xff, 0xff, 0, 7, 0, 7]);
        header(&mut d, 0, 0);
        d.extend([0, 0, 0, 0, 2, 0, 8, 0, 5, 0xff, 0xfb, 0, 9, 0, 0, 0, 0]);
        d.extend([0, 8]);
        header(&mut d, 5, 1);
        d.extend(b"sheet,0\n\n");
        d.extend(b"\n");
        d.extend([0, 9]);
        header(&mut d, 8, 0);
        d.extend(b"tip\n");
        let (count, list) = decode(&d).unwrap();
        assert_eq!(count, 3);
        assert_eq!(list.len(), 3);
        let layer = &list[0];
        assert_eq!(
            (layer.id, layer.parent, layer.width, layer.height),
            (7, Some(7), 10, 20)
        );
        assert_eq!(layer.hover_popup, Some(261));
        assert_eq!(layer.conditions, [Condition { op: 3, value: 9 }]);
        assert_eq!(layer.scripts, [vec![1, 0]]);
        match &layer.content {
            Content::Layer { children, .. } => {
                assert_eq!(
                    children,
                    &[Child { id: 8, x: 5, y: -5 }, Child { id: 9, x: 0, y: 0 }]
                );
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(list[1].parent, Some(7));
        assert_eq!(
            list[1].content,
            Content::Sprite {
                sprite: Some(SpriteRef {
                    name: "sheet".into(),
                    index: 0
                }),
                enabled_sprite: None
            }
        );
        assert_eq!(list[1].tooltip.as_deref(), Some("Ok"));
        assert_eq!(list[2].content, Content::Tooltip { text: "tip".into() });
    }

    #[test]
    fn truncation_is_an_error() {
        let mut d = vec![0, 1, 0, 1];
        header(&mut d, 0, 0);
        assert!(decode(&d).is_err());
    }
}
