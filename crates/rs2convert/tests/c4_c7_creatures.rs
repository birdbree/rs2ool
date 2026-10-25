//! Creature assembly, rig inference, clip conversion and validation, on a
//! humanoid and on non-humanoids, plus the player kit.
//!
//! Subjects are chosen by structure: the humanoid is the first NPC with
//! several body models whose mesh carries a label from every region of the
//! default body; the non-humanoids are the first NPCs without that shape
//! whose animations use frame packs no earlier pick uses, plus the first
//! scaled one.

mod common;

use std::collections::BTreeSet;

use rs2cache::Records;
use rs2cache::types::record::{Record, RecordType};
use rs2convert::creature::{self, Creature};
use rs2convert::palette::Palette;
use rs2convert::pipeline::{self, CreatureOutput};
use rs2convert::rig::PLAYER_REGIONS;
use rs2convert::validate::THRESHOLD;

/// How many non-humanoids with distinct frame packs to validate.
const NON_HUMANOIDS: usize = 6;

fn palette() -> Palette {
    // Validation does not depend on colours; a one-entry palette keeps the
    // test fast.
    Palette::from_rgb(vec![[128, 128, 128]])
}

/// NPC `id` assembled, when its definition has distinct stand and walk
/// sequences that both resolve and its mesh has vertex labels.
fn animated(records: &Records, index: &rs2cache::anim::FrameIndex, id: u32) -> Option<Creature> {
    let Ok(Record::Npc(d)) = records.decode(RecordType::Npc, id) else {
        return None;
    };
    let (Some(stand), Some(walk)) = (d.stand_anim, d.walk_anim) else {
        return None;
    };
    if stand == walk {
        return None;
    }
    let c = creature::npc(records, index, id, &[]).ok()?;
    (!c.mesh_labels().is_empty() && c.sequences.len() >= 2).then_some(c)
}

/// Whether a mesh has at least one label from every default body region.
fn body_shaped(c: &Creature) -> bool {
    let labels = c.mesh_labels();
    PLAYER_REGIONS
        .iter()
        .all(|(_, ls)| ls.iter().any(|l| labels.contains(l)))
}

/// The first humanoid-style NPC: at least two body models, animated, and
/// body-shaped labels.
fn humanoid(cache: &rs2cache::Cache) -> u32 {
    let records = Records::new(cache);
    let index = cache.frame_index().unwrap();
    records
        .ids(RecordType::Npc)
        .unwrap()
        .into_iter()
        .find(|&id| {
            let Ok(Record::Npc(d)) = records.decode(RecordType::Npc, id) else {
                return false;
            };
            d.models.len() >= 2 && animated(&records, &index, id).is_some_and(|c| body_shaped(&c))
        })
        .expect("no NPC with two or more models, stand and walk, and body-shaped labels")
}

/// Non-humanoid NPCs whose label structures differ: the first
/// [`NON_HUMANOIDS`] animated, not body-shaped NPCs whose frame packs
/// don't overlap an earlier pick's, then the first scaled one if none of
/// those is scaled.
fn non_humanoids(cache: &rs2cache::Cache) -> Vec<u32> {
    let records = Records::new(cache);
    let index = cache.frame_index().unwrap();
    let mut picks = Vec::new();
    let mut seen_packs = BTreeSet::new();
    let mut scaled = false;
    for id in records.ids(RecordType::Npc).unwrap() {
        if picks.len() >= NON_HUMANOIDS && scaled {
            break;
        }
        let Some(c) = animated(&records, &index, id) else {
            continue;
        };
        if body_shaped(&c) {
            continue;
        }
        let packs: BTreeSet<u32> = c.packs.keys().copied().collect();
        if !packs.is_disjoint(&seen_packs) {
            continue;
        }
        let is_scaled = c.scale != [128; 3];
        if picks.len() < NON_HUMANOIDS || is_scaled {
            seen_packs.extend(packs);
            picks.push(id);
            scaled |= is_scaled;
        }
    }
    assert!(
        picks.len() >= NON_HUMANOIDS && scaled,
        "found only {picks:?} as non-humanoid subjects"
    );
    picks
}

fn convert(cache: &rs2cache::Cache, id: u32) -> CreatureOutput {
    let records = Records::new(cache);
    let index = cache.frame_index().unwrap();
    let c = creature::npc(&records, &index, id, &[]).unwrap();
    pipeline::convert(c, &palette(), id, true, None).unwrap()
}

#[test]
fn c4_humanoid_assembles_into_one_labelled_mesh() {
    let Some(cache) = common::cache() else { return };
    let records = Records::new(&cache);
    let index = cache.frame_index().unwrap();
    let id = humanoid(&cache);
    let Record::Npc(def) = records.decode(RecordType::Npc, id).unwrap() else {
        unreachable!()
    };
    let npc = creature::npc(&records, &index, id, &[]).unwrap();
    eprintln!("humanoid subject: npc {id}");
    match def.name {
        Some(name) => assert_eq!(npc.name, name),
        None => assert_eq!(npc.name, format!("npc {id}")),
    }
    assert!(!npc.name.is_empty());
    assert!(!npc.model.faces.is_empty());
    let mesh = npc.mesh_labels();
    let packs = npc.pack_labels();
    let missing: BTreeSet<_> = mesh.difference(&packs).collect();
    // Label 0 is the unanimated default group.
    assert!(
        missing.iter().all(|&&l| l == 0),
        "mesh labels no frame base moves: {missing:?}"
    );
    assert!(npc.sequences.len() >= 2, "stand and walk");
}

#[test]
fn c5_humanoid_rig() {
    let Some(cache) = common::cache() else { return };
    let out = convert(&cache, humanoid(&cache));
    let rig = &out.inferred;
    // Roughly one bone per default body region.
    let regions = PLAYER_REGIONS.len();
    assert!(
        (regions - 4..=regions + 4).contains(&rig.bones.len()),
        "{} bones for {regions} body regions",
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
    let out = convert(&cache, humanoid(&cache));
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
fn c7_humanoid_validates() {
    let Some(cache) = common::cache() else { return };
    let id = humanoid(&cache);
    let out = convert(&cache, id);
    assert!(!out.clips.is_empty());
    for (_, _, r) in &out.clips {
        eprintln!(
            "npc {id} seq {}: max {:.2}, mean {:.2}",
            r.sequence, r.max, r.mean
        );
        assert!(r.max < THRESHOLD, "seq {} max {}", r.sequence, r.max);
    }
}

#[test]
fn c7_non_humanoids_validate() {
    let Some(cache) = common::cache() else { return };
    for id in non_humanoids(&cache) {
        let out = convert(&cache, id);
        assert!(out.inferred.is_tree(), "npc {id} rig is not a tree");
        assert!(!out.clips.is_empty(), "npc {id} has no clips");
        eprintln!(
            "npc {id}: {} labels, scale {:?}, {} bones, max {:.2}",
            out.creature.mesh_labels().len(),
            out.creature.scale,
            out.rig.bones.len(),
            out.max_error()
        );
        assert!(!out.flagged(), "npc {id} max {}", out.max_error());
    }
}

#[test]
fn override_replaces_a_parent() {
    let Some(cache) = common::cache() else { return };
    let records = Records::new(&cache);
    let index = cache.frame_index().unwrap();
    let id = humanoid(&cache);
    let auto = convert(&cache, id);
    let bone = |n: &str| {
        auto.inferred
            .bones
            .iter()
            .find(|b| b.name == n)
            .unwrap_or_else(|| panic!("no {n} bone"))
    };
    let (head, pelvis) = (bone("head"), bone("pelvis"));
    assert_ne!(
        auto.inferred.bones[head.parent.unwrap()].labels,
        pelvis.labels,
        "the override must change something"
    );
    let table = vec![(
        head.labels.iter().copied().collect(),
        pelvis.labels.iter().copied().collect(),
    )];
    let c = creature::npc(&records, &index, id, &[]).unwrap();
    let out = pipeline::convert(c, &palette(), id, false, Some(&table)).unwrap();
    let bones = &out.inferred.bones;
    let h = bones.iter().find(|b| b.labels == head.labels).unwrap();
    assert_eq!(bones[h.parent.unwrap()].labels, pelvis.labels);
}

#[test]
fn player_kit_parts_and_stance() {
    let Some(cache) = common::cache() else { return };
    let records = Records::new(&cache);
    let index = cache.frame_index().unwrap();
    let kit = pipeline::player_kit(&records, &index).unwrap();
    let idk = records.ids(RecordType::Idk).unwrap();
    assert_eq!(kit.parts.len(), idk.len(), "one kit part per idk entry");
    assert!(kit.parts.iter().all(|(p, _)| p.slot < 7 && p.body_type < 2));
    assert!(
        kit.parts.iter().all(|(p, _)| p.hide_mask == 0),
        "kit parts hide nothing"
    );
    // Both default bodies rig, convert every stance sequence and validate.
    let female = kit.female.expect("the kit has female parts");
    for (body, rig) in [
        (kit.male, pipeline::ids::PLAYER_RIG),
        (female, pipeline::ids::PLAYER_FEMALE_RIG),
    ] {
        let name = body.name.clone();
        let out = pipeline::convert(body, &palette(), rig, true, None).unwrap();
        assert_eq!(out.clips.len(), pipeline::PLAYER_SEQUENCES.len(), "{name}");
        assert!(!out.flagged(), "{name} stance max {}", out.max_error());
    }
}
