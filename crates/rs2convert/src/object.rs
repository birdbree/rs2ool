//! Objects (`loc.dat`): a model record per `(definition, model)`, an
//! `objects` record per definition, a `kits` entry per definition with wall
//! models, and the names and options in one string table.
//!
//! ```text
//! model record   OBJECT_MODEL | loc << 4 | index    (index < 16)
//! kit material   material_id::PALETTE (faces carry their own colours)
//! kit variant    key = type << 2 | rotation (0–3), one per wall model
//! strings        STRINGS_DOMAIN << 8 | locale       (locale 0)
//! ```

use std::collections::{BTreeMap, BTreeSet};

use contentpack::records::object::{
    self, KitRecord, ObjectRecord, State, Transition, collision, kit_kind, layer, placement,
};
use contentpack::records::strings;
use contentpack::{Kind, PackWriter};
use rayon::prelude::*;
use rs2cache::Records;
use rs2cache::types::config::LocDef;
use rs2cache::types::model::Model;
use rs2cache::types::record::{Record, RecordType};

use crate::creature::merge;
use crate::palette::Palette;
use crate::{ConvertError, material_id, model};

/// Base of the object model record ids: `OBJECT_MODEL | loc << 4 | index`.
pub const OBJECT_MODEL: u32 = 0x4000_0000;
/// String-table domain for objects (records `domain << 8 | locale`).
pub const STRINGS_DOMAIN: u32 = 2;

/// The object layer for a 317 location type: walls 0–3 and 9, wall
/// decorations 4–8, free-standing 10–11, roofs 12–21, and floor
/// decoration for everything else.
pub fn layer_of(kind: u8) -> u8 {
    match kind {
        0..=3 | 9 => layer::WALL,
        4..=8 => layer::WALL_DECORATION,
        10 | 11 => layer::FURNITURE,
        12..=21 => layer::ROOF,
        _ => layer::FLOOR_DECORATION,
    }
}

/// A definition's geometry as the original client builds it, before
/// placement: mirrored, scaled, then offset.
///
/// Mirroring negates z and swaps each face's first and last corners, so
/// faces still point outwards.
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

/// The models of a definition, by placement type, each already
/// [`shaped`].
///
/// If no model has a type (opcode 5), they're all merged into one type-10
/// model, as the original client draws them. Otherwise each typed model
/// stands alone, and any untyped one among them counts as type 10.
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
pub(crate) struct Strings {
    pub(crate) list: Vec<String>,
    index: BTreeMap<String, u32>,
}

impl Strings {
    /// The string's index, adding it if it's new; `NONE` for no string.
    pub(crate) fn add(&mut self, s: Option<&str>) -> u32 {
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
    /// Every model of every definition touched, morph children included.
    pub models: PackWriter,
    /// One record per requested id.
    pub objects: PackWriter,
    /// One wall kit per requested id that has wall models.
    pub kits: PackWriter,
    /// The string table (names and options), as one record.
    pub strings: PackWriter,
}

/// Convert `ids`, and the models of the morph children their states need.
///
/// Each morph child becomes an extra state on its parent's record rather
/// than a record of its own (unless it's in `ids` too). Fails if a
/// definition has more than 16 models.
pub fn convert(
    records: &Records,
    ids: &[u32],
    palette: &Palette,
    source_cache_crc: u32,
) -> Result<ObjectPacks, ConvertError> {
    // Requested definitions plus every morph child reachable from them.
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
        // One state per definition: its first model, collision bits and
        // option strings.
        let state_of = |loc: u32, d: &LocDef, strings: &mut Strings| {
            // The low four bits are the blocked-direction mask.
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
        // An animated state loops its animation on itself.
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
        // The layer comes from the first model's type (10 when it has none).
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
        // Wall kit: every wall model at each of the four rotations, keyed
        // `type << 2 | rotation`.
        let walls: Vec<(u16, u32)> = shapes_here
            .iter()
            .enumerate()
            .filter(|(_, (k, _))| layer_of(*k) == layer::WALL)
            .flat_map(|(i, (k, _))| {
                (0..4u16).map(move |rot| (u16::from(*k) << 2 | rot, model_ref(id, i)))
            })
            .collect();
        if !walls.is_empty() {
            // The wall models' faces already carry their palette indices
            // and texture slots, so the kit's material is the shared
            // palette material that every untextured face uses.
            let kit = KitRecord {
                material: material_id::PALETTE,
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
