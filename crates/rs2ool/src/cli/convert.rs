//! The converting `rs2ool pack` subcommands: thin wrappers over `rs2convert::run`, which
//! the explorer calls too.

use std::path::Path;

use anyhow::Context;
use rs2cache::{Cache, Records};
use rs2convert::rig::ParentOverride;
use rs2convert::run::{self, Outcome};

use super::pack::{AnimArgs, Ids};

/// Set when a converted clip exceeds the validation threshold; the process
/// then exits 3.
pub static FLAGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub struct Ctx<'c> {
    pub records: Records<'c>,
    pub crc: u32,
}

/// Encode RGBA as a PNG file.
pub fn png(path: &Path, w: u32, h: u32, rgba: &[u8]) -> Result<(), String> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    image::RgbaImage::from_raw(w, h, rgba.to_vec())
        .ok_or("image size")?
        .save(path)
        .map_err(|e| e.to_string())
}

fn finish(outcome: anyhow::Result<Outcome>, dir: &Path) -> anyhow::Result<()> {
    let outcome = outcome?;
    for (kind, n, path) in run::write(&outcome, dir, png)? {
        println!("{kind}\t{n} records\t{}", path.display());
    }
    for line in &outcome.log {
        println!("{line}");
    }
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

impl<'c> Ctx<'c> {
    pub fn new(cache: &'c Cache) -> anyhow::Result<Self> {
        Ok(Self {
            crc: rs2convert::source_cache_crc(cache)?,
            records: Records::new(cache),
        })
    }

    pub fn materials(&self, dir: &Path) -> anyhow::Result<()> {
        finish(
            run::materials(&self.records, self.crc).map_err(Into::into),
            dir,
        )
    }

    pub fn textures(&self, dir: &Path) -> anyhow::Result<()> {
        finish(
            run::textures(&self.records, self.crc).map_err(Into::into),
            dir,
        )
    }

    pub fn models(&self, ids: &Ids, dir: &Path) -> anyhow::Result<()> {
        finish(
            run::models(&self.records, self.crc, ids.id).map_err(Into::into),
            dir,
        )
    }

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

    pub fn player_kit(&self, anims: &AnimArgs, dir: &Path) -> anyhow::Result<()> {
        let o = overrides(anims)?;
        finish(
            run::player_kit(&self.records, self.crc, anims.with_anims, o.as_ref())
                .map_err(Into::into),
            dir,
        )
    }

    pub fn sprites(&self, archive: Option<&str>, dir: &Path) -> anyhow::Result<()> {
        let archives = run::sprite_archives(archive)?;
        finish(
            run::sprites(&self.records, self.crc, &archives).map_err(Into::into),
            dir,
        )
    }

    pub fn floors(&self, dir: &Path) -> anyhow::Result<()> {
        finish(
            run::floors(&self.records, self.crc).map_err(Into::into),
            dir,
        )
    }

    pub fn objects(&self, ids: &Ids, dir: &Path) -> anyhow::Result<()> {
        finish(
            run::objects(&self.records, self.crc, ids.id).map_err(Into::into),
            dir,
        )
    }
}
