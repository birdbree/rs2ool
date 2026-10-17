//! The conversion commands as library calls, shared by `rs2conv` and the
//! explorer so a conversion started from either produces byte-identical
//! packs. Each command returns an [`Outcome`]; [`write`] puts it in a pack
//! directory.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use rs2cache::types::record::{Record, RecordType};
use rs2cache::{Records, archive};
use rs2pack::{Kind, PackWriter};

use crate::palette::{Palette, PaletteError, corpus};
use crate::pipeline::{self, CreatureOutput, ids as pids};
use crate::rig::ParentOverride;
use crate::texture::{TextureAtlas, images};
use crate::{ConvertError, model};

/// What a conversion produced.
#[derive(Default)]
pub struct Outcome {
    pub packs: Vec<PackWriter>,
    /// Side files, relative to the pack directory (for example
    /// `palette.json`).
    pub files: Vec<(PathBuf, Vec<u8>)>,
    /// RGBA images to save as PNG, relative to the pack directory.
    pub images: Vec<(PathBuf, u32, u32, Vec<u8>)>,
    /// Converted creatures, keyed for their `<key>/validation.json`.
    pub creatures: Vec<(String, CreatureOutput)>,
    /// One line per notable result, for the user.
    pub log: Vec<String>,
}

impl Outcome {
    /// Whether any converted clip exceeded the validation threshold.
    pub fn flagged(&self) -> bool {
        self.creatures.iter().any(|(_, c)| c.flagged())
    }
}

/// Build the palette from the cache's model colours.
pub fn palette(records: &Records) -> Result<(Palette, PaletteError), ConvertError> {
    let corpus = corpus(records)?;
    let mut p = Palette::build(&corpus);
    p.prime(&corpus);
    let e = p.error(&corpus);
    Ok((p, e))
}

fn json(v: &impl serde::Serialize) -> Vec<u8> {
    let mut b = serde_json::to_vec_pretty(v).expect("serialises");
    b.push(b'\n');
    b
}

/// The palette's materials pack, swatch and error report.
pub fn materials(records: &Records, crc: u32) -> Result<Outcome, ConvertError> {
    let (p, e) = palette(records)?;
    let (w, h, rgba) = p.swatch(8);
    Ok(Outcome {
        packs: vec![p.pack(crc)],
        files: vec![("palette.json".into(), json(&e))],
        images: vec![("palette.png".into(), w, h, rgba)],
        log: vec![format!(
            "palette: {} entries over {} colours ({} faces); error mean {:.2}, max {:.2} RGB units",
            p.rgb.len(),
            e.colours,
            e.faces,
            e.mean,
            e.max
        )],
        ..Default::default()
    })
}

/// The texture atlas pack and texture materials.
pub fn textures(records: &Records, crc: u32) -> Result<Outcome, ConvertError> {
    let atlas = TextureAtlas::build(&images(records)?)?;
    Ok(Outcome {
        packs: vec![atlas.pack(crc)?, atlas.materials(crc)],
        ..Default::default()
    })
}

/// Models: one id, or every model.
pub fn models(records: &Records, crc: u32, id: Option<u32>) -> Result<Outcome, ConvertError> {
    let (palette, _) = palette(records)?;
    let list = match id {
        Some(id) => vec![id],
        None => records.ids(RecordType::Model)?,
    };
    let converted: Vec<Result<(u32, Vec<u8>), ConvertError>> = list
        .par_iter()
        .map(|&id| {
            let Record::Model(m) = records.decode(RecordType::Model, id)? else {
                unreachable!()
            };
            Ok((id, model::convert(&m, &palette)?.encode()?))
        })
        .collect();
    let mut w = PackWriter::new(Kind::Models, crc);
    for r in converted {
        let (id, rec) = r?;
        w.insert(id, rec);
    }
    Ok(Outcome {
        packs: vec![w],
        ..Default::default()
    })
}

/// One NPC: merged model, rig and (with `with_anims`) validated clips.
pub fn npc(
    records: &Records,
    crc: u32,
    id: u32,
    with_anims: bool,
    extra: &[u16],
    overrides: Option<&ParentOverride>,
) -> Result<Outcome, ConvertError> {
    let (palette, _) = palette(records)?;
    let index = records.cache().frame_index()?;
    let creature = crate::creature::npc(records, &index, id, extra)?;
    let out = pipeline::convert(creature, &palette, id, with_anims, overrides)?;
    let (mut models, mut rigs, mut clips) = pipeline::writers(crc);
    out.write(
        pids::CREATURE_MODEL | id,
        id,
        &mut models,
        &mut rigs,
        &mut clips,
    )?;
    let mut packs = vec![models, rigs];
    if with_anims {
        packs.push(clips);
    }
    let log = vec![format!(
        "{}: {} bones",
        out.creature.name,
        out.rig.bones.len()
    )];
    Ok(Outcome {
        packs,
        creatures: vec![(format!("npc_{id}"), out)],
        log,
        ..Default::default()
    })
}

/// Every identity-kit part, and the default body rigged with the stance
/// sequences.
pub fn player_kit(
    records: &Records,
    crc: u32,
    with_anims: bool,
    overrides: Option<&ParentOverride>,
) -> Result<Outcome, ConvertError> {
    let (palette, _) = palette(records)?;
    let index = records.cache().frame_index()?;
    let (parts, body) = pipeline::player_kit(records, &index)?;
    let out = pipeline::convert(body, &palette, pids::PLAYER_RIG, with_anims, overrides)?;
    let (mut models, mut rigs, mut clips) = pipeline::writers(crc);
    out.write(
        pids::PLAYER_MODEL,
        pids::PLAYER_RIG,
        &mut models,
        &mut rigs,
        &mut clips,
    )?;
    let mut meta = BTreeMap::new();
    for (part, m) in &parts {
        models.insert(
            pids::KIT_MODEL | part.idk,
            model::convert(m, &palette)?.encode()?,
        );
        meta.insert(
            (pids::KIT_MODEL | part.idk).to_string(),
            serde_json::to_value(part).expect("serialises"),
        );
    }
    models.set_extra("kit_parts", serde_json::to_value(meta).expect("serialises"));
    let mut packs = vec![models, rigs];
    if with_anims {
        packs.push(clips);
    }
    let log = vec![format!(
        "player kit: {} parts; body rig {} bones",
        parts.len(),
        out.rig.bones.len()
    )];
    Ok(Outcome {
        packs,
        creatures: vec![("player".into(), out)],
        log,
        ..Default::default()
    })
}

/// Media and/or title sprites, and fonts with the title archive.
pub fn sprites(records: &Records, crc: u32, archives: &[u8]) -> Result<Outcome, ConvertError> {
    Ok(Outcome {
        packs: vec![crate::sprite::pack(records, archives, crc)?],
        ..Default::default()
    })
}

/// The archives `sprites` reads: media and title, or one of them by name.
pub fn sprite_archives(name: Option<&str>) -> Result<Vec<u8>, ConvertError> {
    match name {
        None => Ok(vec![archive::MEDIA, archive::TITLE]),
        Some(n) => match archive::archive_by_name(n) {
            Some(a) if a == archive::MEDIA || a == archive::TITLE => Ok(vec![a]),
            _ => Err(ConvertError::Invalid(
                "--archive must be media or title".into(),
            )),
        },
    }
}

/// Floor materials and their string table.
pub fn floors(records: &Records, crc: u32) -> Result<Outcome, ConvertError> {
    let (palette, _) = palette(records)?;
    let (m, s) = crate::floor::packs(records, &palette, crc)?;
    Ok(Outcome {
        packs: vec![m, s],
        ..Default::default()
    })
}

/// Objects: one id, or every definition.
pub fn objects(records: &Records, crc: u32, id: Option<u32>) -> Result<Outcome, ConvertError> {
    let (palette, _) = palette(records)?;
    let list = match id {
        Some(id) => vec![id],
        None => records.ids(RecordType::Loc)?,
    };
    let p = crate::object::convert(records, &list, &palette, crc)?;
    let packs = [p.models, p.objects, p.kits, p.strings]
        .into_iter()
        .filter(|w| !w.is_empty())
        .collect();
    Ok(Outcome {
        packs,
        ..Default::default()
    })
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> ConvertError + '_ {
    move |e| ConvertError::Invalid(format!("{}: {e}", path.display()))
}

/// Write an outcome into a pack directory: each pack at
/// `<dir>/<kind>/<hash>.pack`, side files, images (through `png`, which
/// encodes RGBA to a file), each creature's `<key>/validation.json`, and
/// the merged `validation_summary.json`. Returns the packs written as
/// `(kind, records, path)`.
pub fn write(
    outcome: &Outcome,
    dir: &Path,
    png: impl Fn(&Path, u32, u32, &[u8]) -> Result<(), String>,
) -> Result<Vec<(Kind, usize, PathBuf)>, ConvertError> {
    let mut written = Vec::new();
    for w in &outcome.packs {
        let path = w
            .write_to(dir)
            .map_err(|e| ConvertError::Invalid(e.to_string()))?;
        written.push((w.kind(), w.len(), path));
    }
    for (rel, bytes) in &outcome.files {
        let p = dir.join(rel);
        std::fs::create_dir_all(dir).map_err(io(dir))?;
        std::fs::write(&p, bytes).map_err(io(&p))?;
    }
    for (rel, w, h, rgba) in &outcome.images {
        let p = dir.join(rel);
        png(&p, *w, *h, rgba)
            .map_err(|e| ConvertError::Invalid(format!("{}: {e}", p.display())))?;
    }
    if !outcome.creatures.is_empty() {
        let summary_path = dir.join("validation_summary.json");
        let mut summary: BTreeMap<String, serde_json::Value> = std::fs::read(&summary_path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        for (key, c) in &outcome.creatures {
            let cdir = dir.join(key);
            std::fs::create_dir_all(&cdir).map_err(io(&cdir))?;
            let p = cdir.join("validation.json");
            std::fs::write(&p, json(&c.validation())).map_err(io(&p))?;
            summary.insert(
                key.clone(),
                serde_json::json!({ "name": c.creature.name, "max": c.max_error(), "flagged": c.flagged(), "bones": c.rig.bones.len() }),
            );
        }
        std::fs::write(&summary_path, json(&summary)).map_err(io(&summary_path))?;
    }
    Ok(written)
}
