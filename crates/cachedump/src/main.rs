//! `cachedump`: list, dump, decode, describe and verify a 317 cache.
//!
//! Exit codes: 0 ok, 1 usage, 2 cache error, 3 partial failure (some
//! records failed; they are listed in the manifest or the verify report).

mod dump;
mod groups;
mod list;
mod output;
mod select;
mod verify;
mod xref;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use rs2cache::{Cache, Records};
use rs2intermediate::record::RecordType;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Cache directory holding main_file_cache.dat and .idx0–4.
    #[arg(long, env = "RS2_CACHE", global = true)]
    cache: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Print the indices and archives, or the contents of one.
    List(ListArgs),
    /// Extract records to files, raw or decoded.
    Dump(DumpArgs),
    /// Decode records of one type to JSON (shorthand for `dump --type`).
    Decode(DecodeArgs),
    /// Print one record's metadata sidecar.
    Info(InfoArgs),
    /// Check every sector chain and every file; exit 3 on damage.
    Verify,
}

#[derive(Args)]
#[group(multiple = false)]
struct ListArgs {
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
pub struct IdArgs {
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
pub struct SourceArgs {
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
pub struct DumpArgs {
    #[command(flatten)]
    pub source: SourceArgs,
    #[command(flatten)]
    pub ids: IdArgs,
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
    pub out: OutArgs,
}

#[derive(Args, Clone)]
pub struct OutArgs {
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
    pub ids: IdArgs,
    #[command(flatten)]
    pub out: OutArgs,
}

#[derive(Args)]
struct InfoArgs {
    #[arg(long = "type", value_name = "TYPE")]
    kind: RecordType,
    #[arg(long)]
    id: u32,
}

/// How a run ended, mapped to the exit code.
pub enum Outcome {
    Ok,
    Partial,
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "error".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            let code = if e.use_stderr() { 1 } else { 0 };
            let _ = e.print();
            return ExitCode::from(code);
        }
    };
    let Some(dir) = cli.cache else {
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
    let result = match cli.cmd {
        Cmd::List(a) => list::run(&records, a.index, a.archive.as_deref(), a.kind),
        Cmd::Dump(a) => dump::run(&records, &a),
        Cmd::Decode(a) => {
            let args = DumpArgs {
                source: SourceArgs {
                    index: None,
                    archive: None,
                    kind: Some(a.kind),
                    region: None,
                    all_regions: false,
                    group: None,
                },
                ids: a.ids,
                entry: None,
                raw: false,
                decode: true,
                out: a.out,
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
