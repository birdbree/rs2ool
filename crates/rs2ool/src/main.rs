//! `rs2ool`: read a RuneScape 2 revision-317 cache, convert it into
//! low-poly client packs, and browse both.
//!
//! Exit codes: 0 ok, 1 usage, 2 cache or I/O error, 3 partial failure.

mod cli;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

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
    /// List, dump, decode, describe and verify the cache, or browse it.
    #[command(subcommand)]
    Cache(cli::cache::Cmd),
    /// Convert cache records into packs, inspect and build packs, or
    /// browse them.
    #[command(subcommand)]
    Pack(cli::pack::Cmd),
}

impl Cmd {
    /// The explorer sets up its own logging.
    fn is_explorer(&self) -> bool {
        #[cfg(feature = "explorer")]
        if matches!(
            self,
            Cmd::Cache(cli::cache::Cmd::Explorer { .. })
                | Cmd::Pack(cli::pack::Cmd::Explorer { .. })
        ) {
            return true;
        }
        false
    }
}

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            let code = if e.use_stderr() { 1 } else { 0 };
            let _ = e.print();
            return ExitCode::from(code);
        }
    };
    if !cli.cmd.is_explorer() {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "error".into()),
            )
            .with_writer(std::io::stderr)
            .init();
    }
    match cli.cmd {
        Cmd::Cache(c) => cli::cache::run(cli.cache, c),
        Cmd::Pack(c) => cli::pack::run(cli.cache, c),
    }
}
