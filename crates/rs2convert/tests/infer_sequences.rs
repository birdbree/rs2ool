//! Sequence inference finds sequences the cache does name: with only an
//! NPC's stand sequence to go on, its walk sequence ranks near the top of
//! the candidates. Reports the hit rates, split between creatures and the
//! shared humanoid pool, where ranking means little.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use rs2cache::Records;
use rs2cache::types::record::{Record, RecordType};
use rs2convert::infer::SequenceIndex;

#[test]
fn held_out_walk_sequences_rank_near_the_top() {
    let Some(cache) = common::cache() else { return };
    let records = Records::new(&cache);
    let index = SequenceIndex::build(&cache, &records, &cache.frame_index().unwrap()).unwrap();

    let mut labels: BTreeMap<u16, Option<BTreeSet<u8>>> = BTreeMap::new();
    let real = |a: Option<u16>| a.filter(|&a| a != u16::MAX).map(u32::from);
    // Per kind of NPC: walk ranks (None when it isn't a candidate at all).
    let (mut creatures, mut shared): (Vec<Option<usize>>, Vec<Option<usize>>) = Default::default();
    // NPCs sharing models and sequences only count once.
    let mut seen = BTreeSet::new();
    for id in records.ids(RecordType::Npc).unwrap() {
        let Ok(Record::Npc(n)) = records.decode(RecordType::Npc, id) else {
            continue;
        };
        let (Some(stand), Some(walk)) = (real(n.stand_anim), real(n.walk_anim)) else {
            continue;
        };
        if stand == walk || !seen.insert((n.models.clone(), stand, walk)) {
            continue;
        }
        let mut model_labels = BTreeSet::new();
        let mut ok = true;
        for &m in &n.models {
            let l = labels.entry(m).or_insert_with(|| {
                match records.decode(RecordType::Model, m.into()) {
                    Ok(Record::Model(md)) => {
                        Some(md.vertex_labels.unwrap_or_default().into_iter().collect())
                    }
                    _ => None,
                }
            });
            match l {
                Some(l) => model_labels.extend(l.iter().copied()),
                None => ok = false,
            }
        }
        if !ok {
            continue;
        }
        let c = index.candidates(&[stand], &model_labels);
        let Some(first) = c.first() else { continue };
        let rank = c.iter().position(|c| c.seq == walk);
        if first.shared() {
            shared.push(rank);
        } else {
            creatures.push(rank);
        }
    }
    let report = |name: &str, v: &[Option<usize>]| {
        let within = |k: usize| v.iter().filter(|r| r.is_some_and(|r| r < k)).count();
        let found = v.iter().filter(|r| r.is_some()).count();
        eprintln!(
            "{name}: {} NPCs; walk is a candidate for {found}, first for {}, top 5 for {}, top 20 for {}",
            v.len(),
            within(1),
            within(5),
            within(20)
        );
        (v.len(), within(5))
    };
    let (n, top5) = report("creatures", &creatures);
    report("shared pool", &shared);
    assert!(n > 100, "enough creatures to judge by");
    // Creatures are where the ranking is meant to work.
    assert!(
        top5 * 10 >= n * 7,
        "walk in the top 5 for at least 70% of creatures: {top5} of {n}"
    );
}

/// Roles from motion on the sequences the cache does name: creatures'
/// stand sequences come out idle and their walk sequences as actions, most
/// creatures have a death among their candidates, and each death really
/// ends collapsed.
#[test]
fn roles_match_the_named_sequences() {
    use rs2convert::creature;
    use rs2convert::infer::{Motion, Role, with_roles};
    let Some(cache) = common::cache() else { return };
    let records = Records::new(&cache);
    let frames = cache.frame_index().unwrap();
    let index = SequenceIndex::build(&cache, &records, &frames).unwrap();
    let real = |a: Option<u16>| a.filter(|&a| a != u16::MAX);
    let mut seen = BTreeSet::new();
    let (mut stands, mut walks) = (
        BTreeMap::<&str, usize>::new(),
        BTreeMap::<&str, usize>::new(),
    );
    let (mut creatures, mut with_death) = (0, 0);
    for id in records.ids(RecordType::Npc).unwrap() {
        let Ok(Record::Npc(n)) = records.decode(RecordType::Npc, id) else {
            continue;
        };
        let (Some(stand), Some(walk)) = (real(n.stand_anim), real(n.walk_anim)) else {
            continue;
        };
        if stand == walk || !seen.insert((stand, walk)) {
            continue;
        }
        let Ok(named) = creature::npc(&records, &frames, id, &[]) else {
            continue;
        };
        let labels: BTreeSet<u8> = named
            .model
            .vertex_labels
            .iter()
            .flatten()
            .copied()
            .collect();
        let mut cands = index.candidates(&[u32::from(stand), u32::from(walk)], &labels);
        if cands.first().is_none_or(|c| c.shared()) {
            continue;
        }
        cands.truncate(16);
        let extra: Vec<u16> = cands.iter().map(|c| c.seq as u16).collect();
        let Ok(c) = creature::npc(&records, &frames, id, &extra) else {
            continue;
        };
        creatures += 1;
        let role_of = |seq: u16| {
            let s = c
                .sequences
                .iter()
                .position(|q| q.def.id == u32::from(seq))?;
            Motion::measure(&c, s).map(|m| m.role())
        };
        if let Some(r) = role_of(stand) {
            *stands.entry(r.name()).or_default() += 1;
        }
        if let Some(r) = role_of(walk) {
            *walks.entry(r.name()).or_default() += 1;
        }
        with_roles(&c, &mut cands);
        with_death += usize::from(cands.iter().any(|c| c.role == Some(Role::Death)));
        for cand in cands.iter().filter(|c| c.role == Some(Role::Death)) {
            let s = c
                .sequences
                .iter()
                .position(|q| q.def.id == cand.seq)
                .unwrap();
            let m = Motion::measure(&c, s).unwrap();
            assert!(m.drop >= 0.3 && m.end_height <= 0.75);
        }
    }
    eprintln!(
        "{creatures} creatures; stand sequences: {stands:?}; walk sequences: {walks:?}; {with_death} have a death candidate"
    );
    let share = |m: &BTreeMap<&str, usize>, k: &str| {
        m.get(k).copied().unwrap_or(0) as f64 / m.values().sum::<usize>().max(1) as f64
    };
    assert!(creatures > 100);
    assert!(share(&stands, "idle") >= 0.6, "stands idle: {stands:?}");
    assert!(
        share(&walks, "action") >= 0.6,
        "walks are actions: {walks:?}"
    );
    assert!(
        with_death * 2 >= creatures,
        "half the creatures have a death candidate: {with_death} of {creatures}"
    );
}
