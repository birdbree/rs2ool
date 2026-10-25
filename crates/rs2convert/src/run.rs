//! The conversion commands as library calls, shared by `rs2ool pack` and the
//! explorer so a conversion started from either produces byte-identical
//! packs.
//!
//! Each command returns an [`Outcome`] and doesn't touch the disk;
//! [`write()`] puts it in a pack directory. Every command takes the source
//! cache CRC (from [`crate::source_cache_crc`]) to stamp into its packs.
//! The ones that need colours build the palette from scratch each time,
//! which is deterministic but not quick.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use contentpack::{Kind, PackWriter};
use rayon::prelude::*;
use rs2cache::types::record::{Record, RecordType};
use rs2cache::{Records, archive};

use crate::palette::{Palette, PaletteError, corpus};
use crate::pipeline::{self, CreatureOutput, ids as pids};
use crate::rig::ParentOverride;
use crate::texture::{TextureAtlas, images};
use crate::{ConvertError, model};

/// What a conversion produced.
#[derive(Default)]
pub struct Outcome {
    /// The packs to write, each into its kind's directory.
    pub packs: Vec<PackWriter>,
    /// Side files, relative to the pack directory (for example
    /// `palette.json`).
    pub files: Vec<(PathBuf, Vec<u8>)>,
    /// RGBA images to save as PNG, relative to the pack directory.
    pub images: Vec<(PathBuf, u32, u32, Vec<u8>)>,
    /// Converted creatures, keyed for their `<key>/validation.json`.
    pub creatures: Vec<(String, CreatureOutput)>,
    /// Human-readable summary lines, one per notable result.
    pub log: Vec<String>,
}

impl Outcome {
    /// Whether any converted clip exceeded the validation threshold.
    pub fn flagged(&self) -> bool {
        self.creatures.iter().any(|(_, c)| c.flagged())
    }
}

/// Build the palette from the cache's model colours, primed for every
/// corpus colour, with its quantisation error.
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

/// The palette's materials pack, plus `palette.json` (the error report)
/// and `palette.png` (a swatch).
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

/// Models: one id, or every model when `id` is `None`. Converted in
/// parallel, written sorted by id.
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
///
/// `extra` adds sequence ids on top of the NPC's own stance and walk
/// sequences. `overrides` replaces inferred bone parents.
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
/// sequences. Part metadata goes in the models pack manifest under
/// `kit_parts`.
pub fn player_kit(
    records: &Records,
    crc: u32,
    with_anims: bool,
    overrides: Option<&ParentOverride>,
) -> Result<Outcome, ConvertError> {
    let (palette, _) = palette(records)?;
    let index = records.cache().frame_index()?;
    let kit = pipeline::player_kit(records, &index)?;
    let parts = kit.parts;
    let (mut models, mut rigs, mut clips) = pipeline::writers(crc);
    let mut creatures = Vec::new();
    // Each default body with its model and rig ids and its report key.
    let bodies = [
        Some((kit.male, pids::PLAYER_MODEL, pids::PLAYER_RIG, "player")),
        kit.female.map(|f| {
            (
                f,
                pids::PLAYER_FEMALE_MODEL,
                pids::PLAYER_FEMALE_RIG,
                "player_female",
            )
        }),
    ];
    for (body, model_id, rig_id, key) in bodies.into_iter().flatten() {
        let out = pipeline::convert(body, &palette, rig_id, with_anims, overrides)?;
        out.write(model_id, rig_id, &mut models, &mut rigs, &mut clips)?;
        creatures.push((key.to_owned(), out));
    }
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
    let bodies: Vec<String> = creatures
        .iter()
        .map(|(key, c)| format!("{key} rig {} bones", c.rig.bones.len()))
        .collect();
    let log = vec![format!(
        "player kit: {} parts; {}",
        parts.len(),
        bodies.join(", ")
    )];
    Ok(Outcome {
        packs,
        creatures,
        log,
        ..Default::default()
    })
}

/// Sprites from the archives in `archives` (media and/or title), plus the
/// fonts when the title archive is one of them.
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

/// Floor materials and their name string table.
pub fn floors(records: &Records, crc: u32) -> Result<Outcome, ConvertError> {
    let (palette, _) = palette(records)?;
    let (m, s) = crate::floor::packs(records, &palette, crc)?;
    Ok(Outcome {
        packs: vec![m, s],
        ..Default::default()
    })
}

/// Objects: one id, or every definition when `id` is `None`. Packs that
/// come out empty are left out.
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

/// One item definition, or every item definition: its `items` record,
/// its inventory and wear models, and the string table.
pub fn items(records: &Records, crc: u32, id: Option<u32>) -> Result<Outcome, ConvertError> {
    let (palette, _) = palette(records)?;
    let list = match id {
        Some(id) => vec![id],
        None => records.ids(RecordType::Obj)?,
    };
    let p = crate::item::convert(records, &list, &palette, crc)?;
    let packs = [p.models, p.items, p.strings]
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

/// Write an outcome into a pack directory.
///
/// Each pack goes to `<dir>/<kind>/<hash>.pack`, then the side files, the
/// images (through `png`, which you give to encode RGBA to a file, so this
/// crate doesn't need an image library), each creature's
/// `<key>/validation.json`, and `validation_summary.json`. The summary is
/// merged: entries for other creatures already in it are kept, and one
/// that can't be parsed is started over. Returns the packs written as
/// `(kind, record count, path)`.
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
