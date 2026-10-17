//! `rs2ool`: read an RS2 revision-317 cache, convert its records into
//! packs, and browse both.
//!
//! This file is just the top of the command line. The subcommands are in
//! `cli/`, and the real work happens in the `rs2ool` library (the `cache`
//! commands and the explorer) and in `rs2convert` (the `pack` conversions).
//!
//! Exit codes, the same for every subcommand:
//!
//! - 0: ok, including `--help` and `--version`.
//! - 1: usage. A bad command line, no cache for a command that needs one,
//!   or a selection that makes no sense.
//! - 2: cache or I/O error. The cache won't open, a record can't be read,
//!   an output can't be written.
//! - 3: partial failure. Some records failed (they're in the manifest or
//!   the verify report), or a converted clip went over the validation
//!   threshold. Everything that worked is still written.

mod cli;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

// The whole command line. Doc comments on clap items are the `--help`
// text, so notes for readers of this code are plain `//` comments.
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Cache directory holding main_file_cache.dat and .idx0–4.
    #[arg(long, env = "RS2_CACHE", global = true)]
    cache: Option<PathBuf>,
    // `cache` or `pack`.
    #[command(subcommand)]
    cmd: Cmd,
}

// The two command groups.
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
            // `--help` and `--version` come through here too, on stdout;
            // they're not errors. Clap's own exit code would be 2, which
            // means something else here.
            let code = if e.use_stderr() { 1 } else { 0 };
            let _ = e.print();
            return ExitCode::from(code);
        }
    };
    if !cli.cmd.is_explorer() {
        // Errors only, on stderr, unless RUST_LOG says otherwise.
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
