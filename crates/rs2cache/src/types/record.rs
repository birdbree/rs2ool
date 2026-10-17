//! Record types, decoded records and their metadata sidecars.
//!
//! Adding a record type means one [`RecordType`] arm, one [`Record`] arm,
//! one [`TypeSummary`] arm, its decoder, and its explorer tree node.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::types::anim::FramePack;
use crate::types::config::{
    FloDef, IdkDef, LocDef, NpcDef, ObjDef, SeqDef, SpotAnimDef, VarbitDef, VarpDef,
};
use crate::types::interface::Interface;
use crate::types::map::Region;
use crate::types::model::Model;
use crate::types::sprite::{Font, SpriteSheet};

macro_rules! record_types {
    ($($variant:ident => $name:literal),* $(,)?) => {
        /// Every kind of record the tools list, dump and decode.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(rename_all = "lowercase")]
        pub enum RecordType {
            $($variant),*
        }

        impl RecordType {
            pub const ALL: &[RecordType] = &[$(RecordType::$variant),*];

            /// The command-line name (`"model"`, `"spotanim"`).
            pub fn name(self) -> &'static str {
                match self {
                    $(RecordType::$variant => $name),*
                }
            }
        }
    };
}

record_types! {
    Model => "model",
    Anim => "anim",
    Map => "map",
    Loc => "loc",
    Npc => "npc",
    Obj => "obj",
    Idk => "idk",
    Seq => "seq",
    SpotAnim => "spotanim",
    Flo => "flo",
    Varp => "varp",
    Varbit => "varbit",
    Texture => "texture",
    Sprite => "sprite",
    Font => "font",
    Interface => "interface",
    Midi => "midi",
}

impl fmt::Display for RecordType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Error for an unrecognised record type name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownRecordType(pub String);

impl fmt::Display for UnknownRecordType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<_> = RecordType::ALL.iter().map(|t| t.name()).collect();
        write!(
            f,
            "unknown record type {:?}; expected one of {}",
            self.0,
            names.join(", ")
        )
    }
}

impl std::error::Error for UnknownRecordType {}

impl FromStr for RecordType {
    type Err = UnknownRecordType;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        RecordType::ALL
            .iter()
            .copied()
            .find(|t| t.name().eq_ignore_ascii_case(s))
            .ok_or_else(|| UnknownRecordType(s.to_owned()))
    }
}

/// A decoded record of any type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "record", rename_all = "lowercase")]
pub enum Record {
    Model(Model),
    Anim(FramePack),
    Map(Region),
    Loc(LocDef),
    Npc(NpcDef),
    Obj(ObjDef),
    Idk(IdkDef),
    Seq(SeqDef),
    SpotAnim(SpotAnimDef),
    Flo(FloDef),
    Varp(VarpDef),
    Varbit(VarbitDef),
    Texture(SpriteSheet),
    Sprite(SpriteSheet),
    Font(Font),
    Interface(Interface),
    /// MIDI is not decoded; this is the inflated file.
    Midi(Vec<u8>),
}

impl Record {
    pub fn record_type(&self) -> RecordType {
        match self {
            Record::Model(_) => RecordType::Model,
            Record::Anim(_) => RecordType::Anim,
            Record::Map(_) => RecordType::Map,
            Record::Loc(_) => RecordType::Loc,
            Record::Npc(_) => RecordType::Npc,
            Record::Obj(_) => RecordType::Obj,
            Record::Idk(_) => RecordType::Idk,
            Record::Seq(_) => RecordType::Seq,
            Record::SpotAnim(_) => RecordType::SpotAnim,
            Record::Flo(_) => RecordType::Flo,
            Record::Varp(_) => RecordType::Varp,
            Record::Varbit(_) => RecordType::Varbit,
            Record::Texture(_) => RecordType::Texture,
            Record::Sprite(_) => RecordType::Sprite,
            Record::Font(_) => RecordType::Font,
            Record::Interface(_) => RecordType::Interface,
            Record::Midi(_) => RecordType::Midi,
        }
    }
}

/// Where a stored file came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Source {
    /// A file of index 1–4.
    Index { index: u8, id: u32 },
    /// An entry of a jag archive in index 0; `entry` is the name, or `#hash`.
    Archive { archive: u8, entry: String },
}

/// One stored file a record was decoded from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMeta {
    pub source: Source,
    /// Sectors of an index 1–4 file, in chain order. Empty for archive
    /// entries, whose archive's chain is the same for every entry.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sector_chain: Vec<u32>,
    pub raw_size: u64,
    pub inflated_size: u64,
    /// CRC32 of the stored (still compressed) bytes.
    pub crc32: u32,
    /// `"gzip"`, `"bzip2"`, or `"none"`.
    pub compression: String,
}

/// Bounds `(min, max)` of a model, in cache units.
pub type Bounds = ([i32; 3], [i32; 3]);

/// The type-specific part of a metadata sidecar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "summary", rename_all = "lowercase")]
pub enum TypeSummary {
    Model {
        vertex_count: usize,
        face_count: usize,
        textured_face_count: usize,
        vertex_label_count: usize,
        face_label_count: usize,
        bounds: Option<Bounds>,
        /// Texture ids used, when cross-references were requested.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        textures: Option<Vec<u16>>,
    },
    Anim {
        frame_ids: Vec<u16>,
        transform_count: usize,
        labels: Vec<u8>,
    },
    Map {
        region: u16,
        members: bool,
        /// Lowest and highest corner height per plane.
        height_range: Vec<(i32, i32)>,
        object_count: usize,
    },
    /// Config definitions: the decoded fields.
    Config {
        definition: Box<Record>,
    },
    /// Textures, media and title sprites.
    Sprite {
        width: u16,
        height: u16,
        palette_size: usize,
        sub_sprites: usize,
        /// Models that use this texture, when cross-references were
        /// requested.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        used_by_models: Option<Vec<u32>>,
    },
    Font {
        glyphs: usize,
        max_height: u16,
    },
    Interface {
        kind: u8,
        width: u16,
        height: u16,
        children: usize,
    },
    Midi {
        size: usize,
    },
}

/// A metadata sidecar: what a record is, where it came from, and a
/// type-specific summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meta {
    pub record_type: RecordType,
    pub id: u32,
    pub files: Vec<FileMeta>,
    #[serde(flatten)]
    pub summary: TypeSummary,
}
