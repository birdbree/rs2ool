//! Whole-creature conversion: model, rig, clips and validation, shared by
//! `rs2ool pack` and the explorer so both produce byte-identical packs.
//!
//! [`convert`] does one [`Creature`]; [`player_kit`] assembles the
//! identity-kit parts and the default body. Record ids for everything that
//! doesn't come straight from a cache id are in [`ids`].

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
    /// The default female body's merged model.
    pub const PLAYER_FEMALE_MODEL: u32 = 0x3000_0001;
    /// The player rig's id; NPC rigs use the NPC id.
    pub const PLAYER_RIG: u32 = 0xFFFF;
    /// The female body's rig.
    pub const PLAYER_FEMALE_RIG: u32 = 0xFFFE;
    /// Spot animation `s`'s model is model record `SPOTANIM_MODEL | s`.
    pub const SPOTANIM_MODEL: u32 = 0x7000_0000;
    /// Spot animation `s`'s rig is rig `SPOTANIM_RIG | s`, above every NPC
    /// id and below the player rigs.
    pub const SPOTANIM_RIG: u32 = 0x8000;
    /// Clip id for sequence `seq` on rig `rig`: the rig in the high 16 bits,
    /// the sequence in the low 16.
    pub fn clip(rig: u32, seq: u32) -> u32 {
        rig << 16 | seq
    }
}

/// A converted creature.
pub struct CreatureOutput {
    /// The creature it came from.
    pub creature: Creature,
    /// The merged model with scale baked in, as a pack model.
    pub model: PackModel,
    /// The rig as inferred, with names, label sets and cache-space pivots.
    pub inferred: InferredRig,
    /// The rig record, after an encode and decode round trip.
    pub rig: Rig,
    /// `(clip id, clip, report)` per sequence, clips round-tripped like the
    /// rig. Empty without animations.
    pub clips: Vec<(u32, Clip, ClipReport)>,
}

/// What `validation.json` holds for a creature.
#[derive(Debug, Serialize)]
pub struct ValidationFile<'a> {
    /// The creature's name.
    pub creature: &'a str,
    /// Every bone, root first.
    pub bones: Vec<BoneSummary<'a>>,
    /// Largest clip `max`, 0 with no clips.
    pub max: f64,
    /// Whether any clip is flagged.
    pub flagged: bool,
    /// Each clip's report.
    pub clips: Vec<&'a ClipReport>,
}

/// One bone in `validation.json`.
#[derive(Debug, Serialize)]
pub struct BoneSummary<'a> {
    /// The bone's name.
    pub name: &'a str,
    /// Parent bone index, `None` for the root.
    pub parent: Option<usize>,
    /// The bone's label set, sorted.
    pub labels: Vec<u8>,
}

impl CreatureOutput {
    /// The worst vertex error over every clip (0 with no clips).
    pub fn max_error(&self) -> f64 {
        self.clips.iter().map(|c| c.2.max).fold(0.0, f64::max)
    }

    /// Whether any clip went over the validation threshold.
    pub fn flagged(&self) -> bool {
        self.clips.iter().any(|c| c.2.flagged)
    }

    /// The contents of this creature's `validation.json`.
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

    /// Add this creature's records to the three writers: the model as
    /// `model_id`, the rig as `rig_id`, and each clip under its own id.
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

/// Convert an assembled creature: model, rig and, with `with_anims`, a
/// validated clip per sequence.
///
/// The rig and clips are encoded and decoded before validation, so the
/// report describes exactly what ends up in the pack.
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

/// Empty `(models, rigs, anims)` writers for a creature run.
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
    /// The identity-kit id.
    pub idk: u32,
    /// Body slot, 0–6 (the cache's body part modulo 7).
    pub slot: u8,
    /// 0 for the male set, 1 for the female set (body part / 7).
    pub body_type: u8,
    /// The body slots this part hides when worn: always 0, because a kit
    /// part never hides another. In the original client only worn items
    /// do, and only because the server leaves those slots empty.
    pub hide_mask: u16,
    /// Whether the definition marks the part as selectable.
    pub selectable: bool,
}

/// The identity kit, converted: every part, and the two default bodies.
pub struct PlayerKit {
    /// Each identity-kit part with a body part, as a merged model.
    pub parts: Vec<(KitPart, rs2cache::types::model::Model)>,
    /// The default male body: the first selectable part of each body part
    /// 0–6, merged, with the player stance sequences.
    pub male: Creature,
    /// The default female body, the same from body parts 7–13; `None` if
    /// the kit has no selectable female parts.
    pub female: Option<Creature>,
}

/// The identity-kit parts, each as a merged model, plus the default male
/// and female bodies (the first selectable part of each body part, 0–6
/// and 7–13) assembled as creatures with the player stance sequences.
/// Parts with no body part are skipped.
pub fn player_kit(records: &Records, index: &FrameIndex) -> Result<PlayerKit, ConvertError> {
    let mut parts = Vec::new();
    // Default bodies: the first selectable part per body part, 0–13.
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
        if part < 14 && d.selectable {
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
    let (sequences, packs) = creature::sequences(records, index, &PLAYER_SEQUENCES)?;
    // Each body's parts in body-part order, merged.
    let body = |name: &str, range: std::ops::Range<u8>| {
        let models: Vec<_> = default.range(range).map(|(_, m)| m.clone()).collect();
        (!models.is_empty()).then(|| {
            Creature::new(
                name.into(),
                creature::merge(&models),
                [128; 3],
                sequences.clone(),
                packs.clone(),
            )
        })
    };
    let male = body("player", 0..7)
        .ok_or_else(|| ConvertError::Invalid("the kit has no selectable male parts".into()))?;
    let female = body("player_female", 7..14);
    Ok(PlayerKit {
        parts,
        male,
        female,
    })
}
