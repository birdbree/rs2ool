//! What the viewports draw, from any source kind: a cache record's model
//! as the client assembles it, a model a dump directory holds as JSON, or
//! a converted pack model with its pack palette.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rs2convert::pipeline::ids;
use rs2intermediate::model::Model;
use rs2intermediate::record::{Record, RecordType};
use rs2pack::records::material::{PALETTE_ID, decode_palette};
use rs2pack::records::model::PackModel;
use rs2pack::{Kind, PackReader};

use crate::selection::Sel;
use crate::sources::Source;

pub enum Renderable {
    CacheModel(Model),
    DumpedModel(Model),
    PackModel {
        model: PackModel,
        /// The palette of the materials pack beside the models pack, when
        /// one is there.
        palette: Option<Arc<Vec<[u8; 3]>>>,
    },
}

impl Renderable {
    /// The decoded cache-format model, for the drawers that need one.
    pub fn model(&self) -> Option<&Model> {
        match self {
            Renderable::CacheModel(m) | Renderable::DumpedModel(m) => Some(m),
            Renderable::PackModel { .. } => None,
        }
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
            if reader.manifest.kind != Kind::Models {
                return Ok(None);
            }
            let bytes = reader
                .get(*id)
                .ok_or("no such record")?
                .map_err(|e| e.to_string())?;
            let model = PackModel::decode(&bytes).map_err(|e| e.to_string())?;
            Ok(Some(Renderable::PackModel {
                model,
                palette: sibling_palette(path).map(Arc::new),
            }))
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
