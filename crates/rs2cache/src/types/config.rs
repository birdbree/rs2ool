//! Config definitions from the config archive.
//!
//! Fields hold what the original client ends up with after decoding: its
//! defaults for opcodes that never show up, its derived flags, and `None`
//! where it would use -1. Anything a decoder couldn't make sense of lands in
//! `unknown_opcodes`, which is left out of the JSON when it's empty.
//!
//! Colours in [`Recolour`] are 16-bit HSL (hue 6 bits, saturation 3,
//! lightness 7), the same encoding as model face colours. Scales are in
//! 1/128 units, so 128 means 1.0.

use serde::{Deserialize, Serialize};

/// Swap HSL colour `from` for `to` on every face that uses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recolour {
    /// The face colour to look for.
    pub from: u16,
    /// The colour it becomes.
    pub to: u16,
}

/// A morph table: a varbit (or, failing that, a varp) value picks which
/// entry of `children` to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Morph {
    /// The varbit whose value indexes `children`; `None` when the stream
    /// gives 65535.
    pub varbit: Option<u16>,
    /// The varp used when there's no varbit; `None` for 65535.
    pub varp: Option<u16>,
    /// Child definition ids of the same type, indexed by the variable's
    /// value. `None` entries (65535) show nothing.
    pub children: Vec<Option<u16>>,
}

/// Five option strings; `None` where absent or `"hidden"`.
pub type Actions = [Option<String>; 5];

fn is_false(b: &bool) -> bool {
    !*b
}

/// One model of a location, with the placement type it's drawn for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocModel {
    /// Model id (index 1).
    pub model: u16,
    /// The 317 location type (0–22) this model is drawn for. `None` when
    /// the definition lists models without types (op 5), which means
    /// type 10.
    pub kind: Option<u8>,
}

/// `loc.dat`: a world object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocDef {
    /// Definition id: its position in `loc.idx`.
    pub id: u32,
    /// Models (op 1 typed, op 5 untyped). A later model opcode replaces the
    /// whole list.
    pub models: Vec<LocModel>,
    /// Op 2.
    pub name: Option<String>,
    /// Op 3: the examine text.
    pub examine: Option<String>,
    /// Width in tiles before rotation (op 14); default 1.
    pub size_x: u8,
    /// Length in tiles before rotation (op 15); default 1.
    pub size_y: u8,
    /// Blocks movement. Op 17 clears it, and so does `hollow`.
    pub solid: bool,
    /// Blocks projectiles. Op 18 clears it, and so does `hollow`.
    pub impenetrable: bool,
    /// Whether the object is interactive: op 19's value is 1, or, without
    /// op 19, derived as the original client does from its first model's
    /// type and whether any action opcode showed up.
    pub has_actions: bool,
    /// Op 21: the model is bent to follow the terrain heights under it.
    pub adjust_to_terrain: bool,
    /// Op 22. Despite the name, in the original client this makes the
    /// model keep its vertex normals instead of being lit straight away.
    pub flat_shading: bool,
    /// Op 23 flag; kept as read.
    pub occludes: bool,
    /// Sequence id the object plays (op 24); `None` for 65535.
    pub animation: Option<u16>,
    /// Op 28: how far a wall decoration sits out from its wall; default 16.
    pub decor_displacement: u8,
    /// Lighting ambient offset (op 29).
    pub ambient: i8,
    /// Lighting contrast offset (op 39), kept as read. Unlike npc and obj,
    /// it isn't multiplied by 5 here.
    pub contrast: i8,
    /// Ops 30–34.
    pub actions: Actions,
    /// Op 40, applied in order.
    pub recolours: Vec<Recolour>,
    /// Op 60: which minimap function icon to draw, as an index into the
    /// `mapfunction` sprites.
    pub minimap_function: Option<u16>,
    /// Op 62: the model is mirrored (z negated) before placement.
    pub mirrored: bool,
    /// Op 64 clears it; default true.
    pub casts_shadow: bool,
    /// X scale (op 65), 1/128 units.
    pub scale_x: u16,
    /// Y (vertical) scale (op 66), 1/128 units.
    pub scale_y: u16,
    /// Z scale (op 67), 1/128 units.
    pub scale_z: u16,
    /// Op 68: which `mapscene` sprite the minimap draws for it.
    pub mapscene: Option<u16>,
    /// Op 69: a bit mask of sides, one bit per direction in the low four
    /// bits. Kept as read.
    pub blocked_directions: u8,
    /// Model x offset (op 70), in model units.
    pub offset_x: i16,
    /// Model y offset (op 71). Y points down, so negative is up.
    pub offset_y: i16,
    /// Model z offset (op 72).
    pub offset_z: i16,
    /// Op 73 flag; kept as read.
    pub obstructs_ground: bool,
    /// Op 74. Clears `solid` and `impenetrable` once the stream ends.
    pub hollow: bool,
    /// Whether items can be placed on it. Op 75 sets it; otherwise it's 1
    /// when the object is still solid after `hollow` is applied, else 0.
    pub support_items: u8,
    /// Op 77; children are loc ids.
    pub morph: Option<Morph>,
    /// Opcodes the decoder didn't recognise, in the order met.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown_opcodes: Vec<u8>,
}

/// `npc.dat`: a non-player character.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NpcDef {
    /// Definition id: its position in `npc.idx`.
    pub id: u32,
    /// Body models (op 1), merged into one mesh to draw.
    pub models: Vec<u16>,
    /// Op 2.
    pub name: Option<String>,
    /// Op 3: the examine text.
    pub examine: Option<String>,
    /// Footprint in tiles (op 12); default 1.
    pub size: i8,
    /// Standing sequence (op 13). Kept as read, so 65535 stays 65535.
    pub stand_anim: Option<u16>,
    /// Walking sequence (op 14, or op 17's first value).
    pub walk_anim: Option<u16>,
    /// Op 17's second value: played when walking away from the facing
    /// direction.
    pub walk_back_anim: Option<u16>,
    /// Op 17's third value: played when walking sideways, facing a quarter
    /// turn one way.
    pub walk_left_anim: Option<u16>,
    /// Op 17's fourth value: the same for the other side.
    pub walk_right_anim: Option<u16>,
    /// Ops 30–34.
    pub actions: Actions,
    /// Op 40, applied in order.
    pub recolours: Vec<Recolour>,
    /// Models for the dialogue head (op 60).
    pub chat_head_models: Vec<u16>,
    /// Op 93 clears it; default true.
    pub draw_minimap_dot: bool,
    /// Op 95.
    pub combat_level: Option<u16>,
    /// Horizontal (x and z) scale (op 97), 1/128 units.
    pub scale_xz: u16,
    /// Vertical scale (op 98), 1/128 units.
    pub scale_y: u16,
    /// Op 99: the original client adds these to the scene in an earlier
    /// pass than other characters.
    pub visible: bool,
    /// Lighting ambient offset (op 100).
    pub ambient: i8,
    /// Lighting contrast offset (op 101), already multiplied by 5 as the
    /// original client does, so −640 to 635.
    pub contrast: i16,
    /// Head icon index (op 102).
    pub head_icon: Option<u16>,
    /// Op 103; default 32.
    pub turn_speed: u16,
    /// Op 106; children are npc ids.
    pub morph: Option<Morph>,
    /// Op 107 clears it; default true.
    pub clickable: bool,
    /// Opcodes the decoder didn't recognise, in the order met.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown_opcodes: Vec<u8>,
}

/// One stack-size variant of an item (ops 100–109).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackVariant {
    /// Slot 0–9, from `op − 100`.
    pub slot: u8,
    /// The obj id shown for this variant.
    pub item: u16,
    /// The stack count the variant applies from.
    pub count: u16,
}

/// `obj.dat`: an item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjDef {
    /// Definition id: its position in `obj.idx`.
    pub id: u32,
    /// Model used for the inventory icon and on the ground (op 1); default 0.
    pub inventory_model: u16,
    /// Op 2.
    pub name: Option<String>,
    /// Op 3: the examine text.
    pub examine: Option<String>,
    /// Icon camera zoom (op 4); default 2000.
    pub icon_zoom: u16,
    /// Icon pitch (op 5), in 2048ths of a turn.
    pub icon_pitch: u16,
    /// Icon yaw (op 6), in 2048ths of a turn.
    pub icon_yaw: u16,
    /// Icon roll (op 95), in 2048ths of a turn.
    pub icon_roll: u16,
    /// Icon x offset (op 7).
    pub icon_offset_x: i16,
    /// Icon y offset (op 8).
    pub icon_offset_y: i16,
    /// Op 11.
    pub stackable: bool,
    /// Op 12; default 1.
    pub value: i32,
    /// Op 16.
    pub members: bool,
    /// First worn model for the male body (op 23).
    pub male_equip1: Option<u16>,
    /// Added to every vertex's y of the male worn model (op 23).
    pub male_offset: i8,
    /// Second worn model for the male body (op 24).
    pub male_equip2: Option<u16>,
    /// Third worn model for the male body (op 78); only used when there's
    /// a second.
    pub male_equip3: Option<u16>,
    /// First worn model for the female body (op 25).
    pub female_equip1: Option<u16>,
    /// Added to every vertex's y of the female worn model (op 25).
    pub female_offset: i8,
    /// Second worn model for the female body (op 26).
    pub female_equip2: Option<u16>,
    /// Third worn model for the female body (op 79); only used when
    /// there's a second.
    pub female_equip3: Option<u16>,
    /// First male dialogue-head model (op 90).
    pub male_chat_head1: Option<u16>,
    /// Second male dialogue-head model (op 92).
    pub male_chat_head2: Option<u16>,
    /// First female dialogue-head model (op 91).
    pub female_chat_head1: Option<u16>,
    /// Second female dialogue-head model (op 93).
    pub female_chat_head2: Option<u16>,
    /// Options when it's on the ground (ops 30–34).
    pub ground_actions: Actions,
    /// Options in the inventory (ops 35–39). Unlike ground actions,
    /// `"hidden"` is kept as written.
    pub inventory_actions: Actions,
    /// Op 40, applied in order.
    pub recolours: Vec<Recolour>,
    /// Op 97: the obj id of this item's noted (or unnoted) counterpart.
    pub noted_id: Option<u16>,
    /// Op 98: the obj id of the note template.
    pub noted_template: Option<u16>,
    /// Ops 100–109, sorted by slot; a repeated slot keeps the later value.
    pub stack_variants: Vec<StackVariant>,
    /// X scale (op 110), 1/128 units.
    pub scale_x: u16,
    /// Y scale (op 111), 1/128 units.
    pub scale_y: u16,
    /// Z scale (op 112), 1/128 units.
    pub scale_z: u16,
    /// Lighting ambient offset (op 113).
    pub ambient: i8,
    /// Lighting contrast offset (op 114), already multiplied by 5 as the
    /// original client does, so −640 to 635.
    pub contrast: i16,
    /// Op 115; kept as read.
    pub team: u8,
    /// Opcodes the decoder didn't recognise, in the order met.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown_opcodes: Vec<u8>,
}

/// `idk.dat`: an identity-kit (player body) part.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdkDef {
    /// Definition id: its position in `idk.dat`.
    pub id: u32,
    /// Op 1: 0–6 are the male slots, 7–13 the same slots for the female
    /// body. So `slot = body_part % 7`, `body_type = body_part / 7`.
    pub body_part: Option<u8>,
    /// Op 2; merged into one mesh to draw.
    pub models: Vec<u16>,
    /// Offered in character design; op 3 clears it.
    pub selectable: bool,
    /// Ops 40–45 and 50–55, paired slot by slot up to the first zero
    /// `from`.
    pub recolours: Vec<Recolour>,
    /// Dialogue-head models (ops 60–64), one per slot.
    pub chat_head_models: [Option<u16>; 5],
    /// Opcodes the decoder didn't recognise, in the order met.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown_opcodes: Vec<u8>,
}

/// One frame of a sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeqFrame {
    /// Global frame id. Look it up with
    /// [`FrameIndex`](crate::anim::FrameIndex); it isn't a file id.
    pub frame: u16,
    /// Global frame id applied to the interleaved slots, if any (65535 in
    /// the stream means none).
    pub secondary: Option<u16>,
    /// Length in client cycles of 20 ms. 0 means "use the frame's own delay
    /// from its frame pack".
    pub delay: u16,
}

/// `seq.dat`: an animation sequence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeqDef {
    /// Definition id: its position in `seq.dat`.
    pub id: u32,
    /// Op 1, in play order.
    pub frames: Vec<SeqFrame>,
    /// How many frames to step back when the sequence runs off the end
    /// (op 2). Kept as read, so 65535 stays 65535.
    pub loop_offset: Option<u16>,
    /// Op 3: the frame-base slots the secondary frames drive, as stored
    /// (ascending in practice), without the original client's sentinel.
    /// Yes, they're slot numbers, not vertex labels.
    pub interleave_labels: Option<Vec<u8>>,
    /// Op 4 flag; kept as read.
    pub stretches: bool,
    /// Op 5; default 5. A newly requested sequence only replaces the one
    /// playing if its priority is at least as high.
    pub priority: u8,
    /// Op 6: replaces the player's off-hand (shield slot) equipment while
    /// this plays, in the equipment slot's own encoding.
    pub offhand_override: Option<u16>,
    /// Op 7: the same for the main-hand (weapon slot) equipment.
    pub mainhand_override: Option<u16>,
    /// Op 8: how many times it loops before stopping; default 99.
    pub max_loops: u8,
    /// Op 9: how the sequence yields to movement while the entity is
    /// moving. Without op 9 it's 2 if there are interleave labels, else 0.
    pub animating_precedence: u8,
    /// Op 10: how it yields to movement otherwise. Same default rule as
    /// `animating_precedence`.
    pub walking_precedence: u8,
    /// Op 11: what happens when the playing sequence is requested again
    /// (1 restarts it, 2 only resets the loop count); default 2.
    pub replay_mode: u8,
    /// Opcodes the decoder didn't recognise, in the order met.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown_opcodes: Vec<u8>,
}

/// `spotanim.dat`: a graphic effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpotAnimDef {
    /// Definition id: its position in `spotanim.dat`.
    pub id: u32,
    /// Op 1.
    pub model: Option<u16>,
    /// The sequence that animates the model (op 2).
    pub sequence: Option<u16>,
    /// Horizontal scale (op 4), applied to x and z, 1/128 units. Don't read
    /// the name as axes.
    pub scale_xy: u16,
    /// Vertical scale (op 5), applied to y, 1/128 units.
    pub scale_z: u16,
    /// Op 6; kept as read.
    pub rotation: u16,
    /// Lighting ambient (op 7), unsigned.
    pub ambient: u8,
    /// Lighting contrast (op 8), unsigned and not multiplied.
    pub contrast: u8,
    /// Ops 40–45 and 50–55, paired slot by slot up to the first zero
    /// `from`.
    pub recolours: Vec<Recolour>,
    /// Opcodes the decoder didn't recognise, in the order met.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown_opcodes: Vec<u8>,
}

/// `flo.dat`: an underlay or overlay floor type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FloDef {
    /// Definition id: its position in `flo.dat`. Map tiles store it plus 1.
    pub id: u32,
    /// 24-bit RGB, `0xRRGGBB` (op 1).
    pub rgb: Option<u32>,
    /// Texture id, 0–49 (op 2).
    pub texture: Option<u8>,
    /// Op 3. The original client reads it and then ignores it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub hide_underlay: bool,
    /// Op 5 clears it; default true.
    pub occlude: bool,
    /// Op 6.
    pub name: Option<String>,
    /// Op 7: 24-bit RGB used on the minimap.
    pub secondary_rgb: Option<u32>,
    /// Opcodes the decoder didn't recognise, in the order met.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown_opcodes: Vec<u8>,
}

/// `varp.dat`: a player variable.
///
/// Only op 5 has a known meaning. The rest are kept under their opcode
/// numbers rather than given names nobody can back up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VarpDef {
    /// Definition id: its position in `varp.dat`.
    pub id: u32,
    /// Op 5: selects built-in client behaviour driven by this variable.
    pub client_code: Option<u16>,
    /// Op 1, as read.
    pub op1: Option<u8>,
    /// Op 2, as read.
    pub op2: Option<u8>,
    /// Op 3: the original client also lists this varp in a separate table.
    pub op3: bool,
    /// Op 4 flag.
    pub op4: bool,
    /// Op 6 flag.
    pub op6: bool,
    /// Op 7, as read.
    pub op7: Option<i32>,
    /// Ops 8 and 11 (and any varbit with op 2) set this same flag.
    pub flagged: bool,
    /// Op 10, as read.
    pub op10: Option<String>,
    /// Op 12, as read.
    pub op12: Option<i32>,
    /// Op 13 flag.
    pub op13: bool,
    /// Opcodes the decoder didn't recognise, in the order met.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown_opcodes: Vec<u8>,
}

/// `varbit.dat`: a bit range of a varp.
///
/// The value is bits `lsb` through `msb` of the varp, both ends included.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VarbitDef {
    /// Definition id: its position in `varbit.dat`.
    pub id: u32,
    /// The varp this varbit lives in (op 1).
    pub varp: u16,
    /// Lowest bit, 0–31 (op 1).
    pub lsb: u8,
    /// Highest bit, 0–31 (op 1).
    pub msb: u8,
    /// Op 2: marks the varp as flagged.
    pub flags_varp: bool,
    /// Op 3, as read.
    pub op3: Option<i32>,
    /// Op 4, as read.
    pub op4: Option<i32>,
    /// Op 10, as read.
    pub op10: Option<String>,
    /// Opcodes the decoder didn't recognise, in the order met.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unknown_opcodes: Vec<u8>,
}
