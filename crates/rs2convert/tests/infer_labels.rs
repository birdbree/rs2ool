//! Roles from motion against sequences labelled by eye. The labels stay
//! local: set `RS2_SEQUENCE_LABELS` to a JSON file of
//! `[{"npc": 1, "seq": 2820, "class": "death", "note": "…"}, …]`. Without
//! it, or without `RS2_CACHE`, the test skips.
//!
//! Reports the confusion between the labelled classes and the roles, and
//! checks the current rules tell deaths apart better than the original
//! single death rule (the top drops 30% and it ends at most 75% as tall).

mod common;

use std::collections::BTreeMap;

use rs2cache::Records;
use rs2convert::creature;
use rs2convert::infer::{Motion, Role};

#[test]
fn roles_against_hand_labels() {
    let Some(cache) = common::cache() else { return };
    let Some(path) = std::env::var_os("RS2_SEQUENCE_LABELS") else {
        eprintln!("skipping: RS2_SEQUENCE_LABELS is not set");
        return;
    };
    let labels: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let records = Records::new(&cache);
    let frames = cache.frame_index().unwrap();
    let old = |m: &Motion| m.drop >= 0.3 && m.end_height <= 0.75;
    // (class, role) counts, and death hits for the old and new rules:
    // [true positive, false positive, false negative].
    let mut confusion: BTreeMap<(String, &str), usize> = BTreeMap::new();
    let (mut was, mut now) = ([0; 3], [0; 3]);
    for l in &labels {
        let npc = l["npc"].as_u64().unwrap() as u32;
        let seq = l["seq"].as_u64().unwrap() as u16;
        let class = l["class"].as_str().unwrap().to_owned();
        let c = creature::npc(&records, &frames, npc, &[seq]).unwrap();
        let s = c
            .sequences
            .iter()
            .position(|q| q.def.id == u32::from(seq))
            .unwrap();
        let m = Motion::measure(&c, s).unwrap();
        let role = m.role();
        *confusion.entry((class.clone(), role.name())).or_default() += 1;
        let truth = class == "death";
        for (counts, said) in [(&mut was, old(&m)), (&mut now, role == Role::Death)] {
            match (truth, said) {
                (true, true) => counts[0] += 1,
                (false, true) => counts[1] += 1,
                (true, false) => counts[2] += 1,
                (false, false) => {}
            }
        }
    }
    eprintln!("{} labelled sequences; class → role:", labels.len());
    for ((class, role), n) in &confusion {
        eprintln!("  {class:<10} → {role:<7} {n}");
    }
    let precision = |c: [usize; 3]| c[0] as f64 / (c[0] + c[1]).max(1) as f64;
    let recall = |c: [usize; 3]| c[0] as f64 / (c[0] + c[2]).max(1) as f64;
    eprintln!(
        "deaths: old rule precision {:.2} recall {:.2}; now precision {:.2} recall {:.2}",
        precision(was),
        recall(was),
        precision(now),
        recall(now)
    );
    assert!(labels.len() >= 15, "at least 15 labelled sequences");
    assert!(precision(now) > precision(was), "deaths told apart better");
    assert!(recall(now) >= recall(was), "without losing deaths");
}
