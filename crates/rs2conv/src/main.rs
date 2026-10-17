//! `rs2conv`: convert 317 cache records into low-poly client packs.
//!
//! Exit codes: 0 ok, 1 usage, 2 cache or I/O error, 3 partial failure.

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
    /// Bake a directory of loose records (`pack.json` plus `<id>.bin`) into
    /// a hashed pack.
    Pack {
        dir: PathBuf,
        /// Output file (`.pack`), or a directory to write `<hash>.pack` in.
        #[arg(short = 'o', long = "out")]
        out: PathBuf,
    },
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
    let _ = &cli.cache;
    let result = match cli.cmd {
        Cmd::Inspect { pack } => inspect(&pack),
        Cmd::Pack { dir, out } => loose::bake(&dir, &out).map(|p| println!("{}", p.display())),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}
