//! Reverse references for `--xref`, computed once and cached in
//! `<out>/xref.json`.

use std::collections::{BTreeMap, BTreeSet};

use rayon::prelude::*;
use rs2cache::Records;
use rs2intermediate::record::{Record, RecordType};
use serde::{Deserialize, Serialize};

use crate::groups;
use crate::output::{Output, Provenance};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Xref {
    /// Model → texture ids its textured faces use.
    pub model_textures: BTreeMap<u32, Vec<u16>>,
    /// Texture → models that use it.
    pub texture_models: BTreeMap<u32, Vec<u32>>,
    /// `loc:<id>`, `npc:<id>`, `obj:<id>` → model ids.
    pub object_models: BTreeMap<String, Vec<u16>>,
    /// Sequence → frame packs.
    pub sequence_packs: BTreeMap<u32, Vec<u32>>,
    /// Region → location ids placed in it.
    pub region_objects: BTreeMap<u32, Vec<u16>>,
}

impl Xref {
    pub fn load_or_build(records: &Records, out: &Output) -> anyhow::Result<Self> {
        let path = out.root().join("xref.json");
        if let Ok(bytes) = std::fs::read(&path)
            && let Ok(x) = serde_json::from_slice(&bytes)
        {
            return Ok(x);
        }
        let x = Self::build(records)?;
        out.json("xref.json", &x, &Provenance::default())?;
        Ok(x)
    }

    pub fn build(records: &Records) -> anyhow::Result<Self> {
        let mut x = Xref::default();
        let models = records.ids(RecordType::Model)?;
        let textures: Vec<(u32, Vec<u16>)> = models
            .par_iter()
            .filter_map(|&id| {
                let Ok(Record::Model(m)) = records.decode(RecordType::Model, id) else {
                    return None;
                };
                let types = m.face_types.as_ref()?;
                // In 317 a textured face's colour is its texture id.
                let used: BTreeSet<u16> = types
                    .iter()
                    .zip(&m.face_colours)
                    .filter(|(t, _)| *t & 2 != 0)
                    .map(|(_, &c)| c)
                    .collect();
                (!used.is_empty()).then(|| (id, used.into_iter().collect()))
            })
            .collect();
        for (model, texs) in textures {
            for &t in &texs {
                x.texture_models.entry(t.into()).or_default().push(model);
            }
            x.model_textures.insert(model, texs);
        }
        for t in [RecordType::Loc, RecordType::Npc, RecordType::Obj] {
            for id in records.ids(t)? {
                let models: Vec<u16> = match records.decode(t, id)? {
                    Record::Loc(d) => d.models.iter().map(|m| m.model).collect(),
                    Record::Npc(d) => d.models,
                    Record::Obj(d) => [
                        Some(d.inventory_model),
                        d.male_equip1,
                        d.male_equip2,
                        d.male_equip3,
                        d.female_equip1,
                        d.female_equip2,
                        d.female_equip3,
                    ]
                    .into_iter()
                    .flatten()
                    .collect(),
                    _ => continue,
                };
                if !models.is_empty() {
                    x.object_models.insert(format!("{t}:{id}"), models);
                }
            }
        }
        let index = records.cache().frame_index()?;
        for id in records.ids(RecordType::Seq)? {
            if let Record::Seq(s) = records.decode(RecordType::Seq, id)? {
                x.sequence_packs
                    .insert(id, groups::packs(&s, &index).into_iter().collect());
            }
        }
        let regions: Vec<(u32, Vec<u16>)> = records
            .ids(RecordType::Map)?
            .par_iter()
            .filter_map(|&id| match records.decode(RecordType::Map, id) {
                Ok(Record::Map(r)) => {
                    let ids: BTreeSet<u16> = r.locations.iter().map(|l| l.id).collect();
                    Some((id, ids.into_iter().collect()))
                }
                _ => None,
            })
            .collect();
        x.region_objects.extend(regions);
        Ok(x)
    }
}
