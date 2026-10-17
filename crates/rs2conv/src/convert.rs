//! The converting subcommands: thin wrappers over `rs2convert`.

use std::path::Path;

use anyhow::Context;
use rayon::prelude::*;
use rs2cache::{Cache, Records};
use rs2convert::palette::{Palette, corpus};
use rs2convert::texture::{TextureAtlas, images};
use rs2intermediate::record::{Record, RecordType};
use rs2pack::{Kind, PackWriter};

use crate::Ids;

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

    pub fn textures(&self, dir: &Path) -> anyhow::Result<()> {
        let atlas = TextureAtlas::build(&images(&self.records)?)?;
        write(&atlas.pack(self.crc)?, dir)?;
        write(&atlas.materials(self.crc), dir)?;
        Ok(())
    }
}
