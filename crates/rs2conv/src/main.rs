//! `rs2conv`: convert 317 cache records into low-poly client packs.
//!
//! Exit codes: 0 ok, 1 usage, 2 cache or I/O error, 3 partial failure.

mod convert;
mod loose;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use rs2pack::PackReader;

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
    /// Print a pack's manifest and index.
    Inspect { pack: PathBuf },
    /// Build the client palette from every model colour and write the
    /// materials pack (palette and default material) plus palette.png.
    Materials(Out),
    /// Pack the 50 textures into an atlas, with a material per texture.
    Textures(Out),
    /// Convert models into a models pack.
    Models {
        #[command(flatten)]
        ids: Ids,
        #[command(flatten)]
        out: Out,
    },
    /// Bake a directory of loose records (`pack.json` plus `<id>.bin`) into
    /// a hashed pack.
    Pack {
        dir: PathBuf,
        /// Output file (`.pack`), or a directory to write `<hash>.pack` in.
        #[arg(short = 'o', long = "out")]
        out: PathBuf,
    },
}

/// Which ids to convert.
#[derive(clap::Args, Clone)]
#[group(required = true, multiple = false)]
pub struct Ids {
    /// One id.
    #[arg(long)]
    pub id: Option<u32>,
    /// Every id.
    #[arg(long)]
    pub all: bool,
}

#[derive(clap::Args)]
pub struct Out {
    /// Pack directory; packs go to <dir>/<kind>/<hash>.pack.
    #[arg(short = 'o', long = "out")]
    pub dir: PathBuf,
}

fn inspect(path: &std::path::Path) -> anyhow::Result<()> {
    let r = PackReader::open(path)?;
    println!("hash\t{}", r.hash);
    println!("manifest\t{}", serde_json::to_string(&r.manifest)?);
    println!("id\toffset\tlength\tcrc");
    for e in &r.index {
        println!("{}\t{}\t{}\t{:08x}", e.id, e.offset, e.length, e.crc);
    }
    Ok(())
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
    let result = match cli.cmd {
        Cmd::Inspect { pack } => inspect(&pack),
        Cmd::Pack { dir, out } => loose::bake(&dir, &out).map(|p| println!("{}", p.display())),
        cmd => {
            let Some(dir) = cli.cache else {
                eprintln!("error: no cache directory; pass --cache <dir> or set RS2_CACHE");
                return ExitCode::from(1);
            };
            let cache = match rs2cache::Cache::open(&dir) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::from(2);
                }
            };
            let ctx = convert::Ctx::new(&cache);
            match cmd {
                Cmd::Materials(o) => ctx.and_then(|c| c.materials(&o.dir)),
                Cmd::Textures(o) => ctx.and_then(|c| c.textures(&o.dir)),
                Cmd::Models { ids, out } => ctx.and_then(|c| c.models(&ids, &out.dir)),
                Cmd::Inspect { .. } | Cmd::Pack { .. } => unreachable!(),
            }
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}
