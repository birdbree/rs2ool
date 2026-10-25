//! The model a record describes, put together the way the original client
//! draws it, for viewers: NPCs merged, recoloured and scaled; objects shaped for type 10
//! (or their first type); items as their inventory model; identity-kit
//! parts merged; spot animations recoloured and scaled.
//!
//! [`animated`] gives the same models ready to pose with a sequence, as a
//! [`Subject`], and [`player_body`] does the same for a default player
//! body.

use rs2cache::Records;
use rs2cache::anim::FrameIndex;
use rs2cache::types::config::{LocDef, Recolour};
use rs2cache::types::model::Model;
use rs2cache::types::record::{Record, RecordType};

use crate::ConvertError;
use crate::creature::{self, Creature, merge, recolour};
use crate::deform::apply_scale;

fn load(records: &Records, id: u16) -> Result<Model, ConvertError> {
    match records.decode(RecordType::Model, id.into())? {
        Record::Model(m) => Ok(m),
        _ => unreachable!(),
    }
}

/// Merge and recolour `ids`' models, returned with the scale they take.
/// `None` for an empty list.
fn assemble(
    records: &Records,
    ids: &[u16],
    recolours: &[Recolour],
    scale: [i32; 3],
) -> Result<Option<(Model, [i32; 3])>, ConvertError> {
    if ids.is_empty() {
        return Ok(None);
    }
    let parts = ids
        .iter()
        .map(|&m| load(records, m))
        .collect::<Result<Vec<_>, _>>()?;
    let mut m = merge(&parts);
    recolour(&mut m, recolours);
    Ok(Some((m, scale)))
}

/// The model a record describes before its scale, with that scale (128 a
/// unit). Locations come back unshaped, with their definition for
/// [`crate::object::shape_vertices`].
type Parts = (Model, [i32; 3], Option<Box<LocDef>>);

fn parts_of(records: &Records, record: &Record) -> Result<Option<Parts>, ConvertError> {
    let s = |v: u16| i32::from(v);
    let scaled = |r: Option<(Model, [i32; 3])>| r.map(|(m, k)| (m, k, None));
    match record {
        Record::Model(m) => Ok(Some((m.clone(), [128; 3], None))),
        Record::Npc(d) => assemble(
            records,
            &d.models,
            &d.recolours,
            [s(d.scale_xz), s(d.scale_y), s(d.scale_xz)],
        )
        .map(scaled),
        Record::Obj(d) => assemble(
            records,
            &[d.inventory_model],
            &d.recolours,
            [s(d.scale_x), s(d.scale_y), s(d.scale_z)],
        )
        .map(scaled),
        Record::Idk(d) => assemble(records, &d.models, &d.recolours, [128; 3]).map(scaled),
        Record::SpotAnim(d) => match d.model {
            Some(m) => assemble(
                records,
                &[m],
                &d.recolours,
                [s(d.scale_xy), s(d.scale_z), s(d.scale_xy)],
            )
            .map(scaled),
            None => Ok(None),
        },
        Record::Loc(d) => {
            let models = crate::object::unshaped_models_of(records, d)?;
            let pick = models.iter().position(|(k, _)| *k == 10).unwrap_or(0);
            Ok(models.into_iter().nth(pick).map(|(_, mut m)| {
                recolour(&mut m, &d.recolours);
                (m, [128; 3], Some(Box::new(d.clone())))
            }))
        }
        _ => Ok(None),
    }
}

/// The model `record` describes, if it describes one. Record types with no
/// model give `None`.
pub fn model_for(records: &Records, record: &Record) -> Result<Option<Model>, ConvertError> {
    Ok(parts_of(records, record)?.map(|(mut m, scale, loc)| {
        match loc {
            Some(d) => m = crate::object::shaped(m, &d),
            None => apply_scale(&mut m.vertices, scale),
        }
        m
    }))
}

/// A model ready to pose with sequences: a creature, plus for a location
/// the shaping each pose gets afterwards (the original client animates a
/// location's model, then mirrors, scales and moves it).
#[derive(Debug, Clone)]
pub struct Subject {
    /// The model, its scale and its sequences.
    pub creature: Creature,
    shape: Option<Box<LocDef>>,
}

impl Subject {
    /// The rest model as it's drawn, scaled (and for a location shaped):
    /// what to light.
    pub fn model(&self) -> Model {
        match &self.shape {
            Some(d) => crate::object::shaped(self.creature.model.clone(), d),
            None => self.creature.rest_model(),
        }
    }

    /// Vertices posed from the creature's model, finished: shaped for a
    /// location, as they are for anything else.
    pub fn finish(&self, mut vertices: Vec<[i32; 3]>) -> Vec<[i32; 3]> {
        if let Some(d) = &self.shape {
            crate::object::shape_vertices(&mut vertices, d);
        }
        vertices
    }

    /// The vertices at frame `i` of sequence `s` as the original client
    /// draws them. See [`Creature::posed`].
    pub fn posed(&self, s: usize, i: usize) -> Option<Vec<[i32; 3]>> {
        self.creature.posed(s, i).map(|v| self.finish(v))
    }

    /// The vertices at frame `i` of sequence `s` while the creature moves,
    /// at frame `j` of movement sequence `m`, as the original client draws
    /// them. See [`Creature::posed_moving`].
    pub fn posed_moving(&self, s: usize, i: usize, m: usize, j: usize) -> Option<Vec<[i32; 3]>> {
        self.creature
            .posed_moving(s, i, m, j)
            .map(|v| self.finish(v))
    }

    /// The index of sequence `id` among the creature's sequences.
    pub fn sequence(&self, id: u16) -> Option<usize> {
        self.creature
            .sequences
            .iter()
            .position(|s| s.def.id == u32::from(id))
    }
}

/// The model `kind` `id` describes, with `seqs` loaded to pose it. An NPC
/// comes with its stance and walk sequences too, as `rs2ool pack npc`
/// assembles it, and a spot animation with its own. `None` for a record
/// type with no model, or a record with none.
pub fn animated(
    records: &Records,
    index: &FrameIndex,
    kind: RecordType,
    id: u32,
    seqs: &[u16],
) -> Result<Option<Subject>, ConvertError> {
    let creature = match kind {
        RecordType::Npc => creature::npc(records, index, id, seqs)?,
        RecordType::SpotAnim
            if matches!(records.decode(kind, id)?, Record::SpotAnim(d)
                if d.model.is_some() && d.sequence.is_some()) =>
        {
            with_sequences(
                creature::spotanim(records, index, id)?,
                records,
                index,
                seqs,
            )?
        }
        _ => {
            let record = records.decode(kind, id)?;
            let Some((model, scale, shape)) = parts_of(records, &record)? else {
                return Ok(None);
            };
            let (sequences, packs) = creature::sequences(records, index, seqs)?;
            let name = format!("{} {id}", kind.name());
            return Ok(Some(Subject {
                creature: Creature::new(name, model, scale, sequences, packs),
                shape,
            }));
        }
    };
    Ok(Some(Subject {
        creature,
        shape: None,
    }))
}

/// The default player body (male, or female with `female`) with its
/// stance sequences and `seqs`, as `rs2ool pack player-kit` assembles it.
pub fn player_body(
    records: &Records,
    index: &FrameIndex,
    female: bool,
    seqs: &[u16],
) -> Result<Subject, ConvertError> {
    let kit = crate::pipeline::player_kit(records, index)?;
    let body = if female {
        kit.female
            .ok_or_else(|| ConvertError::Invalid("the kit has no female body".into()))?
    } else {
        kit.male
    };
    Ok(Subject {
        creature: with_sequences(body, records, index, seqs)?,
        shape: None,
    })
}

/// `c` with `extra` sequences added after its own.
fn with_sequences(
    c: Creature,
    records: &Records,
    index: &FrameIndex,
    extra: &[u16],
) -> Result<Creature, ConvertError> {
    if extra.is_empty() {
        return Ok(c);
    }
    let ids: Vec<u16> = c
        .sequences
        .iter()
        .map(|s| s.def.id as u16)
        .chain(extra.iter().copied())
        .collect();
    let (sequences, packs) = creature::sequences(records, index, &ids)?;
    let mut out = Creature::new(c.name, c.model, c.scale, sequences, packs);
    out.moving = c.moving;
    Ok(out)
}

/// The sequence a record plays by itself: an NPC's stand sequence, a spot
/// animation's sequence, a location's animation. `None` when it has none
/// or its type has none.
pub fn own_sequence(
    records: &Records,
    kind: RecordType,
    id: u32,
) -> Result<Option<u16>, ConvertError> {
    Ok(match kind {
        RecordType::Npc | RecordType::SpotAnim | RecordType::Loc => {
            match records.decode(kind, id)? {
                Record::Npc(d) => d.stand_anim,
                Record::SpotAnim(d) => d.sequence,
                Record::Loc(d) => d.animation,
                _ => None,
            }
        }
        _ => None,
    })
}

/// The model a location definition draws for placement type `kind`, as the
/// original client picks it: the typed model for that type, or (for definitions with
/// untyped models) the merged models when the type is 10 or 11.
pub fn location_model(
    records: &Records,
    def: &rs2cache::types::config::LocDef,
    kind: u8,
) -> Result<Option<Model>, ConvertError> {
    let untyped = def.models.iter().all(|m| m.kind.is_none());
    // Type 11 is a rotated type 10 in the original client: same model.
    let want = if kind == 11 { 10 } else { kind };
    if untyped && want != 10 {
        return Ok(None);
    }
    let models = crate::object::models_of(records, def)?;
    Ok(models
        .into_iter()
        .find(|(k, _)| *k == want)
        .map(|(_, mut m)| {
            recolour(&mut m, &def.recolours);
            m
        }))
}
