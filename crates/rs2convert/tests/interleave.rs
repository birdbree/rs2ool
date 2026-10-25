//! Secondary frames and interleaving against the cache in `RS2_CACHE`
//! (skipped without one).
//!
//! Chat-head sequences apply a secondary frame after each frame, and their
//! clips hold both. Actions with an interleave list mix with the player's
//! walk and run: the reference follows the original client's rules, the
//! clips give the legs to the movement, and mixed playback stays within
//! the mix threshold.

use std::collections::BTreeSet;

use rs2cache::Records;
use rs2cache::types::anim::kind;
use rs2cache::types::record::{Record, RecordType};
use rs2convert::creature::{self, Creature};
use rs2convert::palette::Palette;
use rs2convert::validate::{MIX_THRESHOLD, validate};
use rs2convert::{deform, pipeline, preview};

mod common;

fn seqs_where(records: &Records, f: impl Fn(&rs2cache::types::config::SeqDef) -> bool) -> Vec<u16> {
    records
        .ids(RecordType::Seq)
        .unwrap()
        .into_iter()
        .filter(|&s| matches!(records.decode(RecordType::Seq, s), Ok(Record::Seq(d)) if f(&d)))
        .map(|s| s as u16)
        .collect()
}

fn palette() -> Palette {
    Palette::from_rgb(vec![[0, 0, 0]])
}

#[test]
fn chat_heads_apply_their_secondary_frames() {
    let Some(cache) = common::cache() else { return };
    let records = Records::new(&cache);
    let index = cache.frame_index().unwrap();
    let chat = seqs_where(&records, |d| d.frames.iter().any(|f| f.secondary.is_some()));
    assert!(!chat.is_empty(), "some sequences have secondary frames");

    // Every secondary frame is in a pack.
    let (sequences, packs) = creature::sequences(&records, &index, &chat).unwrap();
    for s in &sequences {
        for (f, p) in s.def.frames.iter().zip(&s.secondary) {
            assert_eq!(f.secondary.is_some(), p.is_some(), "seq {}", s.def.id);
        }
    }

    // A chat head: the first NPC with head models whose labels the
    // sequences move.
    let head = records
        .ids(RecordType::Npc)
        .unwrap()
        .into_iter()
        .find_map(|id| match records.decode(RecordType::Npc, id) {
            Ok(Record::Npc(d)) if !d.chat_head_models.is_empty() => {
                let models: Vec<_> = d
                    .chat_head_models
                    .iter()
                    .map(|&m| match records.decode(RecordType::Model, m.into()) {
                        Ok(Record::Model(m)) => m,
                        _ => unreachable!(),
                    })
                    .collect();
                let mut m = creature::merge(&models);
                creature::recolour(&mut m, &d.recolours);
                m.vertex_labels.is_some().then_some((id, m))
            }
            _ => None,
        })
        .expect("an NPC with a labelled chat head");
    let (id, model) = head;
    let c = Creature::new(format!("npc {id} head"), model, [128; 3], sequences, packs);

    // The reference applies the frame, then the secondary frame, each from
    // a zero origin.
    let groups = c.groups();
    for (k, s) in c.sequences.iter().enumerate() {
        for i in 0..s.def.frames.len() {
            let frames = c.frames(s, i).unwrap();
            assert_eq!(
                frames.len(),
                1 + usize::from(s.def.frames[i].secondary.is_some())
            );
            let mut v = c.model.vertices.clone();
            for (pack, f) in &frames {
                deform::apply_frame(&mut v, &groups, &pack.base, f);
            }
            assert_eq!(c.posed(k, i).unwrap(), v, "seq {} frame {i}", s.def.id);
        }
    }

    // The converted clips hold both frames: against the reference, they
    // land far closer than clips converted from the first frames alone.
    // (A chat head is a poor fit for a rig either way, and no pack holds
    // one, so the errors themselves aren't the point.)
    let mut bare = c.clone();
    for q in &mut bare.sequences {
        q.secondary.iter_mut().for_each(|p| *p = None);
    }
    let both = pipeline::convert(c.clone(), &palette(), 0, true, None).unwrap();
    let first = pipeline::convert(bare, &palette(), 0, true, None).unwrap();
    let (mut with, mut without) = (0.0, 0.0);
    for (k, s) in c.sequences.iter().enumerate() {
        with += both.clips[k].2.max;
        without += validate(&c, &first.rig, &first.clips[k].1, s).max;
    }
    let n = c.sequences.len() as f64;
    println!(
        "npc {id} head: {} chat clips, mean worst error {:.2} with secondary frames, {:.2} without",
        both.clips.len(),
        with / n,
        without / n
    );
    assert!(with * 2.0 < without);
}

#[test]
fn actions_mix_with_the_walk_and_run() {
    let Some(cache) = common::cache() else { return };
    let records = Records::new(&cache);
    let index = cache.frame_index().unwrap();
    let actions = seqs_where(&records, |d| d.interleave_labels.is_some());
    let walk_base = {
        let c = preview::player_body(&records, &index, false, &[])
            .unwrap()
            .creature;
        let w = c.sequences.iter().find(|s| s.def.id == 819).unwrap();
        c.frame(w, 0).unwrap().0.base.clone()
    };

    let legs: BTreeSet<&str> = [
        "pelvis", "r_thigh", "l_thigh", "r_shin", "l_shin", "r_foot", "l_foot",
    ]
    .into();
    let mut checked = 0;
    let mut worst: f64 = 0.0;
    let mut by_movement = std::collections::BTreeMap::<u32, f64>::new();
    for &a in &actions {
        let c = preview::player_body(&records, &index, false, &[a])
            .unwrap()
            .creature;
        let s = c
            .sequences
            .iter()
            .position(|q| q.def.id == u32::from(a))
            .unwrap();
        if c.frame(&c.sequences[s], 0).unwrap().0.base != walk_base {
            // Made for another skeleton: the walk's slots mean other
            // things in its base.
            continue;
        }
        checked += 1;
        let w = c.sequences.iter().position(|q| q.def.id == 819).unwrap();
        let base = &walk_base;
        let slots = c.sequences[s].interleave().unwrap();

        // Labels only listed slots move follow the walk exactly, as long
        // as nothing the action moves sets the walk's origins.
        let moved_by = |listed: bool| -> BTreeSet<u8> {
            base.iter()
                .enumerate()
                .filter(|(i, b)| b.kind != kind::ORIGIN && slots.contains(&(*i as u8)) == listed)
                .flat_map(|(_, b)| b.labels.iter().copied())
                .collect()
        };
        let only_listed: BTreeSet<u8> = &moved_by(true) - &moved_by(false);
        let labels = c.model.vertex_labels.clone().unwrap();
        for (i, j) in [(0, 0), (1, 3), (2, 5)] {
            let i = i.min(c.sequences[s].def.frames.len() - 1);
            let mixed = c.posed_moving(s, i, w, j).unwrap();
            let walking = c.posed(w, j).unwrap();
            for (v, l) in labels.iter().enumerate() {
                if only_listed.contains(l) {
                    assert_eq!(mixed[v], walking[v], "seq {a}: vertex {v} (label {l})");
                }
            }
        }

        // The legs go to the movement clip, and the mix stays close.
        let out = pipeline::convert(c, &palette(), 0, true, None).unwrap();
        let (_, clip, report) = &out.clips[s];
        let names: BTreeSet<&str> = clip
            .moving
            .iter()
            .map(|&b| out.inferred.bones[usize::from(b)].name.as_str())
            .collect();
        assert_eq!(names, legs, "seq {a}");
        assert!(!clip.mix_keys.is_empty());
        let mixes: BTreeSet<u32> = report.moving.iter().map(|m| m.sequence).collect();
        assert_eq!(mixes, [819, 820, 821, 822, 823, 824].into(), "seq {a}");
        for m in &report.moving {
            worst = worst.max(m.max);
            let w = by_movement.entry(m.sequence).or_default();
            *w = w.max(m.max);
            assert!(!m.flagged, "seq {a} over seq {}: {:.2}", m.sequence, m.max);
        }
    }
    println!("{checked} actions mixed; worst by movement sequence {by_movement:.2?}");
    assert!(
        checked > 100,
        "{checked} actions share the walk's frame base"
    );
    assert!(worst <= MIX_THRESHOLD);
}
