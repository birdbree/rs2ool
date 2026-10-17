//! `rs2ool cache`: list, dump, decode, describe and verify a 317 cache, or
//! browse it.
//!
//! Exit codes: 0 ok, 1 usage, 2 cache error, 3 partial failure (some
//! records failed; they are listed in the manifest or the verify report).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Subcommand};
use rs2cache::types::record::RecordType;
use rs2cache::{Cache, Records};
use rs2ool::cache::{DumpArgs, IdArgs, OutArgs, Outcome, SourceArgs, dump, list, select, verify};

#[derive(Subcommand)]
pub enum Cmd {
    /// Print the indices and archives, or the contents of one.
    List(ListArgs),
    /// Extract records to files, raw or decoded.
    Dump(DumpCli),
    /// Decode records of one type to JSON (shorthand for `dump --type`).
    Decode(DecodeArgs),
    /// Print one record's metadata sidecar.
    Info(InfoArgs),
    /// Check every sector chain and every file; exit 3 on damage.
    Verify,
    /// Open the explorer on the cache.
    #[cfg(feature = "explorer")]
    Explorer {
        #[command(flatten)]
        view: super::ExplorerArgs,
        /// Also open a pack (repeatable).
        #[arg(long = "pack", value_name = "FILE")]
        packs: Vec<PathBuf>,
    },
}

#[derive(Args)]
#[group(multiple = false)]
pub struct ListArgs {
    /// List the ids of an index with their sizes.
    #[arg(long)]
    index: Option<u8>,
    /// List an archive's entries (name or number).
    #[arg(long)]
    archive: Option<String>,
    /// List the ids of a record type, with names where it has them.
    #[arg(long = "type", value_name = "TYPE")]
    kind: Option<RecordType>,
}

/// Which ids to take.
#[derive(Args, Clone, Default)]
#[group(multiple = false)]
pub struct IdCli {
    /// One id.
    #[arg(long)]
    pub id: Option<u32>,
    /// An inclusive id range, `A-B`.
    #[arg(long, value_parser = parse_range)]
    pub range: Option<(u32, u32)>,
    /// Every id.
    #[arg(long)]
    pub all: bool,
}

fn parse_range(s: &str) -> Result<(u32, u32), String> {
    let (a, b) = s.split_once('-').ok_or("expected A-B")?;
    let a = a.trim().parse().map_err(|e| format!("{a}: {e}"))?;
    let b = b.trim().parse().map_err(|e| format!("{b}: {e}"))?;
    if a > b {
        return Err(format!("range {a}-{b} is empty"));
    }
    Ok((a, b))
}

/// Where the records come from: exactly one.
#[derive(Args, Clone)]
#[group(required = true, multiple = false)]
pub struct SourceCli {
    /// An index, 0–4.
    #[arg(long)]
    pub index: Option<u8>,
    /// An archive of index 0, by name or number.
    #[arg(long)]
    pub archive: Option<String>,
    /// A record type.
    #[arg(long = "type", value_name = "TYPE")]
    pub kind: Option<RecordType>,
    /// One map region, `RX_RY`.
    #[arg(long, value_parser = parse_region)]
    pub region: Option<(u8, u8)>,
    /// Every region in map_index.
    #[arg(long)]
    pub all_regions: bool,
    /// A dependency group: `npc:<id>`, `item:<id>` or `player-kit`.
    #[arg(long)]
    pub group: Option<String>,
}

fn parse_region(s: &str) -> Result<(u8, u8), String> {
    let (x, y) = s.split_once('_').ok_or("expected RX_RY, e.g. 50_50")?;
    Ok((
        x.parse().map_err(|e| format!("{x}: {e}"))?,
        y.parse().map_err(|e| format!("{y}: {e}"))?,
    ))
}

#[derive(Args, Clone)]
pub struct DumpCli {
    #[command(flatten)]
    pub source: SourceCli,
    #[command(flatten)]
    pub ids: IdCli,
    /// With --archive, only this entry (a name, or `#hash`).
    #[arg(long, requires = "archive")]
    pub entry: Option<String>,
    /// Write stored (compressed) bytes instead of inflated ones.
    #[arg(long, conflicts_with_all = ["kind", "group", "decode"])]
    pub raw: bool,
    /// Decode to JSON/PNG instead of writing bytes.
    #[arg(long)]
    pub decode: bool,
    #[command(flatten)]
    pub out: OutCli,
}

#[derive(Args, Clone)]
pub struct OutCli {
    /// Output directory.
    #[arg(short = 'o', long = "out")]
    pub dir: PathBuf,
    /// Write a <id>.meta.json sidecar beside every output.
    #[arg(long)]
    pub meta: bool,
    /// Write only the sidecars.
    #[arg(long, conflicts_with = "meta")]
    pub meta_only: bool,
    /// Add reverse references to sidecars (cached in <out>/xref.json).
    #[arg(long)]
    pub xref: bool,
    /// Write one <type>.jsonl instead of a file per record.
    #[arg(long)]
    pub jsonl: bool,
}

#[derive(Args, Clone)]
pub struct DecodeArgs {
    /// Record type.
    pub kind: RecordType,
    #[command(flatten)]
    pub ids: IdCli,
    #[command(flatten)]
    pub out: OutCli,
}

#[derive(Args)]
pub struct InfoArgs {
    #[arg(long = "type", value_name = "TYPE")]
    kind: RecordType,
    #[arg(long)]
    id: u32,
}

impl From<IdCli> for IdArgs {
    fn from(a: IdCli) -> Self {
        IdArgs {
            id: a.id,
            range: a.range,
            all: a.all,
        }
    }
}

impl From<SourceCli> for SourceArgs {
    fn from(a: SourceCli) -> Self {
        SourceArgs {
            index: a.index,
            archive: a.archive,
            kind: a.kind,
            region: a.region,
            all_regions: a.all_regions,
            group: a.group,
        }
    }
}

impl From<OutCli> for OutArgs {
    fn from(a: OutCli) -> Self {
        OutArgs {
            dir: a.dir,
            meta: a.meta,
            meta_only: a.meta_only,
            xref: a.xref,
            jsonl: a.jsonl,
            quiet: false,
        }
    }
}

impl From<DumpCli> for DumpArgs {
    fn from(a: DumpCli) -> Self {
        DumpArgs {
            source: a.source.into(),
            ids: a.ids.into(),
            entry: a.entry,
            raw: a.raw,
            decode: a.decode,
            out: a.out.into(),
        }
    }
}

pub fn run(cache: Option<PathBuf>, cmd: Cmd) -> ExitCode {
    #[cfg(feature = "explorer")]
    if let Cmd::Explorer { view, packs } = cmd {
        return view.open(cache, packs);
    }
    let Some(dir) = cache else {
        eprintln!("error: no cache directory; pass --cache <dir> or set RS2_CACHE");
        return ExitCode::from(1);
    };
    let cache = match Cache::open(&dir) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    let records = Records::new(&cache);
    let result = match cmd {
        Cmd::List(a) => list::run(&records, a.index, a.archive.as_deref(), a.kind),
        Cmd::Dump(a) => dump::run(&records, &a.into()),
        Cmd::Decode(a) => {
            let args = DumpArgs {
                source: SourceArgs {
                    kind: Some(a.kind),
                    ..Default::default()
                },
                ids: a.ids.into(),
                entry: None,
                raw: false,
                decode: true,
                out: a.out.into(),
            };
            dump::run(&records, &args)
        }
        Cmd::Info(a) => records
            .meta(a.kind, a.id)
            .map_err(anyhow::Error::from)
            .and_then(|m| {
                println!("{}", serde_json::to_string_pretty(&m)?);
                Ok(Outcome::Ok)
            }),
        Cmd::Verify => verify::run(&cache),
        #[cfg(feature = "explorer")]
        Cmd::Explorer { .. } => unreachable!(),
    };
    match result {
        Ok(Outcome::Ok) => ExitCode::SUCCESS,
        Ok(Outcome::Partial) => ExitCode::from(3),
        Err(e) => {
            eprintln!("error: {e:#}");
            let usage = e.downcast_ref::<select::UsageError>().is_some();
            ExitCode::from(if usage { 1 } else { 2 })
        }
    }
}
