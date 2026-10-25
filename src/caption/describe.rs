//! Sequence descriptions written into the anims pack, for
//! `rs2ool pack npc --describe` and `pack player-kit --describe`.
//!
//! ```text
//! extra.descriptions: { "<clip id>": { "seq", "class", "role", "text", "by", "reviewed" } }
//! ```
//!
//! One entry per converted clip that has a caption ([`Store::best`]). The
//! clip records are untouched, and with no captions there's no table, so
//! the packs are byte-identical to a conversion without descriptions.

use std::collections::BTreeMap;

use anyhow::Result;
use contentpack::Kind;
use rs2cache::Cache;
use rs2convert::run::Outcome;
use serde_json::{Value, json};

use super::classify::role_of;
use super::store::Store;

/// The captions for `cache`, from the platform data directory: empty when
/// there's no data directory or no caption file yet.
pub fn store_for(cache: &Cache) -> Result<Store> {
    let crc = rs2convert::source_cache_crc(cache)?;
    match crate::settings::data_dir() {
        Some(dir) => Store::open(&dir, crc),
        None => Store::open(&std::env::temp_dir().join("rs2ool-no-data-dir"), crc),
    }
}

/// Adds a `descriptions` table to `outcome`'s anims pack for every clip
/// with a caption, preferring captions drawn on NPC `npc`. Returns how
/// many clips got one.
pub fn describe(outcome: &mut Outcome, store: &Store, npc: Option<u32>) -> usize {
    let mut table: BTreeMap<String, Value> = BTreeMap::new();
    for (_, c) in &outcome.creatures {
        for (id, _, report) in &c.clips {
            let Some(e) = store.best(npc, report.sequence) else {
                continue;
            };
            // A reviewed caption's checked class, else its own.
            let class = e.reviewed_class().or(e.class.as_deref());
            table.insert(
                id.to_string(),
                json!({
                    "seq": report.sequence,
                    "class": class,
                    "role": class.and_then(role_of).map(|r| r.name()),
                    "text": e.text,
                    "by": e.model,
                    "reviewed": e.review.is_some(),
                }),
            );
        }
    }
    let n = table.len();
    if n > 0
        && let Some(anims) = outcome.packs.iter_mut().find(|p| p.kind() == Kind::Anims)
    {
        anims.set_extra("descriptions", Value::Object(table.into_iter().collect()));
    }
    n
}
