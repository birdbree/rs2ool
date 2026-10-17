//! Objects (`loc.dat`): a model record per `(definition, model)`, an
//! `objects` record per definition, a `kits` entry per wall-class
//! definition, and the names and options in a string table.

use std::collections::{BTreeMap, BTreeSet};

use rayon::prelude::*;
use rs2cache::Records;
use rs2cache::types::config::LocDef;
use rs2cache::types::model::Model;
use rs2cache::types::record::{Record, RecordType};
use rs2pack::records::object::{
    self, KitRecord, ObjectRecord, State, Transition, collision, kit_kind, layer, placement,
};
use rs2pack::records::strings;
use rs2pack::{Kind, PackWriter};

use crate::creature::merge;
use crate::palette::Palette;
use crate::{ConvertError, model};

/// Object model records are `OBJECT_MODEL | loc << 4 | index`.
pub const OBJECT_MODEL: u32 = 0x4000_0000;
/// Wall kits use synthetic material `KIT_MATERIAL + loc`.
pub const KIT_MATERIAL: u16 = 2000;
/// String-table domain for objects (records `domain << 8 | locale`).
pub const STRINGS_DOMAIN: u32 = 2;

/// The client's layer for a 317 location type.
pub fn layer_of(kind: u8) -> u8 {
    match kind {
        0..=3 | 9 => layer::WALL,
        4..=8 => layer::WALL_DECORATION,
        10 | 11 => layer::FURNITURE,
        12..=21 => layer::ROOF,
        _ => layer::FLOOR_DECORATION,
    }
}

/// Geometry as the client builds it for a definition, before placement:
/// mirrored (z negated, winding kept), scaled, then offset.
pub fn shaped(mut m: Model, d: &LocDef) -> Model {
    if d.mirrored {
        for v in &mut m.vertices {
            v[2] = -v[2];
        }
        for f in &mut m.faces {
            f.swap(0, 2);
        }
    }
    let scale = [d.scale_x, d.scale_y, d.scale_z].map(i32::from);
    crate::deform::apply_scale(&mut m.vertices, scale);
    let off = [d.offset_x, d.offset_y, d.offset_z].map(i32::from);
    for v in &mut m.vertices {
        for a in 0..3 {
            v[a] += off[a];
        }
    }
    m
}

/// The models of a definition, by placement type: untyped models (op 5)
/// merge into one type-10 model, as the client draws them.
pub fn models_of(records: &Records, d: &LocDef) -> Result<Vec<(u8, Model)>, ConvertError> {
    let load = |id: u16| -> Result<Model, ConvertError> {
        match records.decode(RecordType::Model, id.into())? {
            Record::Model(m) => Ok(m),
            _ => unreachable!(),
        }
    };
    if d.models.iter().all(|m| m.kind.is_none()) {
        if d.models.is_empty() {
            return Ok(Vec::new());
        }
        let parts = d
            .models
            .iter()
            .map(|m| load(m.model))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(vec![(10, shaped(merge(&parts), d))]);
    }
    d.models
        .iter()
        .map(|m| Ok((m.kind.unwrap_or(10), shaped(load(m.model)?, d))))
        .collect()
}

/// Deduplicating string table builder.
#[derive(Default)]
struct Strings {
    list: Vec<String>,
    index: BTreeMap<String, u32>,
}

impl Strings {
    fn add(&mut self, s: Option<&str>) -> u32 {
        let Some(s) = s else { return object::NONE };
        if let Some(&i) = self.index.get(s) {
            return i;
        }
        let i = self.list.len() as u32;
        self.list.push(s.to_owned());
        self.index.insert(s.to_owned(), i);
        i
    }
}

/// The packs produced for a set of objects.
pub struct ObjectPacks {
    pub models: PackWriter,
    pub objects: PackWriter,
    pub kits: PackWriter,
    pub strings: PackWriter,
}

/// Convert `ids` (and the morph children their states need).
pub fn convert(
    records: &Records,
    ids: &[u32],
    palette: &Palette,
    source_cache_crc: u32,
) -> Result<ObjectPacks, ConvertError> {
    let defs: BTreeMap<u32, LocDef> = {
        let mut want: BTreeSet<u32> = ids.iter().copied().collect();
        let mut out = BTreeMap::new();
        while let Some(id) = want.pop_first() {
            if out.contains_key(&id) {
                continue;
            }
            let Record::Loc(d) = records.decode(RecordType::Loc, id)? else {
                unreachable!()
            };
            if let Some(m) = &d.morph {
                want.extend(
                    m.children
                        .iter()
                        .flatten()
                        .map(|&c| u32::from(c))
                        .filter(|c| !out.contains_key(c)),
                );
            }
            out.insert(id, d);
        }
        out
    };
    let shapes: Vec<(u32, Vec<(u8, Model)>)> = defs
        .par_iter()
        .map(|(&id, d)| Ok((id, models_of(records, d)?)))
        .collect::<Result<_, ConvertError>>()?;
    let shapes: BTreeMap<u32, Vec<(u8, Model)>> = shapes.into_iter().collect();

    let mut packs = ObjectPacks {
        models: PackWriter::new(Kind::Models, source_cache_crc),
        objects: PackWriter::new(Kind::Objects, source_cache_crc),
        kits: PackWriter::new(Kind::Kits, source_cache_crc),
        strings: PackWriter::new(Kind::Strings, source_cache_crc),
    };
    let model_ref = |loc: u32, i: usize| OBJECT_MODEL | loc << 4 | i as u32;
    for (&id, list) in &shapes {
        if list.len() > 16 {
            return Err(ConvertError::Invalid(format!(
                "loc {id} has {} models",
                list.len()
            )));
        }
        let converted: Vec<_> = list
            .par_iter()
            .map(|(_, m)| model::convert(m, palette))
            .collect::<Result<_, _>>()?;
        for (i, rec) in converted.iter().enumerate() {
            packs.models.insert(model_ref(id, i), rec.encode()?);
        }
    }
    let mut strings = Strings::default();
    for (&id, d) in &defs {
        if !ids.contains(&id) {
            continue;
        }
        let state_of = |loc: u32, d: &LocDef, strings: &mut Strings| {
            let blocked = u16::from(d.blocked_directions & 0x0f) << collision::BLOCKED_SHIFT;
            State {
                model: if shapes.get(&loc).is_some_and(|l| !l.is_empty()) {
                    model_ref(loc, 0)
                } else {
                    object::NONE
                },
                collision: u16::from(d.solid) * collision::SOLID
                    + u16::from(d.impenetrable) * collision::IMPENETRABLE
                    + blocked,
                light: object::NONE,
                sound: object::NONE,
                options: d.actions.each_ref().map(|a| strings.add(a.as_deref())),
            }
        };
        let mut states = vec![state_of(id, d, &mut strings)];
        let mut transitions = Vec::new();
        if let Some(a) = d.animation {
            transitions.push(Transition {
                from: 0,
                to: 0,
                anim: a.into(),
                ticks: 0,
            });
        }
        for &child in d.morph.iter().flat_map(|m| m.children.iter()).flatten() {
            let c = &defs[&u32::from(child)];
            let s = states.len() as u16;
            states.push(state_of(child.into(), c, &mut strings));
            if let Some(a) = c.animation {
                transitions.push(Transition {
                    from: s,
                    to: s,
                    anim: a.into(),
                    ticks: 0,
                });
            }
        }
        let shapes_here = &shapes[&id];
        let first_kind = shapes_here.first().map_or(10, |(k, _)| *k);
        let mut place = 0u16;
        for (on, bit) in [
            (d.adjust_to_terrain, placement::ADJUST_TO_TERRAIN),
            (d.obstructs_ground, placement::OBSTRUCTS_GROUND),
            (d.occludes, placement::OCCLUDES),
            (d.casts_shadow, placement::CASTS_SHADOW),
            (d.support_items != 0, placement::SUPPORTS_ITEMS),
            (d.hollow, placement::HOLLOW),
            (d.flat_shading, placement::FLAT_SHADED),
            (d.has_actions, placement::INTERACTIVE),
        ] {
            if on {
                place |= bit;
            }
        }
        let rec = ObjectRecord {
            id: id as u16,
            name: strings.add(d.name.as_deref()),
            width: d.size_x,
            height: d.size_y,
            layer: layer_of(first_kind),
            placement: place,
            surfaces: d
                .recolours
                .iter()
                .map(|r| (palette.index_of(r.from), palette.index_of(r.to)))
                .collect(),
            models: shapes_here
                .iter()
                .enumerate()
                .map(|(i, (k, _))| (*k, model_ref(id, i)))
                .collect(),
            states,
            transitions,
        };
        packs.objects.insert(id, rec.encode()?);
        let walls: Vec<(u16, u32)> = shapes_here
            .iter()
            .enumerate()
            .filter(|(_, (k, _))| layer_of(*k) == layer::WALL)
            .flat_map(|(i, (k, _))| {
                (0..4u16).map(move |rot| (u16::from(*k) << 2 | rot, model_ref(id, i)))
            })
            .collect();
        if !walls.is_empty() {
            let kit = KitRecord {
                material: KIT_MATERIAL + id as u16,
                kind: kit_kind::WALL,
                variants: walls,
            };
            packs.kits.insert(id, kit.encode());
        }
    }
    packs
        .strings
        .insert(STRINGS_DOMAIN << 8, strings::encode(&strings.list));
    Ok(packs)
}
