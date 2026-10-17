//! Record types, decoded records and their metadata sidecars.
//!
//! Adding a record type means one [`RecordType`] arm, one [`Record`] arm,
//! one [`TypeSummary`] arm and its decoder. Nothing else should need to
//! change.

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
    ($($(#[$doc:meta])* $variant:ident => $name:literal),* $(,)?) => {
        /// Every kind of record the tools list, dump and decode.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(rename_all = "lowercase")]
        pub enum RecordType {
            $($(#[$doc])* $variant),*
        }

        impl RecordType {
            /// Every record type, in declaration order.
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

// Each entry is `Variant => "command-line name"`, with its doc comment.
record_types! {
    /// A model, index 1; id is the file id.
    Model => "model",
    /// A frame pack, index 2; id is the file id.
    Anim => "anim",
    /// A map region, index 4; id is `x << 8 | y`.
    Map => "map",
    /// `loc.dat`: world objects.
    Loc => "loc",
    /// `npc.dat`: non-player characters.
    Npc => "npc",
    /// `obj.dat`: items.
    Obj => "obj",
    /// `idk.dat`: identity-kit body parts.
    Idk => "idk",
    /// `seq.dat`: animation sequences.
    Seq => "seq",
    /// `spotanim.dat`: graphic effects.
    SpotAnim => "spotanim",
    /// `flo.dat`: floor types.
    Flo => "flo",
    /// `varp.dat`: player variables.
    Varp => "varp",
    /// `varbit.dat`: bit ranges of varps.
    Varbit => "varbit",
    /// A texture from the textures archive; ids 0–49.
    Texture => "texture",
    /// A sprite entry from the media or title archive, fonts excluded.
    Sprite => "sprite",
    /// One of the four fonts in the title archive; ids 0–3.
    Font => "font",
    /// An interface component; id is the component id.
    Interface => "interface",
    /// A MIDI file, index 3; id is the file id.
    Midi => "midi",
}

impl fmt::Display for RecordType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Error for an unrecognised record type name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownRecordType(
    /// The name that didn't match.
    pub String,
);

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
    /// A model.
    Model(Model),
    /// A frame pack.
    Anim(FramePack),
    /// A map region.
    Map(Region),
    /// A loc definition.
    Loc(LocDef),
    /// An npc definition.
    Npc(NpcDef),
    /// An obj definition.
    Obj(ObjDef),
    /// An identity-kit definition.
    Idk(IdkDef),
    /// A sequence definition.
    Seq(SeqDef),
    /// A spotanim definition.
    SpotAnim(SpotAnimDef),
    /// A floor definition.
    Flo(FloDef),
    /// A varp definition.
    Varp(VarpDef),
    /// A varbit definition.
    Varbit(VarbitDef),
    /// A texture, as a single-sprite sheet.
    Texture(SpriteSheet),
    /// A sprite entry with all its sub-sprites.
    Sprite(SpriteSheet),
    /// A font with its derived metrics.
    Font(Font),
    /// An interface component.
    Interface(Interface),
    /// MIDI is not decoded; this is the inflated file.
    Midi(Vec<u8>),
}

impl Record {
    /// Which [`RecordType`] this is.
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
    Index {
        /// Index number.
        index: u8,
        /// File id within the index.
        id: u32,
    },
    /// An entry of a jag archive in index 0.
    Archive {
        /// Archive number (file id in index 0).
        archive: u8,
        /// The entry's name, or `#hash` when the name isn't known.
        entry: String,
    },
}

/// One stored file a record was decoded from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMeta {
    /// Where the file lives.
    pub source: Source,
    /// Sectors of an index 1–4 file, in chain order. Empty for archive
    /// entries, whose archive's chain is the same for every entry.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sector_chain: Vec<u32>,
    /// Stored size in bytes: the whole file as read from its sectors, or an
    /// archive entry's compressed size from the archive's table.
    pub raw_size: u64,
    /// Size after decompression.
    pub inflated_size: u64,
    /// CRC-32 of the stored bytes (still compressed, version trailer
    /// included) for an index file. For an archive entry it's the CRC of
    /// the unpacked entry data instead.
    pub crc32: u32,
    /// `"gzip"` or `"none"` for an index file. For an archive entry,
    /// `"bzip2"` (packed on its own), `"bzip2-archive"` (stored plain inside
    /// an archive packed as a whole) or `"none"`.
    pub compression: String,
}

/// Bounds `(min, max)` of a model, in cache units.
pub type Bounds = ([i32; 3], [i32; 3]);

/// The type-specific part of a metadata sidecar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "summary", rename_all = "lowercase")]
pub enum TypeSummary {
    /// A model.
    Model {
        /// Number of vertices.
        vertex_count: usize,
        /// Number of faces.
        face_count: usize,
        /// Number of textured-face triples.
        textured_face_count: usize,
        /// Number of distinct vertex labels.
        vertex_label_count: usize,
        /// Number of distinct face labels.
        face_label_count: usize,
        /// Axis-aligned bounds; `None` for a model with no vertices.
        bounds: Option<Bounds>,
        /// Texture ids used, when cross-references were requested.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        textures: Option<Vec<u16>>,
    },
    /// A frame pack.
    Anim {
        /// Global frame ids the pack defines, in stored order.
        frame_ids: Vec<u16>,
        /// Number of slots in the frame base.
        transform_count: usize,
        /// Distinct labels the base uses, ascending.
        labels: Vec<u8>,
    },
    /// A map region.
    Map {
        /// Region id, `x << 8 | y`.
        region: u16,
        /// Members-only region.
        members: bool,
        /// Lowest and highest corner height per plane.
        height_range: Vec<(i32, i32)>,
        /// Number of placed locations.
        object_count: usize,
    },
    /// Config definitions: the decoded fields.
    Config {
        /// The whole decoded definition.
        definition: Box<Record>,
    },
    /// Textures, media and title sprites.
    Sprite {
        /// Canvas width (`max_width`).
        width: u16,
        /// Canvas height (`max_height`).
        height: u16,
        /// Palette entries, transparent entry 0 included.
        palette_size: usize,
        /// Number of sub-sprites.
        sub_sprites: usize,
        /// Models that use this texture, when cross-references were
        /// requested.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        used_by_models: Option<Vec<u32>>,
    },
    /// A font.
    Font {
        /// Number of glyphs (256).
        glyphs: usize,
        /// Canvas height of the glyph sheet.
        max_height: u16,
    },
    /// An interface component.
    Interface {
        /// Component type.
        kind: u8,
        /// Width in pixels.
        width: u16,
        /// Height in pixels.
        height: u16,
        /// Number of children; 0 for anything but a layer.
        children: usize,
    },
    /// A MIDI file.
    Midi {
        /// Inflated size in bytes.
        size: usize,
    },
}

/// A metadata sidecar: what a record is, where it came from, and a
/// type-specific summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meta {
    /// The record's type.
    pub record_type: RecordType,
    /// The record's id within its type.
    pub id: u32,
    /// Every stored file it was decoded from.
    pub files: Vec<FileMeta>,
    /// The type-specific part, flattened into the same JSON object.
    #[serde(flatten)]
    pub summary: TypeSummary,
}
