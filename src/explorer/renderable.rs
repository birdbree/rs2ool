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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use contentpack::records::anim::Clip;
use contentpack::records::material::{PALETTE_ID, decode_palette};
use contentpack::records::model::PackModel;
use contentpack::records::object::{self, ObjectRecord};
use contentpack::records::rig::Rig;
use contentpack::records::sprite::SpriteRecord;
use contentpack::records::strings::decode as decode_strings;
use contentpack::records::texture::{ATLAS_BASE, Atlas};
use contentpack::{Kind, PackReader};
use rs2cache::Records;
use rs2cache::anim::FrameIndex;
use rs2cache::types::config::{LocDef, Morph, NpcDef};
use rs2cache::types::model::Model;
use rs2cache::types::record::{Record, RecordType};
use rs2cache::types::sprite::{Font, SpriteSheet};
use rs2convert::creature::{self, Creature};
use rs2convert::infer::{Candidate, SequenceIndex};
use rs2convert::morph::Variable;
use rs2convert::object::STRINGS_DOMAIN as OBJECT_STRINGS;
use rs2convert::pipeline::ids;
use rs2convert::rig::{self, InferredRig};

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
        /// The NPC's model, rig source and resolved sequences: the ones
        /// it names, then the inferred ones.
        creature: Arc<Creature>,
        /// Rest pose, scaled.
        model: Model,
        /// The inferred sequences among `creature`'s, best first, with
        /// their evidence and role (see [`rs2convert::infer`]).
        inferred: Vec<Candidate>,
        /// The rig inferred from `creature`, as conversion would infer it,
        /// for showing its bones before converting; `None` when it can't
        /// be rigged (no vertex labels).
        rig: Option<Arc<InferredRig>>,
        /// Whether the inferred sequences' roles come from a learned role
        /// tree rather than the hand-tuned rules.
        learned: bool,
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

/// Each open cache's sequence index, built on first use: it decodes every
/// frame pack, so it's worth keeping.
fn sequence_index(
    path: &Path,
    records: &Records,
    index: &FrameIndex,
) -> Option<Arc<SequenceIndex>> {
    static INDEXES: OnceLock<Mutex<BTreeMap<PathBuf, Arc<SequenceIndex>>>> = OnceLock::new();
    let mut map = INDEXES.get_or_init(Default::default).lock().ok()?;
    if let Some(i) = map.get(path) {
        return Some(i.clone());
    }
    let i = Arc::new(SequenceIndex::build(records.cache(), records, index).ok()?);
    map.insert(path.to_path_buf(), i.clone());
    Some(i)
}

/// NPC `id`'s creature with every inferred sequence added after the named
/// ones, grouped by role, and those candidates (see
/// [`rs2convert::infer::npc_candidates`]), and whether a learned role tree
/// gave the roles. Falls back to `named` (the creature with only its named
/// sequences) when there are none or they won't load.
fn with_inferred(
    source: &Source,
    records: &Records,
    index: &FrameIndex,
    id: u32,
    named: Creature,
) -> (Creature, Vec<Candidate>, bool) {
    let Source::Cache { path, .. } = source else {
        return (named, Vec::new(), false);
    };
    let Some(seqs) = sequence_index(path, records, index) else {
        return (named, Vec::new(), false);
    };
    match role_trees(records) {
        Some(trees) => {
            // The NPC's own family tree if there is one, else the global.
            let names: Vec<u32> = named.sequences.iter().map(|s| s.def.id).collect();
            let t = trees.for_family(seqs.family_of(&names));
            let (c, inferred) =
                rs2convert::infer::npc_candidates_by(&seqs, records, index, id, named, &|m| {
                    t.role(m)
                });
            (c, inferred, true)
        }
        None => {
            let (c, inferred) = rs2convert::infer::npc_candidates(&seqs, records, index, id, named);
            (c, inferred, false)
        }
    }
}

/// The role trees to judge roles by: the ones `rs2ool cache classify
/// --save` saved, if there are any and they parse, else the built-in ones.
/// Read on every NPC load, so newly saved trees show on the next. On a
/// cache other than the one they were learned on, only the global tree
/// counts, since family numbers don't carry over (see
/// [`RoleTrees::for_cache`](rs2convert::classify::RoleTrees::for_cache)).
fn role_trees(records: &Records) -> Option<rs2convert::classify::RoleTrees> {
    let saved = crate::settings::role_tree_path()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|bytes| match serde_json::from_slice(&bytes) {
            Ok(t) => Some(t),
            Err(e) => {
                tracing::warn!("role trees: {e}");
                None
            }
        });
    let trees = saved.unwrap_or_else(rs2convert::classify::RoleTrees::built_in);
    let crc = rs2convert::source_cache_crc(records.cache()).ok()?;
    Some(trees.for_cache(crc))
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
                // A spot animation with a sequence plays it, posed as the
                // original client poses it; one without shows its model.
                Record::SpotAnim(d) => {
                    if let Some(index) = records.cache().frame_index().ok()
                        && let Some(c) = creature::spotanim(records, &index, d.id)
                            .ok()
                            .filter(|c| !c.sequences.is_empty())
                    {
                        let model = c.rest_model();
                        let rig = rig::infer(&c, None).ok().map(Arc::new);
                        return Ok(Some(Renderable::Creature {
                            creature: Arc::new(c),
                            model,
                            inferred: Vec::new(),
                            rig,
                            learned: false,
                        }));
                    }
                }
                // An NPC whose sequences resolve plays them; otherwise it
                // shows its preview like any definition.
                Record::Npc(d) => {
                    if let Some(index) = records.cache().frame_index().ok()
                        && let Some(c) = creature::npc(records, &index, d.id, &[])
                            .ok()
                            .filter(|c| !c.sequences.is_empty())
                    {
                        let (c, inferred, learned) =
                            with_inferred(source, records, &index, d.id, c);
                        let model = c.rest_model();
                        let rig = rig::infer(&c, None).ok().map(Arc::new);
                        return Ok(Some(Renderable::Creature {
                            creature: Arc::new(c),
                            model,
                            inferred,
                            rig,
                            learned,
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
    /// For a cache location or NPC, the morph table behind its states.
    pub morph: Option<MorphView>,
}

/// A morph table and how its slots map onto the states.
#[derive(Debug, Clone)]
pub struct MorphView {
    /// The table.
    pub table: Morph,
    /// The variable it reads; `None` when its varbit won't decode.
    pub variable: Option<Variable>,
    /// For each slot of the table, the state showing its child; `None` for
    /// an empty slot or a child that won't load.
    pub slots: Vec<Option<usize>>,
}

impl MorphView {
    /// The state the original client would show with the variable's varp
    /// holding `contents`, or `None` when it would draw nothing.
    pub fn state_for(&self, contents: i32) -> Option<usize> {
        let value = self.variable?.read(contents);
        self.slots
            .get(rs2convert::morph::pick(&self.table, value)?)
            .copied()
            .flatten()
    }
}

/// States from a morph table: the definition itself as state 0, labelled
/// `default (<kind> <id>)`, then each child that loads, labelled
/// `<kind> <id>`. `child` loads one child's options and renderable.
fn morph_states(
    records: &Records,
    kind: &str,
    id: u32,
    options: Vec<String>,
    own: Option<Arc<Renderable>>,
    table: &Morph,
    mut child: impl FnMut(u16) -> Option<(Vec<String>, Option<Arc<Renderable>>)>,
) -> Option<States> {
    // A table with no children has no states to pick between.
    table.children.iter().flatten().next()?;
    let mut st = States {
        labels: vec![format!("default ({kind} {id})")],
        options: vec![options],
        renderables: vec![own],
        morph: None,
    };
    let mut slots = Vec::with_capacity(table.children.len());
    for c in &table.children {
        let state = c.and_then(|c| {
            let (options, r) = child(c)?;
            st.labels.push(format!("{kind} {c}"));
            st.options.push(options);
            st.renderables.push(r);
            Some(st.labels.len() - 1)
        });
        slots.push(state);
    }
    st.morph = Some(MorphView {
        table: table.clone(),
        variable: Variable::of(records, table),
        slots,
    });
    Some(st)
}

/// The states of a cache location or NPC with a morph table, or of an
/// object in an objects pack (drawn from the models pack beside it, its
/// options from the strings pack). `None` for anything else, or a morph
/// table with no children.
///
/// An NPC's own state draws nothing here; the selection fills it in with
/// the creature it already loaded, rather than load it twice.
pub fn load_states(source: &Source, sel: &Sel, record: Option<&Record>) -> Option<States> {
    match (source, sel, record) {
        (Source::Cache { records, .. }, _, Some(r @ Record::Loc(d))) => {
            let model = |r: &Record| {
                rs2convert::preview::model_for(records, r)
                    .ok()
                    .flatten()
                    .map(|m| Arc::new(Renderable::CacheModel(m)))
            };
            let options = |d: &LocDef| d.actions.iter().flatten().cloned().collect::<Vec<_>>();
            morph_states(
                records,
                "loc",
                d.id,
                options(d),
                model(r),
                d.morph.as_ref()?,
                |c| {
                    let r = records.decode(RecordType::Loc, c.into()).ok()?;
                    let Record::Loc(cd) = &r else { return None };
                    Some((options(cd), model(&r)))
                },
            )
        }
        (Source::Cache { records, .. }, _, Some(Record::Npc(d))) => {
            let options = |d: &NpcDef| d.actions.iter().flatten().cloned().collect::<Vec<_>>();
            morph_states(
                records,
                "npc",
                d.id,
                options(d),
                None,
                d.morph.as_ref()?,
                |c| {
                    let r = records.decode(RecordType::Npc, c.into()).ok()?;
                    let Record::Npc(cd) = &r else { return None };
                    let shown =
                        load_renderable(source, &Sel::Record(RecordType::Npc, c.into()), Some(&r))
                            .ok()
                            .flatten()
                            .map(Arc::new);
                    Some((options(cd), shown))
                },
            )
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
        ids::SPOTANIM_MODEL => Some(ids::SPOTANIM_RIG | (model & 0x0FFF_FFFF)),
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
/// id, an NPC becomes `CREATURE_MODEL | id`, a spot animation
/// `SPOTANIM_MODEL | id`. `None` for other types.
pub fn cache_to_pack((t, id): (RecordType, u32)) -> Option<u32> {
    match t {
        RecordType::Model if id < ids::CREATURE_MODEL => Some(id),
        RecordType::Npc => Some(ids::CREATURE_MODEL | id),
        RecordType::SpotAnim => Some(ids::SPOTANIM_MODEL | id),
        _ => None,
    }
}

/// The cache record a pack model record was converted from; the inverse of
/// [`cache_to_pack`]. `None` for kit parts, the player body and the like.
pub fn pack_to_cache(id: u32) -> Option<(RecordType, u32)> {
    match id & 0xF000_0000 {
        0 => Some((RecordType::Model, id)),
        ids::CREATURE_MODEL => Some((RecordType::Npc, id & 0x0FFF_FFFF)),
        ids::SPOTANIM_MODEL => Some((RecordType::SpotAnim, id & 0x0FFF_FFFF)),
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
