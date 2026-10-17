//! Test subjects picked from the cache by their properties, so the tests
//! don't depend on any particular record.

use rs2cache::Records;
use rs2cache::types::record::{Record, RecordType};

/// The first NPC that is a full walking character: a name, at least two
/// body models, the default scale on both axes, and both a stand and a
/// walk sequence that decode.
pub fn walking_npc(records: &Records) -> u32 {
    let seq = |s: Option<u16>| {
        s.filter(|&s| s != u16::MAX)
            .is_some_and(|s| records.decode(RecordType::Seq, s.into()).is_ok())
    };
    records
        .ids(RecordType::Npc)
        .unwrap()
        .into_iter()
        .find(|&id| {
            matches!(records.decode(RecordType::Npc, id), Ok(Record::Npc(d))
                if d.name.as_deref().is_some_and(|n| !n.is_empty())
                    && d.models.len() >= 2
                    && d.scale_xz == 128
                    && d.scale_y == 128
                    && seq(d.stand_anim)
                    && seq(d.walk_anim))
        })
        .expect("an NPC with a name, two or more models, and stand and walk sequences")
}
