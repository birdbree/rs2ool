//! The `rs2ool cache` commands.
//!
//! They live in the library so the explorer runs the same code and writes
//! the same files. The binary's clap structs convert into the plain option
//! structs here ([`DumpArgs`], [`SourceArgs`], [`IdArgs`], [`OutArgs`]), and
//! each command returns an [`Outcome`].
//!
//! Errors come back as `anyhow::Error`. One wrapping a
//! [`select::UsageError`] means the selection made no sense (exit code 1);
//! anything else is a cache or I/O problem (exit code 2).

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
///
/// Set at most one. [`IdArgs::resolve`] turns it into a list.
#[derive(Debug, Clone, Default)]
pub struct IdArgs {
    /// `--id`: exactly this id, whether or not the source has it.
    pub id: Option<u32>,
    /// `--range A-B`: every id from A to B, both included, that the source
    /// actually has.
    pub range: Option<(u32, u32)>,
    /// `--all`: every id the source has.
    pub all: bool,
}

/// Where the records come from. Set exactly one of these.
#[derive(Debug, Clone, Default)]
pub struct SourceArgs {
    /// An index, 0–4.
    pub index: Option<u8>,
    /// An archive of index 0, by name or number.
    pub archive: Option<String>,
    /// A record type.
    pub kind: Option<RecordType>,
    /// One map region, `(rx, ry)`; its map id is `rx << 8 | ry`.
    pub region: Option<(u8, u8)>,
    /// Every region listed in `map_index`.
    pub all_regions: bool,
    /// A dependency group: `npc:<id>`, `item:<id>` or `player-kit`.
    pub group: Option<String>,
}

/// Where and how to write.
#[derive(Debug, Clone, Default)]
pub struct OutArgs {
    /// Output directory; created if it's not there. `manifest.json` goes
    /// at its top.
    pub dir: PathBuf,
    /// A `<id>.meta.json` sidecar beside every output.
    pub meta: bool,
    /// Only the sidecars.
    pub meta_only: bool,
    /// Reverse references in sidecars (cached in `<out>/xref.json`).
    pub xref: bool,
    /// One `<type>.jsonl` instead of a file per record.
    pub jsonl: bool,
    /// No progress bars and no closing summary line; the explorer reports
    /// progress its own way.
    pub quiet: bool,
}

/// Everything a `dump` (or `decode`) run needs.
///
/// `decode` is just a dump with `source.kind` set and `decode: true`.
#[derive(Debug, Clone, Default)]
pub struct DumpArgs {
    /// Where the records come from.
    pub source: SourceArgs,
    /// Which ids; only `--index` and `--type` take them.
    pub ids: IdArgs,
    /// With an archive, only this entry (a name, or `#hash`).
    pub entry: Option<String>,
    /// Stored (compressed) bytes instead of inflated ones.
    pub raw: bool,
    /// Decode to JSON/PNG instead of writing bytes. `--type` and `--group`
    /// always decode, so it's a no-op there.
    pub decode: bool,
    /// Where and how to write.
    pub out: OutArgs,
}

/// How a run ended, when it didn't fail outright.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Everything worked. Absent ids don't count as failures.
    Ok,
    /// Some records failed; they're listed in the manifest or the verify
    /// report. The binary exits 3.
    Partial,
}
