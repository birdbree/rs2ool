//! Phases C4–C7 checks: creature assembly, rig inference, clip conversion
//! and validation, on Man and on three non-humanoids, plus the player kit.

mod common;

use std::collections::BTreeSet;

use rs2cache::Records;
use rs2convert::creature;
use rs2convert::palette::Palette;
use rs2convert::pipeline::{self, CreatureOutput};
use rs2convert::validate::THRESHOLD;

fn palette() -> Palette {
    // Validation does not depend on colours; a one-entry palette keeps the
    // test fast.
    Palette::from_rgb(vec![[128, 128, 128]])
}

fn convert(cache: &rs2cache::Cache, id: u32) -> CreatureOutput {
    let records = Records::new(cache);
    let index = cache.frame_index().unwrap();
    let c = creature::npc(&records, &index, id, &[]).unwrap();
    pipeline::convert(c, &palette(), id, true, None).unwrap()
}

#[test]
fn c4_man_assembles_into_one_labelled_mesh() {
    let Some(cache) = common::cache() else { return };
    let records = Records::new(&cache);
    let index = cache.frame_index().unwrap();
    let man = creature::npc(&records, &index, 1, &[]).unwrap();
    assert_eq!(man.name, "Man");
    assert!(!man.model.faces.is_empty());
    let mesh = man.mesh_labels();
    let packs = man.pack_labels();
    let missing: BTreeSet<_> = mesh.difference(&packs).collect();
    // Label 0 is the unanimated default group.
    assert!(
        missing.iter().all(|&&l| l == 0),
        "mesh labels no frame base moves: {missing:?}"
    );
    assert!(man.sequences.len() >= 2, "stand and walk");
}

#[test]
fn c5_man_rig() {
    let Some(cache) = common::cache() else { return };
    let out = convert(&cache, 1);
    let rig = &out.inferred;
    assert!(
        (12..=20).contains(&rig.bones.len()),
        "{} bones",
        rig.bones.len()
    );
    assert!(rig.is_tree(), "one root, parents first, no cycles");
    assert_eq!(rig.bones.iter().filter(|b| b.parent.is_none()).count(), 1);
    let named = |n: &str| {
        rig.bones
            .iter()
            .position(|b| b.name == n)
            .unwrap_or_else(|| panic!("no {n} bone"))
    };
    assert_eq!(rig.bones[named("head")].parent, Some(named("torso")));
}

#[test]
fn c6_clips_cover_every_frame() {
    let Some(cache) = common::cache() else { return };
    let out = convert(&cache, 1);
    for (s, (_, clip, _)) in out.creature.sequences.iter().zip(&out.clips) {
        let frames: BTreeSet<u8> = clip.keys.iter().map(|k| k.t).collect();
        assert_eq!(
            frames.len(),
            s.def.frames.len(),
            "seq {} key frames",
            s.def.id
        );
        assert_eq!(clip.keys.len(), s.def.frames.len() * out.rig.bones.len());
        let delays: u32 = rs2convert::clip::delays(&out.creature, s)
            .iter()
            .map(|&d| u32::from(d))
            .sum();
        assert_eq!(
            u32::from(clip.duration),
            delays,
            "seq {} duration",
            s.def.id
        );
    }
}

#[test]
fn c7_man_validates() {
    let Some(cache) = common::cache() else { return };
    let out = convert(&cache, 1);
    for (_, _, r) in &out.clips {
        eprintln!(
            "Man seq {}: max {:.2}, mean {:.2}",
            r.sequence, r.max, r.mean
        );
        assert!(r.max < THRESHOLD, "seq {} max {}", r.sequence, r.max);
    }
}

#[test]
fn c7_non_humanoids_validate() {
    let Some(cache) = common::cache() else { return };
    // A bird, a quadruped and a multi-limbed (and scaled) creature.
    for (id, what) in [(41, "Chicken"), (81, "Cow"), (50, "King black dragon")] {
        let out = convert(&cache, id);
        assert_eq!(out.creature.name, what);
        assert!(out.inferred.is_tree());
        eprintln!(
            "{what}: {} bones, max {:.2}",
            out.rig.bones.len(),
            out.max_error()
        );
        assert!(!out.flagged(), "{what} max {}", out.max_error());
    }
}

#[test]
fn override_replaces_a_parent() {
    let Some(cache) = common::cache() else { return };
    let records = Records::new(&cache);
    let index = cache.frame_index().unwrap();
    let auto = convert(&cache, 1);
    let head = auto
        .inferred
        .bones
        .iter()
        .find(|b| b.name == "head")
        .unwrap();
    let pelvis = auto
        .inferred
        .bones
        .iter()
        .find(|b| b.name == "pelvis")
        .unwrap();
    let table = vec![(
        head.labels.iter().copied().collect(),
        pelvis.labels.iter().copied().collect(),
    )];
    let c = creature::npc(&records, &index, 1, &[]).unwrap();
    let out = pipeline::convert(c, &palette(), 1, false, Some(&table)).unwrap();
    let bones = &out.inferred.bones;
    let h = bones.iter().find(|b| b.labels == head.labels).unwrap();
    assert_eq!(bones[h.parent.unwrap()].labels, pelvis.labels);
}

#[test]
fn player_kit_parts_and_stance() {
    let Some(cache) = common::cache() else { return };
    let records = Records::new(&cache);
    let index = cache.frame_index().unwrap();
    let (parts, body) = pipeline::player_kit(&records, &index).unwrap();
    let idk = records
        .ids(rs2cache::types::record::RecordType::Idk)
        .unwrap();
    assert_eq!(parts.len(), idk.len(), "one kit part per idk entry");
    assert!(parts.iter().all(|(p, _)| p.slot < 7 && p.body_type < 2));
    let out = pipeline::convert(body, &palette(), pipeline::ids::PLAYER_RIG, true, None).unwrap();
    assert_eq!(out.clips.len(), pipeline::PLAYER_SEQUENCES.len());
    assert!(!out.flagged(), "stance max {}", out.max_error());
}
