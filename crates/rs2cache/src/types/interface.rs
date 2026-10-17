//! Interfaces from the interface archive's `data` entry.

use serde::{Deserialize, Serialize};

/// A child placed inside a layer, at an offset from the layer's corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Child {
    pub id: u16,
    pub x: i16,
    pub y: i16,
}

/// One enable condition: compare the result of script `index` using `op`
/// against `value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Condition {
    pub op: u8,
    pub value: u16,
}

/// A sprite reference `name,index` into the media archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpriteRef {
    pub name: String,
    pub index: u32,
}

/// Text style shared by text-bearing components.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextStyle {
    pub centered: bool,
    /// Font index: 0 p11, 1 p12, 2 b12, 3 q8.
    pub font: u8,
    pub shadow: bool,
}

/// The type-specific part of an interface component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Content {
    /// Type 0: a container of positioned children.
    Layer {
        scroll_max: u16,
        hidden: bool,
        children: Vec<Child>,
    },
    /// Type 1: unused in 317; its fields are read and kept.
    Unused {
        a: u16,
        b: u8,
        style: TextStyle,
        colour: i32,
    },
    /// Type 2: an inventory grid of `width × height` slots.
    Inventory {
        swappable: bool,
        is_inventory: bool,
        usable: bool,
        /// Client field `aBoolean235`.
        replaceable: bool,
        pad_x: u8,
        pad_y: u8,
        /// Up to 20 background sprites, with positions.
        slot_sprites: Vec<Option<(i16, i16, Option<SpriteRef>)>>,
        actions: [Option<String>; 5],
    },
    /// Type 3: a rectangle.
    Rectangle {
        filled: bool,
        colour: i32,
        enabled_colour: i32,
        hover_colour: i32,
        enabled_hover_colour: i32,
    },
    /// Type 4: text.
    Text {
        style: TextStyle,
        text: String,
        enabled_text: String,
        colour: i32,
        enabled_colour: i32,
        hover_colour: i32,
        enabled_hover_colour: i32,
    },
    /// Type 5: a sprite, with an alternative shown when enabled.
    Sprite {
        sprite: Option<SpriteRef>,
        enabled_sprite: Option<SpriteRef>,
    },
    /// Type 6: a 3D model.
    Model {
        /// Disabled model, when the type byte says it is a model id.
        model: Option<u16>,
        enabled_model: Option<u16>,
        animation: Option<u16>,
        enabled_animation: Option<u16>,
        zoom: u16,
        rotation_x: u16,
        rotation_y: u16,
    },
    /// Type 7: an item list drawn as text.
    ItemText {
        style: TextStyle,
        colour: i32,
        pad_x: i16,
        pad_y: i16,
        is_inventory: bool,
        actions: [Option<String>; 5],
    },
    /// Type 8: a tooltip.
    Tooltip { text: String },
    /// Any other type: no type-specific data.
    Other,
}

/// One interface component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Interface {
    pub id: u16,
    /// The root interface this component was declared under.
    pub parent: Option<u16>,
    pub kind: u8,
    pub action_type: u8,
    pub content_type: u16,
    pub width: u16,
    pub height: u16,
    pub alpha: u8,
    pub hover_popup: Option<u16>,
    pub conditions: Vec<Condition>,
    /// Client scripts, as opcode/operand words.
    pub scripts: Vec<Vec<u16>>,
    pub content: Content,
    pub selected_action: Option<String>,
    pub spell_name: Option<String>,
    pub spell_usable_on: Option<u16>,
    pub tooltip: Option<String>,
}
