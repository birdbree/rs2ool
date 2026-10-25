//! The library behind the `rs2ool` binary.
//!
//! `rs2ool cache …` reads a 317 cache and `rs2ool pack …` turns its records
//! into packs. The command line itself lives in the binary; what's here is
//! the part the GUI needs too. The explorer's export actions call straight
//! into [`cache`], so a dump from the GUI writes byte for byte what the CLI
//! writes.
//!
//! # Modules
//!
//! - [`cache`]: the `rs2ool cache` commands, with their option structs
//!   ([`cache::DumpArgs`] and friends).
//!   - [`cache::list`]: `list`, the indices, archives and record ids.
//!   - [`cache::dump`]: `dump` and `decode`, raw or decoded files.
//!   - [`cache::verify`]: `verify`, sector chains, inflation and the
//!     versionlist check.
//!   - [`cache::select`]: turns `--id`/`--range`/`--all` into ids, and the
//!     [`cache::select::UsageError`] that maps to exit code 1.
//!   - [`cache::groups`]: what `--group npc:<id>`, `item:<id>` and
//!     `player-kit` pull in.
//!   - [`cache::xref`]: reverse references for `--xref`.
//!   - [`cache::output`]: the output directory, sidecars and
//!     `manifest.json`.
//! - [`render`]: drawing models with the classic renderer into PNG, APNG
//!   and GIF images, behind `rs2ool cache render`.
//! - `caption`: captioning candidate sequences with a vision model on a
//!   server you run, behind `rs2ool cache caption`. Only there with the
//!   `caption` feature (on by default), and off until it's turned on in
//!   the settings.
//! - `settings`: what the explorer and `cache caption` remember between
//!   runs, in `settings.json`.
//! - `explorer`: the Bevy GUI behind `rs2ool cache explorer` and
//!   `rs2ool pack explorer`. It's only there with the `explorer` feature
//!   (on by default), and its own docs have the module map.
//!
//! # Example
//!
//! Dump every NPC definition as JSON with a sidecar each, the same as
//! `rs2ool cache dump --type npc --all --meta -o out/`:
//!
//! ```no_run
//! use rs2cache::types::record::RecordType;
//! use rs2cache::{Cache, Records};
//! use rs2ool::cache::{DumpArgs, IdArgs, OutArgs, Outcome, SourceArgs, dump};
//!
//! # fn main() -> anyhow::Result<()> {
//! let cache = Cache::open(std::path::Path::new("/path/to/cache"))?;
//! let records = Records::new(&cache);
//! let args = DumpArgs {
//!     source: SourceArgs { kind: Some(RecordType::Npc), ..Default::default() },
//!     ids: IdArgs { all: true, ..Default::default() },
//!     out: OutArgs { dir: "out".into(), meta: true, ..Default::default() },
//!     ..Default::default()
//! };
//! if dump::run(&records, &args)? == Outcome::Partial {
//!     eprintln!("some records failed; see out/manifest.json");
//! }
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]

pub mod cache;
#[cfg(feature = "caption")]
pub mod caption;
#[cfg(feature = "explorer")]
pub mod explorer;
pub mod render;
#[cfg(any(feature = "explorer", feature = "caption"))]
pub mod settings;
