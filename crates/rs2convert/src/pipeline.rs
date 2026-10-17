//! Whole-creature conversion: model, rig, clips and validation, shared by
//! `rs2conv` and the explorer so both produce byte-identical packs.

use std::collections::BTreeMap;

use contentpack::records::anim::Clip;
use contentpack::records::model::PackModel;
use contentpack::records::rig::Rig;
use contentpack::{Kind, PackWriter};
use rs2cache::Records;
use rs2cache::anim::FrameIndex;
use rs2cache::types::record::{Record, RecordType};
use serde::Serialize;

use crate::creature::{self, Creature};
use crate::palette::Palette;
use crate::rig::{InferredRig, ParentOverride, infer};
use crate::validate::{ClipReport, validate};
use crate::{ConvertError, model};

/// Record-id namespaces. Creature and kit-part models share the models
/// pack kind with cache models, so they live above the cache's id range.
pub mod ids {
    /// NPC `n`'s merged model is model record `CREATURE_MODEL | n`.
    pub const CREATURE_MODEL: u32 = 0x1000_0000;
    /// Identity-kit part `k` is model record `KIT_MODEL | k`.
    pub const KIT_MODEL: u32 = 0x2000_0000;
    /// The default player body's merged model.
    pub const PLAYER_MODEL: u32 = 0x3000_0000;
    /// The player rig's id; NPC rigs use the NPC id.
    pub const PLAYER_RIG: u32 = 0xFFFF;
    /// Clip id for sequence `s` on rig `r`.
    pub fn clip(rig: u32, seq: u32) -> u32 {
        rig << 16 | seq
    }
}

/// A converted creature.
pub struct CreatureOutput {
    pub creature: Creature,
    pub model: PackModel,
    pub inferred: InferredRig,
    pub rig: Rig,
    pub clips: Vec<(u32, Clip, ClipReport)>,
}

/// What `validation.json` holds for a creature.
#[derive(Debug, Serialize)]
pub struct ValidationFile<'a> {
    pub creature: &'a str,
    pub bones: Vec<BoneSummary<'a>>,
    pub max: f64,
    pub flagged: bool,
    pub clips: Vec<&'a ClipReport>,
}

#[derive(Debug, Serialize)]
pub struct BoneSummary<'a> {
    pub name: &'a str,
    pub parent: Option<usize>,
    pub labels: Vec<u8>,
}

impl CreatureOutput {
    pub fn max_error(&self) -> f64 {
        self.clips.iter().map(|c| c.2.max).fold(0.0, f64::max)
    }

    pub fn flagged(&self) -> bool {
        self.clips.iter().any(|c| c.2.flagged)
    }

    pub fn validation(&self) -> ValidationFile<'_> {
        ValidationFile {
            creature: &self.creature.name,
            bones: self
                .inferred
                .bones
                .iter()
                .map(|b| BoneSummary {
                    name: &b.name,
                    parent: b.parent,
                    labels: b.labels.iter().copied().collect(),
                })
                .collect(),
            max: self.max_error(),
            flagged: self.flagged(),
            clips: self.clips.iter().map(|c| &c.2).collect(),
        }
    }

    /// Add this creature's records to the three writers.
    pub fn write(
        &self,
        model_id: u32,
        rig_id: u32,
        models: &mut PackWriter,
        rigs: &mut PackWriter,
        anims: &mut PackWriter,
    ) -> Result<(), ConvertError> {
        models.insert(model_id, self.model.encode()?);
        rigs.insert(rig_id, self.rig.encode()?);
        for (id, clip, _) in &self.clips {
            anims.insert(*id, clip.encode()?);
        }
        Ok(())
    }
}

/// Convert an assembled creature. Clips are validated from their encoded
/// records, so the report reflects exactly what the client will read.
pub fn convert(
    creature: Creature,
    palette: &Palette,
    rig_id: u32,
    with_anims: bool,
    overrides: Option<&ParentOverride>,
) -> Result<CreatureOutput, ConvertError> {
    let model = model::convert(&creature.rest_model(), palette)?;
    let inferred = infer(&creature, overrides)?;
    let rig = Rig::decode(&inferred.record()?.encode()?)?;
    let mut clips = Vec::new();
    if with_anims {
        for s in &creature.sequences {
            let clip = crate::clip::convert(&creature, &inferred, s, rig_id)?;
            let clip = Clip::decode(&clip.encode()?)?;
            let report = validate(&creature, &rig, &clip, s);
            clips.push((ids::clip(rig_id, s.def.id), clip, report));
        }
    }
    Ok(CreatureOutput {
        creature,
        model,
        inferred,
        rig,
        clips,
    })
}

/// Empty writers for a creature run.
pub fn writers(source_cache_crc: u32) -> (PackWriter, PackWriter, PackWriter) {
    (
        PackWriter::new(Kind::Models, source_cache_crc),
        PackWriter::new(Kind::Rigs, source_cache_crc),
        PackWriter::new(Kind::Anims, source_cache_crc),
    )
}

/// The player's stance sequences (not in the cache; the standard 317
/// values the server sends): stand, turn, walk, turn 180°, turn 90° each
/// way, run.
pub const PLAYER_SEQUENCES: [u16; 7] = [808, 823, 819, 820, 821, 822, 824];

/// One identity-kit part's metadata, stored in the models pack manifest
/// under `kit_parts`.
#[derive(Debug, Clone, Serialize)]
pub struct KitPart {
    pub idk: u32,
    pub slot: u8,
    pub body_type: u8,
    pub hide_mask: u16,
    pub selectable: bool,
}

/// The identity-kit parts, each as a merged model record, plus the default
/// male body (the first selectable part of each slot 0–6) assembled as a
/// creature with the player stance sequences.
pub fn player_kit(
    records: &Records,
    index: &FrameIndex,
) -> Result<(Vec<(KitPart, rs2cache::types::model::Model)>, Creature), ConvertError> {
    let mut parts = Vec::new();
    let mut default: BTreeMap<u8, rs2cache::types::model::Model> = BTreeMap::new();
    for id in records.ids(RecordType::Idk)? {
        let Record::Idk(d) = records.decode(RecordType::Idk, id)? else {
            unreachable!()
        };
        let Some(part) = d.body_part else { continue };
        let models = d
            .models
            .iter()
            .map(|&m| match records.decode(RecordType::Model, m.into())? {
                Record::Model(m) => Ok(m),
                _ => unreachable!(),
            })
            .collect::<Result<Vec<_>, ConvertError>>()?;
        let mut merged = creature::merge(&models);
        creature::recolour(&mut merged, &d.recolours);
        if part < 7 && d.selectable {
            default.entry(part).or_insert_with(|| merged.clone());
        }
        parts.push((
            KitPart {
                idk: id,
                slot: part % 7,
                body_type: part / 7,
                hide_mask: 0,
                selectable: d.selectable,
            },
            merged,
        ));
    }
    let body: Vec<_> = default.into_values().collect();
    let merged = creature::merge(&body);
    let (sequences, packs) = creature::sequences(records, index, &PLAYER_SEQUENCES)?;
    Ok((
        parts,
        Creature::new("player".into(), merged, [128; 3], sequences, packs),
    ))
}
