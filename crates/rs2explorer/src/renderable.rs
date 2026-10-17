//! What the viewports draw, from any source kind: a cache record's model
//! as the client assembles it, a model a dump directory holds as JSON, a
//! converted pack model with its pack palette, and for the 2D viewport
//! textures, sprites, fonts, interfaces and pack sprites.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rs2convert::creature::{self, Creature};
use rs2convert::object::STRINGS_DOMAIN as OBJECT_STRINGS;
use rs2convert::pipeline::ids;
use rs2intermediate::model::Model;
use rs2intermediate::record::{Record, RecordType};
use rs2intermediate::sprite::{Font, SpriteSheet};
use rs2pack::records::anim::Clip;
use rs2pack::records::material::{PALETTE_ID, decode_palette};
use rs2pack::records::model::PackModel;
use rs2pack::records::object::{self, ObjectRecord};
use rs2pack::records::rig::Rig;
use rs2pack::records::sprite::SpriteRecord;
use rs2pack::records::strings::decode as decode_strings;
use rs2pack::records::texture::{ATLAS_BASE, Atlas};
use rs2pack::{Kind, PackReader};

use crate::selection::Sel;
use crate::sources::Source;
use crate::viewport2d::{InterfaceScene, Rgba, interface_scene};

pub enum Renderable {
    CacheModel(Model),
    DumpedModel(Model),
    PackModel {
        model: PackModel,
        /// The palette of the materials pack beside the models pack, when
        /// one is there.
        palette: Option<Arc<Vec<[u8; 3]>>>,
    },
    /// A cache texture or sprite sheet; textures show on their untrimmed
    /// canvas.
    Sheet {
        sheet: SpriteSheet,
        untrimmed: bool,
    },
    Font(Font),
    Interface(InterfaceScene),
    /// A pack sprite or texture atlas.
    Image(Rgba),
    /// A cache NPC with its sequences, for playback; `model` is its rest
    /// pose with the scale applied.
    Creature {
        creature: Arc<Creature>,
        model: Model,
    },
    /// A converted creature: its model, rig and clips (by clip id).
    PackCreature {
        model: PackModel,
        palette: Option<Arc<Vec<[u8; 3]>>>,
        rig: Box<Rig>,
        clips: Vec<(u32, Clip)>,
    },
}

impl Renderable {
    /// The decoded cache-format model, for the drawers that need one.
    pub fn model(&self) -> Option<&Model> {
        match self {
            Renderable::CacheModel(m)
            | Renderable::DumpedModel(m)
            | Renderable::Creature { model: m, .. } => Some(m),
            _ => None,
        }
    }

    /// Whether the 2D viewport draws it.
    pub fn is_2d(&self) -> bool {
        matches!(
            self,
            Renderable::Sheet { .. }
                | Renderable::Font(_)
                | Renderable::Interface(_)
                | Renderable::Image(_)
        )
    }
}

/// The renderable a selection resolves to, if it has one. `record` is the
/// cache record already decoded for the inspector.
pub fn load_renderable(
    source: &Source,
    sel: &Sel,
    record: Option<&Record>,
) -> Result<Option<Renderable>, String> {
    match (source, sel) {
        (Source::Cache { records, .. }, Sel::Record(..)) => {
            let Some(record) = record else {
                return Ok(None);
            };
            match record {
                Record::Texture(sheet) => {
                    return Ok(Some(Renderable::Sheet {
                        sheet: sheet.clone(),
                        untrimmed: true,
                    }));
                }
                Record::Sprite(sheet) => {
                    return Ok(Some(Renderable::Sheet {
                        sheet: sheet.clone(),
                        untrimmed: false,
                    }));
                }
                Record::Font(font) => return Ok(Some(Renderable::Font(font.clone()))),
                Record::Interface(i) => {
                    return Ok(Some(Renderable::Interface(interface_scene(records, i))));
                }
                // An NPC whose sequences resolve plays them; otherwise it
                // shows its preview like any definition.
                Record::Npc(d) => {
                    if let Some(c) = records
                        .cache()
                        .frame_index()
                        .ok()
                        .and_then(|index| creature::npc(records, &index, d.id, &[]).ok())
                        .filter(|c| !c.sequences.is_empty())
                    {
                        let model = c.rest_model();
                        return Ok(Some(Renderable::Creature {
                            creature: Arc::new(c),
                            model,
                        }));
                    }
                }
                _ => {}
            }
            Ok(rs2convert::preview::model_for(records, record)
                .map_err(|e| e.to_string())?
                .map(Renderable::CacheModel))
        }
        (Source::DumpDir { path }, Sel::DumpFile(rel)) => {
            if dumped_model_id(rel).is_none() {
                return Ok(None);
            }
            let bytes = std::fs::read(path.join(rel)).map_err(|e| e.to_string())?;
            let m: Model = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            Ok(Some(Renderable::DumpedModel(m)))
        }
        (Source::Pack { path, reader }, Sel::PackRecord(id)) => {
            let bytes = reader
                .get(*id)
                .ok_or("no such record")?
                .map_err(|e| e.to_string())?;
            let image = |width: u16, height: u16, rgba: Vec<u8>| {
                Some(Renderable::Image(Rgba {
                    width: width.into(),
                    height: height.into(),
                    pixels: rgba,
                }))
            };
            match reader.manifest.kind {
                Kind::Models => {}
                Kind::Sprites => {
                    return Ok(SpriteRecord::decode(&bytes)
                        .ok()
                        .and_then(|s| image(s.width, s.height, s.rgba)));
                }
                Kind::Textures if id & ATLAS_BASE != 0 => {
                    return Ok(Atlas::decode(&bytes)
                        .ok()
                        .and_then(|a| image(a.width, a.height, a.rgba)));
                }
                _ => return Ok(None),
            }
            let model = PackModel::decode(&bytes).map_err(|e| e.to_string())?;
            let palette = sibling_palette(path).map(Arc::new);
            if let Some((rig, clips)) = creature_rig(*id).and_then(|r| sibling_rig(path, r)) {
                return Ok(Some(Renderable::PackCreature {
                    model,
                    palette,
                    rig: Box::new(rig),
                    clips,
                }));
            }
            Ok(Some(Renderable::PackModel { model, palette }))
        }
        _ => Ok(None),
    }
}

/// The id of a dumped model file (`model/<id>.json`).
pub fn dumped_model_id(rel: &Path) -> Option<u32> {
    let mut parts = rel.iter();
    if parts.next()? != "model" {
        return None;
    }
    let name = Path::new(parts.next()?);
    if parts.next().is_some() || name.extension()? != "json" {
        return None;
    }
    name.file_stem()?.to_str()?.parse().ok()
}

/// An object's states, in the order the objects pack lists them: the
/// location itself, then each morph child. Each has a label, its options
/// and what it draws.
#[derive(Default)]
pub struct States {
    pub labels: Vec<String>,
    pub options: Vec<Vec<String>>,
    pub renderables: Vec<Option<Arc<Renderable>>>,
}

/// The states of a cache location with a morph table, or of an object in
/// an objects pack (drawn from the models pack beside it, its options from
/// the strings pack). `None` for anything else, or a location with no
/// morph children.
pub fn load_states(source: &Source, sel: &Sel, record: Option<&Record>) -> Option<States> {
    match (source, sel, record) {
        (Source::Cache { records, .. }, _, Some(Record::Loc(d))) => {
            let children: Vec<u16> = d
                .morph
                .iter()
                .flat_map(|m| m.children.iter())
                .flatten()
                .copied()
                .collect();
            if children.is_empty() {
                return None;
            }
            let mut st = States::default();
            let mut push = |label: String, r: &Record| {
                let Record::Loc(def) = r else { return };
                st.labels.push(label);
                st.options
                    .push(def.actions.iter().flatten().cloned().collect());
                st.renderables.push(
                    rs2convert::preview::model_for(records, r)
                        .ok()
                        .flatten()
                        .map(|m| Arc::new(Renderable::CacheModel(m))),
                );
            };
            push(format!("default (loc {})", d.id), record?);
            for c in children {
                if let Ok(r) = records.decode(RecordType::Loc, c.into()) {
                    push(format!("loc {c}"), &r);
                }
            }
            Some(st)
        }
        (Source::Pack { path, reader }, Sel::PackRecord(id), _)
            if reader.manifest.kind == Kind::Objects =>
        {
            let o = ObjectRecord::decode(&reader.get(*id)?.ok()?).ok()?;
            let models = sibling_packs(path, Kind::Models);
            let palette = sibling_palette(path).map(Arc::new);
            let strings: Vec<String> = sibling_packs(path, Kind::Strings)
                .iter()
                .find_map(|p| decode_strings(&p.get(OBJECT_STRINGS << 8)?.ok()?).ok())
                .unwrap_or_default();
            let mut st = States::default();
            for (i, s) in o.states.iter().enumerate() {
                st.labels.push(format!("state {i}"));
                st.options.push(
                    s.options
                        .iter()
                        .filter(|&&o| o != object::NONE)
                        .map(|&o| {
                            strings
                                .get(o as usize)
                                .cloned()
                                .unwrap_or_else(|| format!("#{o}"))
                        })
                        .collect(),
                );
                let model = models
                    .iter()
                    .find_map(|p| PackModel::decode(&p.get(s.model)?.ok()?).ok());
                st.renderables.push(model.map(|model| {
                    Arc::new(Renderable::PackModel {
                        model,
                        palette: palette.clone(),
                    })
                }));
            }
            Some(st)
        }
        _ => None,
    }
}

/// The rig a converted creature model is skinned by: NPC `n`'s model is
/// skinned by rig `n`, the player body by the player rig.
pub fn creature_rig(model: u32) -> Option<u32> {
    match model & 0xF000_0000 {
        ids::CREATURE_MODEL => Some(model & 0x0FFF_FFFF),
        ids::PLAYER_MODEL => Some(ids::PLAYER_RIG),
        _ => None,
    }
}

/// The packs of `kind` written beside a pack (`<out>/<kind>/*.pack`),
/// sorted.
fn sibling_packs(pack: &Path, kind: Kind) -> Vec<PackReader> {
    let Some(out) = pack.parent().and_then(Path::parent) else {
        return Vec::new();
    };
    let Ok(dir) = std::fs::read_dir(out.join(kind.name())) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = dir
        .filter_map(|e| Some(e.ok()?.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "pack"))
        .collect();
    paths.sort();
    paths
        .iter()
        .filter_map(|p| PackReader::open(p).ok())
        .collect()
}

/// Rig `rig` and its clips (anim ids `rig << 16 | seq`) from the rigs and
/// anims packs beside a models pack.
pub fn sibling_rig(pack: &Path, rig: u32) -> Option<(Rig, Vec<(u32, Clip)>)> {
    let r = sibling_packs(pack, Kind::Rigs)
        .iter()
        .find_map(|p| Rig::decode(&p.get(rig)?.ok()?).ok())?;
    let mut clips = Vec::new();
    for p in sibling_packs(pack, Kind::Anims) {
        for id in p.ids().filter(|id| id >> 16 == rig).collect::<Vec<_>>() {
            if let Some(Ok(bytes)) = p.get(id)
                && let Ok(c) = Clip::decode(&bytes)
            {
                clips.push((id, c));
            }
        }
    }
    clips.sort_by_key(|(id, _)| *id);
    Some((r, clips))
}

/// The palette from a materials pack written beside a models pack: packs
/// are written to `<out>/<kind>/<hash>.pack`, so it is in
/// `<out>/materials/`.
pub fn sibling_palette(pack: &Path) -> Option<Vec<[u8; 3]>> {
    let out = pack.parent()?.parent()?;
    let mut paths: Vec<PathBuf> = std::fs::read_dir(out.join(Kind::Materials.name()))
        .ok()?
        .filter_map(|e| Some(e.ok()?.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "pack"))
        .collect();
    paths.sort();
    paths.into_iter().find_map(|p| {
        let r = PackReader::open(&p).ok()?;
        decode_palette(&r.get(PALETTE_ID)?.ok()?).ok()
    })
}

/// The same model in another source: a cache model or NPC and the pack
/// record it converts to, either way round, or a dumped model file and
/// its cache record. Returns the source index and selection.
pub fn find_match(sources: &[Source], from: usize, sel: &Sel) -> Option<(usize, Sel)> {
    let src = sources.get(from)?;
    // What the selection is, as a cache record.
    let cache_ref = match (src, sel) {
        (Source::Cache { .. }, Sel::Record(t, id)) => Some((*t, *id)),
        (Source::DumpDir { .. }, Sel::DumpFile(rel)) => {
            dumped_model_id(rel).map(|id| (RecordType::Model, id))
        }
        (Source::Pack { reader, .. }, Sel::PackRecord(id))
            if reader.manifest.kind == Kind::Models =>
        {
            pack_to_cache(*id)
        }
        _ => None,
    }?;
    let pack_id = cache_to_pack(cache_ref);
    let others = || (0..sources.len()).filter(move |&i| i != from);
    // Prefer a different kind of source: cache ↔ pack, dump → cache.
    let in_pack = |i: usize| match (&sources[i], pack_id) {
        (Source::Pack { reader, .. }, Some(id))
            if reader.manifest.kind == Kind::Models && reader.contains(id) =>
        {
            Some((i, Sel::PackRecord(id)))
        }
        _ => None,
    };
    let in_cache = |i: usize| match &sources[i] {
        Source::Cache { .. } => Some((i, Sel::Record(cache_ref.0, cache_ref.1))),
        _ => None,
    };
    match src {
        Source::Cache { .. } => others().find_map(in_pack),
        Source::Pack { .. } | Source::DumpDir { .. } => others()
            .find_map(in_cache)
            .or_else(|| others().find_map(in_pack)),
    }
}

/// The pack model record a cache record converts to.
pub fn cache_to_pack((t, id): (RecordType, u32)) -> Option<u32> {
    match t {
        RecordType::Model if id < ids::CREATURE_MODEL => Some(id),
        RecordType::Npc => Some(ids::CREATURE_MODEL | id),
        _ => None,
    }
}

/// The cache record a pack model record was converted from.
pub fn pack_to_cache(id: u32) -> Option<(RecordType, u32)> {
    match id & 0xF000_0000 {
        0 => Some((RecordType::Model, id)),
        ids::CREATURE_MODEL => Some((RecordType::Npc, id & 0x0FFF_FFFF)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dumped_model_paths() {
        assert_eq!(dumped_model_id(Path::new("model/215.json")), Some(215));
        assert_eq!(dumped_model_id(Path::new("model/215.meta.json")), None);
        assert_eq!(dumped_model_id(Path::new("npc/1.json")), None);
        assert_eq!(dumped_model_id(Path::new("model/x/1.json")), None);
    }

    #[test]
    fn cache_and_pack_ids_round_trip() {
        for r in [(RecordType::Model, 215), (RecordType::Npc, 1)] {
            assert_eq!(pack_to_cache(cache_to_pack(r).unwrap()), Some(r));
        }
        assert_eq!(cache_to_pack((RecordType::Obj, 4151)), None);
        assert_eq!(pack_to_cache(ids::KIT_MODEL | 3), None);
    }
}
