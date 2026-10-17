//! `cachedump`'s commands as a library, so the explorer runs the same code
//! and writes the same files. The binary maps its command line onto these
//! option structs.

pub mod dump;
pub mod groups;
pub mod list;
pub mod output;
pub mod select;
pub mod verify;
pub mod xref;

use std::path::PathBuf;

use rs2cache::types::record::RecordType;

/// Which ids to take: one, an inclusive range, or all.
#[derive(Debug, Clone, Default)]
pub struct IdArgs {
    pub id: Option<u32>,
    pub range: Option<(u32, u32)>,
    pub all: bool,
}

/// Where the records come from: exactly one of these is set.
#[derive(Debug, Clone, Default)]
pub struct SourceArgs {
    /// An index, 0–4.
    pub index: Option<u8>,
    /// An archive of index 0, by name or number.
    pub archive: Option<String>,
    /// A record type.
    pub kind: Option<RecordType>,
    /// One map region, `(rx, ry)`.
    pub region: Option<(u8, u8)>,
    /// Every region in map_index.
    pub all_regions: bool,
    /// A dependency group: `npc:<id>`, `item:<id>` or `player-kit`.
    pub group: Option<String>,
}

/// Where and how to write.
#[derive(Debug, Clone, Default)]
pub struct OutArgs {
    pub dir: PathBuf,
    /// A `<id>.meta.json` sidecar beside every output.
    pub meta: bool,
    /// Only the sidecars.
    pub meta_only: bool,
    /// Reverse references in sidecars (cached in `<out>/xref.json`).
    pub xref: bool,
    /// One `<type>.jsonl` instead of a file per record.
    pub jsonl: bool,
    /// No progress bars (the explorer reports progress itself).
    pub quiet: bool,
}

/// A `dump` run.
#[derive(Debug, Clone, Default)]
pub struct DumpArgs {
    pub source: SourceArgs,
    pub ids: IdArgs,
    /// With an archive, only this entry (a name, or `#hash`).
    pub entry: Option<String>,
    /// Stored (compressed) bytes instead of inflated ones.
    pub raw: bool,
    /// Decode to JSON/PNG instead of writing bytes.
    pub decode: bool,
    pub out: OutArgs,
}

/// How a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    /// Some records failed; they are listed in the manifest or report.
    Partial,
}
