//! The converting subcommands: thin wrappers over `rs2convert`.

use std::path::Path;

use anyhow::Context;
use rs2cache::{Cache, Records};
use rs2convert::palette::{Palette, corpus};
use rs2convert::texture::{TextureAtlas, images};
use rs2pack::PackWriter;

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

    pub fn textures(&self, dir: &Path) -> anyhow::Result<()> {
        let atlas = TextureAtlas::build(&images(&self.records)?)?;
        write(&atlas.pack(self.crc)?, dir)?;
        write(&atlas.materials(self.crc), dir)?;
        Ok(())
    }
}
