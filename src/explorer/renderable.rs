//! What the viewports draw, from any source kind.
//!
//! That's a cache record's model as the original client assembles it, a
//! model a dump directory holds as JSON, a converted pack model with its
//! pack palette, and, for the 2D viewport, textures, sprites, fonts,
//! interfaces and pack images.
//!
//! Packs don't know about each other, so anything that needs a second pack
//! (a palette, a rig, clips, strings) looks for it beside the first one.
//! Packs are written to `<out>/<kind>/<hash>.pack`, so "beside" means
//! `<out>/<other kind>/*.pack`. This module also maps cache ids to pack ids
//! and back, for compare mode.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use contentpack::records::anim::Clip;
use contentpack::records::material::{PALETTE_ID, decode_palette};
use contentpack::records::model::PackModel;
use contentpack::records::object::{self, ObjectRecord};
use contentpack::records::rig::Rig;
use contentpack::records::sprite::SpriteRecord;
use contentpack::records::strings::decode as decode_strings;
use contentpack::records::texture::{ATLAS_BASE, Atlas};
use contentpack::{Kind, PackReader};
use rs2cache::types::model::Model;
use rs2cache::types::record::{Record, RecordType};
use rs2cache::types::sprite::{Font, SpriteSheet};
use rs2convert::creature::{self, Creature};
use rs2convert::object::STRINGS_DOMAIN as OBJECT_STRINGS;
use rs2convert::pipeline::ids;

use crate::explorer::selection::Sel;
use crate::explorer::sources::Source;
use crate::explorer::viewport2d::{InterfaceScene, Rgba, interface_scene};

/// Something a viewport can draw. [`Renderable::is_2d`] says which one.
pub enum Renderable {
    /// A cache model, or a definition's model assembled from its parts.
    CacheModel(Model),
    /// A `model/<id>.json` file from a dump directory.
    DumpedModel(Model),
    /// A model from a models pack.
    PackModel {
        /// The decoded pack model.
        model: PackModel,
        /// The palette of the materials pack beside the models pack, when
        /// one is there.
        palette: Option<Arc<Vec<[u8; 3]>>>,
    },
    /// A cache texture or sprite sheet; textures show on their untrimmed
    /// canvas.
    Sheet {
        /// The decoded sheet.
        sheet: SpriteSheet,
        /// Draw sub-sprites on their full canvas rather than trimmed.
        untrimmed: bool,
    },
    /// A cache font, drawn as a glyph sheet.
    Font(Font),
    /// A cache interface, laid out ready to draw.
    Interface(InterfaceScene),
    /// A pack sprite or texture atlas.
    Image(Rgba),
    /// A cache NPC with its sequences, for playback; `model` is its rest
    /// pose with the scale applied.
    Creature {
        /// The NPC's model, rig source and resolved sequences.
        creature: Arc<Creature>,
        /// Rest pose, scaled.
        model: Model,
    },
    /// A converted creature: its model, rig and clips (by clip id).
    PackCreature {
        /// The creature's pack model.
        model: PackModel,
        /// The palette from the materials pack beside it, if any.
        palette: Option<Arc<Vec<[u8; 3]>>>,
        /// Its rig, from the rigs pack beside it.
        rig: Box<Rig>,
        /// Its clips from the anims pack beside it, as `(clip id, clip)`,
        /// sorted by id.
        clips: Vec<(u32, Clip)>,
    },
}

impl Renderable {
    /// The cache-format model, for the drawers that need one (the classic
    /// renderer, playback). `None` for pack models and 2D things.
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

/// The renderable a selection resolves to, if it has one.
///
/// `record` is the cache record already decoded for the inspector, so it
/// isn't decoded twice. Blocking, like everything that reads a source.
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
                // Falls through to the model handling below.
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
            // A creature model with its rig beside it can play; otherwise
            // it's a static model.
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

/// The id of a dumped model file (`model/<id>.json`); `None` for any
/// other path, sidecars included.
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
///
/// The three lists run in parallel, one entry per state.
#[derive(Default)]
pub struct States {
    /// What the state picker shows, e.g. `default (loc 12)` or `state 1`.
    pub labels: Vec<String>,
    /// The state's right-click options, empty ones left out.
    pub options: Vec<Vec<String>>,
    /// What each state draws; `None` when it draws nothing.
    pub renderables: Vec<Option<Arc<Renderable>>>,
}

/// The states of a cache location with a morph table, or of an object in
/// an objects pack (drawn from the models pack beside it, its options from
/// the strings pack). `None` for anything else, or a location with no
/// morph children.
pub fn load_states(source: &Source, sel: &Sel, record: Option<&Record>) -> Option<States> {
    match (source, sel, record) {
        (Source::Cache { records, .. }, _, Some(Record::Loc(d))) => {
            // Every morph child id; the `Option`s are slots the table
            // leaves empty.
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
            // Adds one location definition as a state.
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
            // The object option table: record `STRINGS_DOMAIN << 8` of the
            // first strings pack beside it that has one.
            let strings: Vec<String> = sibling_packs(path, Kind::Strings)
                .iter()
                .find_map(|p| decode_strings(&p.get(OBJECT_STRINGS << 8)?.ok()?).ok())
                .unwrap_or_default();
            let mut st = States::default();
            for (i, s) in o.states.iter().enumerate() {
                st.labels.push(format!("state {i}"));
                // Options are string ids; show `#id` for any the table lacks.
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
/// skinned by rig `n`, the player body by the player rig. `None` for any
/// other model.
pub fn creature_rig(model: u32) -> Option<u32> {
    // The top nibble of a pack model id says what it is.
    match model & 0xF000_0000 {
        ids::CREATURE_MODEL => Some(model & 0x0FFF_FFFF),
        ids::PLAYER_MODEL => Some(ids::PLAYER_RIG),
        _ => None,
    }
}

/// The packs of `kind` written beside a pack (`<out>/<kind>/*.pack`),
/// sorted by path. Ones that won't open are skipped.
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
/// anims packs beside a models pack. `None` when no rigs pack there has
/// it; no clips is fine.
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
/// are written to `<out>/<kind>/<hash>.pack`, so it's in
/// `<out>/materials/`. The first pack there (by path) with a palette wins.
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

/// The same model in another source, for compare mode: a cache model or
/// NPC and the pack record it converts to, either way round, or a dumped
/// model file and its cache record (or else its pack record). Returns the
/// source index and selection, or `None` when no other open source has it.
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
    // Every other source's index, in the order they were opened.
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
    // Any cache will do; it's assumed to be the one the pack came from.
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

/// The pack model record a cache record converts to: a model keeps its
/// id, an NPC becomes `CREATURE_MODEL | id`. `None` for other types.
pub fn cache_to_pack((t, id): (RecordType, u32)) -> Option<u32> {
    match t {
        RecordType::Model if id < ids::CREATURE_MODEL => Some(id),
        RecordType::Npc => Some(ids::CREATURE_MODEL | id),
        _ => None,
    }
}

/// The cache record a pack model record was converted from; the inverse of
/// [`cache_to_pack`]. `None` for kit parts, the player body and the like.
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
        assert_eq!(dumped_model_id(Path::new("model/42.json")), Some(42));
        assert_eq!(dumped_model_id(Path::new("model/42.meta.json")), None);
        assert_eq!(dumped_model_id(Path::new("npc/1.json")), None);
        assert_eq!(dumped_model_id(Path::new("model/x/1.json")), None);
    }

    #[test]
    fn cache_and_pack_ids_round_trip() {
        for r in [(RecordType::Model, 42), (RecordType::Npc, 7)] {
            assert_eq!(pack_to_cache(cache_to_pack(r).unwrap()), Some(r));
        }
        assert_eq!(cache_to_pack((RecordType::Obj, 9)), None);
        assert_eq!(pack_to_cache(ids::KIT_MODEL | 3), None);
    }
}
