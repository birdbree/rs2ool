//! Dependency groups for `dump --group`: a record plus everything it pulls
//! in.
//!
//! - `npc:<id>`: the NPC, its body and chat-head models, its stand and
//!   walk sequences, and the frame packs those sequences use.
//! - `item:<id>`: the item, its inventory model and every equip and
//!   chat-head model.
//! - `player-kit`: every identity-kit part and its models, plus
//!   [`PLAYER_SEQUENCES`] and their frame packs.

use std::collections::BTreeSet;

use rs2cache::Records;
use rs2cache::types::config::SeqDef;
use rs2cache::types::record::{Record, RecordType};

use crate::cache::select::usage;

/// The player's stance sequences: stand, turn, walk, turn 180°, turn 90°
/// each way, run.
///
/// They aren't in the cache; the server sends them in each player's
/// appearance block. These are the standard 317 values.
pub const PLAYER_SEQUENCES: [u32; 7] = [808, 823, 819, 820, 821, 822, 824];

/// Resolves a group name to the records it covers, each once, sorted by
/// type and then id.
///
/// An unknown group or a missing id is a usage error. A sequence that
/// won't decode is quietly left out, frames and all.
pub fn resolve(records: &Records, name: &str) -> anyhow::Result<Vec<(RecordType, u32)>> {
    let mut out = BTreeSet::new();
    let (kind, arg) = name.split_once(':').unwrap_or((name, ""));
    // Parses the `<id>` after the colon, for the groups that need one.
    let id = || -> anyhow::Result<u32> {
        arg.parse()
            .map_err(|_| usage(format!("group {kind} needs an id, e.g. {kind}:1")))
    };
    // Sequences to chase down to their frame packs once the match is done.
    let mut seqs = Vec::new();
    match kind {
        "npc" => {
            let id = id()?;
            let Record::Npc(d) = records.decode(RecordType::Npc, id)? else {
                unreachable!()
            };
            out.insert((RecordType::Npc, id));
            out.extend(
                d.models
                    .iter()
                    .chain(&d.chat_head_models)
                    .map(|&m| (RecordType::Model, u32::from(m))),
            );
            seqs.extend(
                [
                    d.stand_anim,
                    d.walk_anim,
                    d.walk_back_anim,
                    d.walk_left_anim,
                    d.walk_right_anim,
                ]
                .into_iter()
                .flatten()
                .map(u32::from),
            );
        }
        "item" => {
            let id = id()?;
            let Record::Obj(d) = records.decode(RecordType::Obj, id)? else {
                unreachable!()
            };
            out.insert((RecordType::Obj, id));
            let models = [
                Some(d.inventory_model),
                d.male_equip1,
                d.male_equip2,
                d.male_equip3,
                d.female_equip1,
                d.female_equip2,
                d.female_equip3,
                d.male_chat_head1,
                d.male_chat_head2,
                d.female_chat_head1,
                d.female_chat_head2,
            ];
            out.extend(
                models
                    .into_iter()
                    .flatten()
                    .map(|m| (RecordType::Model, u32::from(m))),
            );
        }
        "player-kit" => {
            for id in records.ids(RecordType::Idk)? {
                let Record::Idk(d) = records.decode(RecordType::Idk, id)? else {
                    unreachable!()
                };
                out.insert((RecordType::Idk, id));
                let heads = d.chat_head_models.iter().flatten();
                out.extend(
                    d.models
                        .iter()
                        .chain(heads)
                        .map(|&m| (RecordType::Model, u32::from(m))),
                );
            }
            seqs.extend(PLAYER_SEQUENCES);
        }
        _ => {
            return Err(usage(format!(
                "unknown group {name:?}; expected npc:<id>, item:<id> or player-kit"
            )));
        }
    }
    if !seqs.is_empty() {
        let index = records.cache().frame_index()?;
        for s in seqs {
            let Ok(Record::Seq(seq)) = records.decode(RecordType::Seq, s) else {
                continue;
            };
            out.insert((RecordType::Seq, s));
            out.extend(
                packs(&seq, &index)
                    .into_iter()
                    .map(|p| (RecordType::Anim, p)),
            );
        }
    }
    Ok(out.into_iter().collect())
}

/// The frame packs (index 2 file ids) a sequence's frames live in,
/// secondary frames included. Frames no pack defines are skipped.
pub fn packs(seq: &SeqDef, index: &rs2cache::anim::FrameIndex) -> BTreeSet<u32> {
    seq.frames
        .iter()
        // Each step's primary frame id, then its secondary one if it has one.
        .flat_map(|f| std::iter::once(f.frame).chain(f.secondary))
        .filter_map(|f| index.pack_for(f))
        .collect()
}
