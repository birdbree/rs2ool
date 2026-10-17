//! Config definitions from the config archive.
//!
//! Fields hold the values the 317 client ends up with after decoding: its
//! defaults for opcodes that are absent, its derived flags, and `None` where
//! the client uses -1. Anything a decoder could not understand is listed in
//! `unknown_opcodes`.

use serde::{Deserialize, Serialize};

/// Replace HSL colour `from` with `to` on every face that uses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recolour {
    pub from: u16,
    pub to: u16,
}

/// A morph table: which child definition to show is picked by a varbit (or,
/// failing that, a varp) value indexing `children`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Morph {
    pub varbit: Option<u16>,
    pub varp: Option<u16>,
    pub children: Vec<Option<u16>>,
}

/// Five option strings; `None` where absent or `"hidden"`.
pub type Actions = [Option<String>; 5];

fn is_false(b: &bool) -> bool {
    !*b
}

/// One model of a location, with the placement type it is used for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocModel {
    pub model: u16,
    /// The 317 location type (0–22) this model is drawn for; `None` when the
    /// definition gives models without types (op 5), meaning type 10.
    pub kind: Option<u8>,
}

/// `loc.dat`: a world object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocDef {
    pub id: u32,
    pub models: Vec<LocModel>,
    pub name: Option<String>,
    pub examine: Option<String>,
    pub size_x: u8,
    pub size_y: u8,
    /// Blocks movement (op 17 clears it; `hollow` clears it too).
    pub solid: bool,
    /// Blocks projectiles (op 18 clears it; `hollow` clears it too).
    pub impenetrable: bool,
    /// Whether the object is interactive: op 19, or derived as the client
    /// does from its actions and model types.
    pub has_actions: bool,
    pub adjust_to_terrain: bool,
    pub flat_shading: bool,
    pub occludes: bool,
    pub animation: Option<u16>,
    pub decor_displacement: u8,
    pub ambient: i8,
    pub contrast: i8,
    pub actions: Actions,
    pub recolours: Vec<Recolour>,
    pub minimap_function: Option<u16>,
    pub mirrored: bool,
    pub casts_shadow: bool,
    pub scale_x: u16,
    pub scale_y: u16,
    pub scale_z: u16,
    pub mapscene: Option<u16>,
    pub blocked_directions: u8,
    pub offset_x: i16,
    pub offset_y: i16,
    pub offset_z: i16,
    pub obstructs_ground: bool,
    pub hollow: bool,
    /// Whether items can be placed on it; defaults to 1 when solid.
    pub support_items: u8,
    pub morph: Option<Morph>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown_opcodes: Vec<u8>,
}

/// `npc.dat`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NpcDef {
    pub id: u32,
    pub models: Vec<u16>,
    pub name: Option<String>,
    pub examine: Option<String>,
    /// Footprint in tiles.
    pub size: i8,
    pub stand_anim: Option<u16>,
    pub walk_anim: Option<u16>,
    /// Played when walking away from the facing direction.
    pub walk_back_anim: Option<u16>,
    /// Op 17's third value: played when walking sideways, facing a quarter
    /// turn one way.
    pub walk_left_anim: Option<u16>,
    /// Op 17's fourth value: the same for the other side.
    pub walk_right_anim: Option<u16>,
    pub actions: Actions,
    pub recolours: Vec<Recolour>,
    pub chat_head_models: Vec<u16>,
    pub draw_minimap_dot: bool,
    pub combat_level: Option<u16>,
    pub scale_xz: u16,
    pub scale_y: u16,
    /// Op 99: drawn above other entities on the same tile.
    pub visible: bool,
    pub ambient: i8,
    /// Already multiplied by 5, as the client does.
    pub contrast: i16,
    pub head_icon: Option<u16>,
    pub turn_speed: u16,
    pub morph: Option<Morph>,
    pub clickable: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown_opcodes: Vec<u8>,
}

/// One stack-size variant of an item (ops 100–109).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackVariant {
    pub slot: u8,
    pub item: u16,
    pub count: u16,
}

/// `obj.dat`: an item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjDef {
    pub id: u32,
    pub inventory_model: u16,
    pub name: Option<String>,
    pub examine: Option<String>,
    pub icon_zoom: u16,
    pub icon_pitch: u16,
    pub icon_yaw: u16,
    pub icon_roll: u16,
    pub icon_offset_x: i16,
    pub icon_offset_y: i16,
    pub stackable: bool,
    pub value: i32,
    pub members: bool,
    pub male_equip1: Option<u16>,
    pub male_offset: i8,
    pub male_equip2: Option<u16>,
    pub male_equip3: Option<u16>,
    pub female_equip1: Option<u16>,
    pub female_offset: i8,
    pub female_equip2: Option<u16>,
    pub female_equip3: Option<u16>,
    pub male_chat_head1: Option<u16>,
    pub male_chat_head2: Option<u16>,
    pub female_chat_head1: Option<u16>,
    pub female_chat_head2: Option<u16>,
    pub ground_actions: Actions,
    /// Unlike ground actions, `"hidden"` is kept as written.
    pub inventory_actions: Actions,
    pub recolours: Vec<Recolour>,
    pub noted_id: Option<u16>,
    pub noted_template: Option<u16>,
    pub stack_variants: Vec<StackVariant>,
    pub scale_x: u16,
    pub scale_y: u16,
    pub scale_z: u16,
    pub ambient: i8,
    /// Already multiplied by 5, as the client does.
    pub contrast: i16,
    pub team: u8,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown_opcodes: Vec<u8>,
}

/// `idk.dat`: an identity-kit (player body) part.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdkDef {
    pub id: u32,
    /// 0–6 male slots, 7–13 the same slots for the female body.
    pub body_part: Option<u8>,
    pub models: Vec<u16>,
    /// Offered in character design; op 3 clears it.
    pub selectable: bool,
    pub recolours: Vec<Recolour>,
    pub chat_head_models: [Option<u16>; 5],
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown_opcodes: Vec<u8>,
}

/// One frame of a sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeqFrame {
    /// Global frame id.
    pub frame: u16,
    /// Frame applied to the interleaved labels, if any.
    pub secondary: Option<u16>,
    /// Length in client cycles of 20 ms; 0 means "use the frame's own delay
    /// from its frame pack".
    pub delay: u16,
}

/// `seq.dat`: an animation sequence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeqDef {
    pub id: u32,
    pub frames: Vec<SeqFrame>,
    /// Frames from the end to replay when looping.
    pub loop_offset: Option<u16>,
    /// Labels the secondary frames drive, ascending, without the client's
    /// sentinel.
    pub interleave_labels: Option<Vec<u8>>,
    pub stretches: bool,
    pub priority: u8,
    pub offhand_override: Option<u16>,
    pub mainhand_override: Option<u16>,
    pub max_loops: u8,
    pub animating_precedence: u8,
    pub walking_precedence: u8,
    pub replay_mode: u8,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown_opcodes: Vec<u8>,
}

/// `spotanim.dat`: a graphic effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpotAnimDef {
    pub id: u32,
    pub model: Option<u16>,
    pub sequence: Option<u16>,
    pub scale_xy: u16,
    pub scale_z: u16,
    pub rotation: u16,
    pub ambient: u8,
    pub contrast: u8,
    pub recolours: Vec<Recolour>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown_opcodes: Vec<u8>,
}

/// `flo.dat`: an underlay or overlay floor type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FloDef {
    pub id: u32,
    /// 24-bit RGB.
    pub rgb: Option<u32>,
    pub texture: Option<u8>,
    /// Op 3. The 317 client reads and ignores it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub hide_underlay: bool,
    pub occlude: bool,
    pub name: Option<String>,
    /// Op 7: the colour used on the minimap.
    pub secondary_rgb: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown_opcodes: Vec<u8>,
}

/// `varp.dat`: a player variable. Only op 5 has a known meaning; the other
/// opcodes are kept under their numbers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VarpDef {
    pub id: u32,
    /// Op 5: selects client-side behaviour driven by this variable.
    pub client_code: Option<u16>,
    pub op1: Option<u8>,
    pub op2: Option<u8>,
    /// Op 3: the client lists this varp in a separate table.
    pub op3: bool,
    pub op4: bool,
    pub op6: bool,
    pub op7: Option<i32>,
    /// Ops 8 and 11 (and any varbit with op 2) set the same client flag.
    pub flagged: bool,
    pub op10: Option<String>,
    pub op12: Option<i32>,
    pub op13: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown_opcodes: Vec<u8>,
}

/// `varbit.dat`: a bit range of a varp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VarbitDef {
    pub id: u32,
    pub varp: u16,
    pub lsb: u8,
    pub msb: u8,
    /// Op 2: marks the varp as flagged.
    pub flags_varp: bool,
    pub op3: Option<i32>,
    pub op4: Option<i32>,
    pub op10: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown_opcodes: Vec<u8>,
}
