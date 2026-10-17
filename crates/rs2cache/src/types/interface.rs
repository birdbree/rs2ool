//! Interfaces from the interface archive's `data` entry.
//!
//! Colours here are `i32` RGB, `0xRRGGBB`. "Enabled" variants are what a
//! component shows while all its [`Condition`]s hold; "hover" variants are
//! for when the mouse is over it.

use serde::{Deserialize, Serialize};

/// A child placed inside a layer, at an offset from the layer's corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Child {
    /// Component id of the child.
    pub id: u16,
    /// X offset from the layer's left edge, in pixels.
    pub x: i16,
    /// Y offset from the layer's top edge, in pixels.
    pub y: i16,
}

/// One enable condition: compare the result of the script at the same
/// position in [`Interface::scripts`] against `value`, using `op`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Condition {
    /// Comparison: 2 holds when the result is below `value`, 3 when it's
    /// above, 4 when it differs, and anything else when it's equal.
    pub op: u8,
    /// The value to compare against.
    pub value: u16,
}

/// A sprite reference `name,index`: sub-sprite `index` of entry
/// `<name>.dat` in the media archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpriteRef {
    /// Entry name without `.dat`.
    pub name: String,
    /// Sub-sprite index within that entry.
    pub index: u32,
}

/// Text style shared by text-bearing components.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextStyle {
    /// Text is centred horizontally.
    pub centered: bool,
    /// Font index: 0 p11, 1 p12, 2 b12, 3 q8 (the
    /// [`FONTS`](crate::sprite::FONTS) order).
    pub font: u8,
    /// Text is drawn with a drop shadow.
    pub shadow: bool,
}

/// The type-specific part of an interface component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Content {
    /// Type 0: a container of positioned children.
    Layer {
        /// Height of the scrollable content in pixels; 0 when it doesn't
        /// scroll.
        scroll_max: u16,
        /// The original client skips the layer unless it's the one being
        /// hovered, and the server can toggle it.
        hidden: bool,
        /// Children in drawing order.
        children: Vec<Child>,
    },
    /// Type 1: unused in 317; its fields are read and kept under neutral
    /// names.
    Unused {
        /// The block's first value.
        a: u16,
        /// The block's second value.
        b: u8,
        /// Text style.
        style: TextStyle,
        /// Colour, `0xRRGGBB`.
        colour: i32,
    },
    /// Type 2: an inventory grid of `width × height` slots.
    Inventory {
        /// Items can be dragged to another slot.
        swappable: bool,
        /// The grid shows an inventory.
        is_inventory: bool,
        /// Items in it can be used on things.
        usable: bool,
        /// Dragging moves an item into the target slot outright, emptying
        /// the source, instead of swapping the two.
        replaceable: bool,
        /// Horizontal gap between slots, in pixels.
        pad_x: u8,
        /// Vertical gap between slots, in pixels.
        pad_y: u8,
        /// 20 optional background sprites, each `(x, y, sprite)`. The
        /// sprite is `None` when its reference doesn't parse.
        slot_sprites: Vec<Option<(i16, i16, Option<SpriteRef>)>>,
        /// Item options; an empty string is stored as `None`.
        actions: [Option<String>; 5],
    },
    /// Type 3: a rectangle.
    Rectangle {
        /// Filled rather than outlined.
        filled: bool,
        /// Normal colour.
        colour: i32,
        /// Colour while the conditions hold.
        enabled_colour: i32,
        /// Colour while hovered.
        hover_colour: i32,
        /// Colour while hovered and enabled.
        enabled_hover_colour: i32,
    },
    /// Type 4: text.
    Text {
        /// Text style.
        style: TextStyle,
        /// Normal text.
        text: String,
        /// Text while the conditions hold.
        enabled_text: String,
        /// Normal colour.
        colour: i32,
        /// Colour while the conditions hold.
        enabled_colour: i32,
        /// Colour while hovered.
        hover_colour: i32,
        /// Colour while hovered and enabled.
        enabled_hover_colour: i32,
    },
    /// Type 5: a sprite, with an alternative shown when enabled.
    Sprite {
        /// Normal sprite; `None` when the reference is empty or doesn't
        /// parse.
        sprite: Option<SpriteRef>,
        /// Sprite while the conditions hold.
        enabled_sprite: Option<SpriteRef>,
    },
    /// Type 6: a 3D model.
    Model {
        /// Model id shown normally; `None` when the block leaves it out.
        model: Option<u16>,
        /// Model id shown while the conditions hold.
        enabled_model: Option<u16>,
        /// Sequence played on the normal model.
        animation: Option<u16>,
        /// Sequence played on the enabled model.
        enabled_animation: Option<u16>,
        /// Camera zoom.
        zoom: u16,
        /// Rotation about x, in 2048ths of a turn.
        rotation_x: u16,
        /// Rotation about y, in 2048ths of a turn.
        rotation_y: u16,
    },
    /// Type 7: an item list drawn as text.
    ItemText {
        /// Text style.
        style: TextStyle,
        /// Text colour.
        colour: i32,
        /// Horizontal gap between entries, in pixels.
        pad_x: i16,
        /// Vertical gap between entries, in pixels.
        pad_y: i16,
        /// The list shows an inventory.
        is_inventory: bool,
        /// Item options; an empty string is stored as `None`.
        actions: [Option<String>; 5],
    },
    /// Type 8: a tooltip. Its text sits after the spell block in the
    /// stream, not in the type-specific position.
    Tooltip {
        /// The tooltip text.
        text: String,
    },
    /// Any other type: no type-specific data.
    Other,
}

/// One interface component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Interface {
    /// Component id; also the record id.
    pub id: u16,
    /// The root this component was declared under; `None` before the
    /// first root marker. A root's parent is its own id.
    pub parent: Option<u16>,
    /// Component type, 0–8; decides which [`Content`] variant you get.
    pub kind: u8,
    /// What clicking does. 2 targets something (so there's a spell block);
    /// 1, 4, 5 and 6 come with a tooltip.
    pub action_type: u8,
    /// Selects built-in behaviour in the original client; 0 for none.
    pub content_type: u16,
    /// Width in pixels.
    pub width: u16,
    /// Height in pixels.
    pub height: u16,
    /// Transparency: 0 is opaque, higher is more see-through.
    pub alpha: u8,
    /// Component shown while hovering this one.
    pub hover_popup: Option<u16>,
    /// Every condition must hold for the enabled state to show.
    pub conditions: Vec<Condition>,
    /// Scripts as opcode/operand words, kept as stored. Condition `i` uses
    /// script `i`.
    pub scripts: Vec<Vec<u16>>,
    /// The type-specific block.
    pub content: Content,
    /// Spell block, present when `action_type` is 2 or `kind` is 2.
    pub selected_action: Option<String>,
    /// Spell block: the spell's name.
    pub spell_name: Option<String>,
    /// Spell block: what the spell can target, kept as read.
    pub spell_usable_on: Option<u16>,
    /// Tooltip for action types 1, 4, 5 and 6. An empty string becomes
    /// `"Ok"`, `"Select"` or `"Continue"`, as in the original client.
    pub tooltip: Option<String>,
}
