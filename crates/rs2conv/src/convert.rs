//! The converting subcommands: thin wrappers over `rs2convert`.

use std::path::Path;

use anyhow::Context;
use rayon::prelude::*;
use rs2cache::{Cache, Records};
use rs2convert::palette::{Palette, corpus};
use rs2convert::texture::{TextureAtlas, images};
use rs2intermediate::record::{Record, RecordType};
use rs2pack::{Kind, PackWriter};

use rs2convert::pipeline::{self, CreatureOutput, ids as pids};
use rs2convert::rig::ParentOverride;

use crate::{AnimArgs, Ids};

/// Set when a converted clip exceeds the validation threshold; the process
/// then exits 3.
pub static FLAGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub struct Ctx<'c> {
    pub records: Records<'c>,
    pub crc: u32,
}

fn write(w: &PackWriter, dir: &Path) -> anyhow::Result<()> {
    let path = w.write_to(dir)?;
    println!("{}\t{} records\t{}", w.kind(), w.len(), path.display());
    Ok(())
}

fn png(path: &Path, (w, h, rgba): (u32, u32, Vec<u8>)) -> anyhow::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    image::RgbaImage::from_raw(w, h, rgba)
        .context("image size")?
        .save(path)
        .with_context(|| format!("writing {}", path.display()))
}

impl<'c> Ctx<'c> {
    pub fn new(cache: &'c Cache) -> anyhow::Result<Self> {
        Ok(Self {
            crc: rs2convert::source_cache_crc(cache)?,
            records: Records::new(cache),
        })
    }

    /// Build the palette from the cache's model colours.
    pub fn palette(&self) -> anyhow::Result<(Palette, rs2convert::palette::PaletteError)> {
        let corpus = corpus(&self.records)?;
        let mut p = Palette::build(&corpus);
        p.prime(&corpus);
        let e = p.error(&corpus);
        Ok((p, e))
    }

    pub fn materials(&self, dir: &Path) -> anyhow::Result<()> {
        let (p, e) = self.palette()?;
        write(&p.pack(self.crc), dir)?;
        png(&dir.join("palette.png"), p.swatch(8))?;
        std::fs::write(dir.join("palette.json"), serde_json::to_vec_pretty(&e)?)?;
        println!(
            "palette: {} entries over {} colours ({} faces); error mean {:.2}, max {:.2} RGB units",
            p.rgb.len(),
            e.colours,
            e.faces,
            e.mean,
            e.max
        );
        Ok(())
    }

    pub fn models(&self, ids: &Ids, dir: &Path) -> anyhow::Result<()> {
        let (palette, _) = self.palette()?;
        let list = match ids.id {
            Some(id) => vec![id],
            None => self.records.ids(RecordType::Model)?,
        };
        let converted: Vec<anyhow::Result<(u32, Vec<u8>)>> = list
            .par_iter()
            .map(|&id| {
                let Record::Model(m) = self.records.decode(RecordType::Model, id)? else {
                    unreachable!()
                };
                let p = rs2convert::model::convert(&m, &palette)?;
                Ok((id, p.encode()?))
            })
            .collect();
        let mut w = PackWriter::new(Kind::Models, self.crc);
        for r in converted {
            let (id, rec) = r?;
            w.insert(id, rec);
        }
        write(&w, dir)
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

    /// Write a creature's validation report and fold it into the summary.
    fn report(&self, dir: &Path, key: &str, out: &CreatureOutput) -> anyhow::Result<()> {
        let cdir = dir.join(key);
        std::fs::create_dir_all(&cdir)?;
        let mut v = serde_json::to_vec_pretty(&out.validation())?;
        v.push(b'\n');
        std::fs::write(cdir.join("validation.json"), v)?;
        let summary_path = dir.join("validation_summary.json");
        let mut summary: std::collections::BTreeMap<String, serde_json::Value> =
            std::fs::read(&summary_path)
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default();
        summary.insert(
            key.to_owned(),
            serde_json::json!({ "name": out.creature.name, "max": out.max_error(), "flagged": out.flagged(), "bones": out.rig.bones.len() }),
        );
        let mut v = serde_json::to_vec_pretty(&summary)?;
        v.push(b'\n');
        std::fs::write(summary_path, v)?;
        for (id, _, r) in &out.clips {
            println!(
                "clip {id:#x} (seq {}): max {:.2} mean {:.2}{}",
                r.sequence,
                r.max,
                r.mean,
                if r.flagged { "  FLAGGED" } else { "" }
            );
        }
        if out.flagged() {
            FLAGGED.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        Ok(())
    }

    pub fn npc(&self, id: u32, anims: &AnimArgs, extra: &[u16], dir: &Path) -> anyhow::Result<()> {
        let (palette, _) = self.palette()?;
        let index = self.records.cache().frame_index()?;
        let creature = rs2convert::creature::npc(&self.records, &index, id, extra)?;
        let out = pipeline::convert(
            creature,
            &palette,
            id,
            anims.with_anims,
            Self::overrides(anims)?.as_ref(),
        )?;
        let (mut models, mut rigs, mut clips) = pipeline::writers(self.crc);
        out.write(
            pids::CREATURE_MODEL | id,
            id,
            &mut models,
            &mut rigs,
            &mut clips,
        )?;
        write(&models, dir)?;
        write(&rigs, dir)?;
        if anims.with_anims {
            write(&clips, dir)?;
        }
        println!("{}: {} bones", out.creature.name, out.rig.bones.len());
        self.report(dir, &format!("npc_{id}"), &out)
    }

    pub fn player_kit(&self, anims: &AnimArgs, dir: &Path) -> anyhow::Result<()> {
        let (palette, _) = self.palette()?;
        let index = self.records.cache().frame_index()?;
        let (parts, body) = pipeline::player_kit(&self.records, &index)?;
        let out = pipeline::convert(
            body,
            &palette,
            pids::PLAYER_RIG,
            anims.with_anims,
            Self::overrides(anims)?.as_ref(),
        )?;
        let (mut models, mut rigs, mut clips) = pipeline::writers(self.crc);
        out.write(
            pids::PLAYER_MODEL,
            pids::PLAYER_RIG,
            &mut models,
            &mut rigs,
            &mut clips,
        )?;
        let mut meta = std::collections::BTreeMap::new();
        for (part, m) in &parts {
            let rec = rs2convert::model::convert(m, &palette)?;
            models.insert(pids::KIT_MODEL | part.idk, rec.encode()?);
            meta.insert(
                (pids::KIT_MODEL | part.idk).to_string(),
                serde_json::to_value(part)?,
            );
        }
        models.set_extra("kit_parts", serde_json::to_value(meta)?);
        write(&models, dir)?;
        write(&rigs, dir)?;
        if anims.with_anims {
            write(&clips, dir)?;
        }
        println!(
            "player kit: {} parts; body rig {} bones",
            parts.len(),
            out.rig.bones.len()
        );
        self.report(dir, "player", &out)
    }

    pub fn sprites(&self, archive: Option<&str>, dir: &Path) -> anyhow::Result<()> {
        use rs2cache::archive::{MEDIA, TITLE, archive_by_name};
        let archives = match archive {
            None => vec![MEDIA, TITLE],
            Some(name) => match archive_by_name(name) {
                Some(n) if n == MEDIA || n == TITLE => vec![n],
                _ => anyhow::bail!("--archive must be media or title"),
            },
        };
        write(
            &rs2convert::sprite::pack(&self.records, &archives, self.crc)?,
            dir,
        )
    }

    pub fn objects(&self, ids: &Ids, dir: &Path) -> anyhow::Result<()> {
        let (palette, _) = self.palette()?;
        let list = match ids.id {
            Some(id) => vec![id],
            None => self.records.ids(RecordType::Loc)?,
        };
        let p = rs2convert::object::convert(&self.records, &list, &palette, self.crc)?;
        for w in [&p.models, &p.objects, &p.kits, &p.strings] {
            if !w.is_empty() {
                write(w, dir)?;
            }
        }
        Ok(())
    }

    pub fn floors(&self, dir: &Path) -> anyhow::Result<()> {
        let (palette, _) = self.palette()?;
        let (materials, strings) = rs2convert::floor::packs(&self.records, &palette, self.crc)?;
        write(&materials, dir)?;
        write(&strings, dir)
    }

    pub fn textures(&self, dir: &Path) -> anyhow::Result<()> {
        let atlas = TextureAtlas::build(&images(&self.records)?)?;
        write(&atlas.pack(self.crc)?, dir)?;
        write(&atlas.materials(self.crc), dir)?;
        Ok(())
    }
}
