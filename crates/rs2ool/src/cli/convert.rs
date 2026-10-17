//! The converting `rs2ool pack` subcommands: thin wrappers over
//! `rs2convert::run`, which the explorer calls too.
//!
//! Each command runs a conversion, writes its packs and reports with
//! `rs2convert::run::write`, then prints a line per pack (kind, record
//! count, path), the conversion's log, and a line per validated clip.

use std::path::Path;

use anyhow::Context;
use rs2cache::{Cache, Records};
use rs2convert::rig::ParentOverride;
use rs2convert::run::{self, Outcome};

use super::pack::{AnimArgs, Ids};

/// Set when a converted clip goes over the validation threshold; the
/// process then exits 3. A global, since only one command runs per process.
pub static FLAGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// What every conversion needs: the cache's records and its CRC.
pub struct Ctx<'c> {
    /// The cache's decoded-record handle.
    pub records: Records<'c>,
    /// The source cache's CRC32, which every pack records.
    pub crc: u32,
}

/// Writes a `w × h` RGBA buffer to `path` as a PNG, making parent
/// directories as needed. This is the image writer `run::write` calls.
pub fn png(path: &Path, w: u32, h: u32, rgba: &[u8]) -> Result<(), String> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    image::RgbaImage::from_raw(w, h, rgba.to_vec())
        .ok_or("image size")?
        .save(path)
        .map_err(|e| e.to_string())
}

/// Writes a conversion's output under `dir` and prints the summary.
fn finish(outcome: anyhow::Result<Outcome>, dir: &Path) -> anyhow::Result<()> {
    let outcome = outcome?;
    for (kind, n, path) in run::write(&outcome, dir, png)? {
        println!("{kind}\t{n} records\t{}", path.display());
    }
    for line in &outcome.log {
        println!("{line}");
    }
    // One line per validated clip: its id, source sequence and error.
    for (_, c) in &outcome.creatures {
        for (id, _, r) in &c.clips {
            println!(
                "clip {id:#x} (seq {}): max {:.2} mean {:.2}{}",
                r.sequence,
                r.max,
                r.mean,
                if r.flagged { "  FLAGGED" } else { "" }
            );
        }
    }
    if outcome.flagged() {
        FLAGGED.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}

/// Loads the `--override` parent table, if one was given.
fn overrides(anims: &AnimArgs) -> anyhow::Result<Option<ParentOverride>> {
    anims
        .override_file
        .as_ref()
        .map(|p| {
            let bytes = std::fs::read(p).with_context(|| format!("reading {}", p.display()))?;
            serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", p.display()))
        })
        .transpose()
}

// One method per converting subcommand; each writes under `dir`.
impl<'c> Ctx<'c> {
    /// Wraps `cache`, reading its CRC.
    pub fn new(cache: &'c Cache) -> anyhow::Result<Self> {
        Ok(Self {
            crc: rs2convert::source_cache_crc(cache)?,
            records: Records::new(cache),
        })
    }

    /// `pack materials`: the palette and default material.
    pub fn materials(&self, dir: &Path) -> anyhow::Result<()> {
        finish(
            run::materials(&self.records, self.crc).map_err(Into::into),
            dir,
        )
    }

    /// `pack textures`: the texture atlas and a material per texture.
    pub fn textures(&self, dir: &Path) -> anyhow::Result<()> {
        finish(
            run::textures(&self.records, self.crc).map_err(Into::into),
            dir,
        )
    }

    /// `pack models`: one model, or all of them when `ids.id` is `None`.
    pub fn models(&self, ids: &Ids, dir: &Path) -> anyhow::Result<()> {
        finish(
            run::models(&self.records, self.crc, ids.id).map_err(Into::into),
            dir,
        )
    }

    /// `pack npc`: one NPC, plus clips for its sequences and the `extra`
    /// ones with `--with-anims`.
    pub fn npc(&self, id: u32, anims: &AnimArgs, extra: &[u16], dir: &Path) -> anyhow::Result<()> {
        let o = overrides(anims)?;
        finish(
            run::npc(
                &self.records,
                self.crc,
                id,
                anims.with_anims,
                extra,
                o.as_ref(),
            )
            .map_err(Into::into),
            dir,
        )
    }

    /// `pack player-kit`: every identity-kit part, and the rigged default
    /// body.
    pub fn player_kit(&self, anims: &AnimArgs, dir: &Path) -> anyhow::Result<()> {
        let o = overrides(anims)?;
        finish(
            run::player_kit(&self.records, self.crc, anims.with_anims, o.as_ref())
                .map_err(Into::into),
            dir,
        )
    }

    /// `pack sprites`: one sprite archive by name, or both when `None`.
    pub fn sprites(&self, archive: Option<&str>, dir: &Path) -> anyhow::Result<()> {
        let archives = run::sprite_archives(archive)?;
        finish(
            run::sprites(&self.records, self.crc, &archives).map_err(Into::into),
            dir,
        )
    }

    /// `pack floors`: floor materials and their string table.
    pub fn floors(&self, dir: &Path) -> anyhow::Result<()> {
        finish(
            run::floors(&self.records, self.crc).map_err(Into::into),
            dir,
        )
    }

    /// `pack objects`: one location, or all of them when `ids.id` is
    /// `None`.
    pub fn objects(&self, ids: &Ids, dir: &Path) -> anyhow::Result<()> {
        finish(
            run::objects(&self.records, self.crc, ids.id).map_err(Into::into),
            dir,
        )
    }
}
