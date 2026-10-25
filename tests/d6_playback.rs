//! An NPC's walk sequence plays at the original client's speed (frame
//! delays at 20 ms per cycle), and its converted version plays the same
//! clip in step: at every frame the skinned pack vertices sit where the
//! reference deformation puts the cache vertices. The bone overlay's
//! skeleton moves with the skinned mesh. An action with an interleave list
//! plays over the walk on both sides.

#![cfg(feature = "explorer")]

use std::path::PathBuf;

use bevy::math::Vec3;
use contentpack::Kind;
use rs2cache::types::record::{Record, RecordType};
use rs2ool::explorer::bones::skeleton;
use rs2ool::explorer::playback::{CYCLE_MS, Clock, Posed, posed, sequences, timing};
use rs2ool::explorer::renderable::Renderable;
use rs2ool::explorer::selection::{Sel, load};
use rs2ool::explorer::sources::{Source, open_cache, open_pack};

mod common;

#[test]
fn npc_walks_at_client_speed_and_in_step_with_its_conversion() {
    let Some(dir) = std::env::var_os("RS2_CACHE").map(PathBuf::from) else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let cache = open_cache(&dir).unwrap();
    let Source::Cache { records, .. } = &cache else {
        unreachable!()
    };
    // Unscaled, so the converted vertices compare with the cache's as is.
    let npc = common::walking_npc(records);
    let Record::Npc(def) = records.decode(RecordType::Npc, npc).unwrap() else {
        unreachable!()
    };
    let walk = u32::from(def.walk_anim.unwrap());

    let (_, left) = load(&cache, &Sel::Record(RecordType::Npc, npc)).unwrap();
    let left = left.unwrap();
    assert!(
        matches!(left, Renderable::Creature { .. }),
        "npc {npc} plays"
    );
    assert!(sequences(&left).contains(&walk));

    // Speed: the clock follows the sequence's delays, a 0 falling back to
    // the frame's own delay, at 20 ms per cycle.
    let Record::Seq(def) = records.decode(RecordType::Seq, walk).unwrap() else {
        unreachable!()
    };
    let (delays, lo) = timing(&left, walk).unwrap();
    assert_eq!(delays.len(), def.frames.len());
    assert!(delays.iter().all(|&d| d > 0));
    for (d, f) in delays.iter().zip(&def.frames) {
        if f.delay != 0 {
            assert_eq!(*d, f.delay);
        }
    }
    let mut clock = Clock::new(delays.clone(), lo);
    let total: f32 = delays.iter().map(|&d| f32::from(d) * CYCLE_MS).sum();
    assert_eq!(clock.duration_ms(), total);
    clock.advance(f32::from(delays[0]) * CYCLE_MS - 0.5);
    assert_eq!(clock.frame, 0, "frame 0 lasts its delay");
    clock.advance(1.0);
    assert_eq!(clock.frame, 1);

    // The converted NPC, written as `rs2ool pack npc --id <npc> --with-anims`
    // would.
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("d6_playback");
    let _ = std::fs::remove_dir_all(&out);
    let png = |_: &std::path::Path, _: u32, _: u32, _: &[u8]| Ok(());
    let conv = rs2convert::run::npc(records, 0, npc, true, &[], None).unwrap();
    let written = rs2convert::run::write(&conv, &out, png).unwrap();
    let (_, _, models) = written.iter().find(|(k, ..)| *k == Kind::Models).unwrap();
    let pack = open_pack(models).unwrap();
    let id = rs2convert::pipeline::ids::CREATURE_MODEL | npc;
    let (_, right) = load(&pack, &Sel::PackRecord(id)).unwrap();
    let right = right.unwrap();
    assert!(
        matches!(right, Renderable::PackCreature { .. }),
        "the converted NPC finds its rig and clips beside its model"
    );
    assert!(sequences(&right).contains(&walk));
    let (pack_delays, _) = timing(&right, walk).unwrap();
    assert_eq!(pack_delays, delays, "both sides keep the same time");

    // In step: frame by frame, the two poses agree.
    let mut worst: f32 = 0.0;
    for f in 0..delays.len() {
        let (Some(Posed::Cache(c, _)), Some(Posed::Pack(p, _))) =
            (posed(&left, walk, f), posed(&right, walk, f))
        else {
            panic!("frame {f} poses on both sides")
        };
        assert_eq!(c.len(), p.len());
        for (a, b) in c.iter().zip(&p) {
            let want = [a[0] as f32, -a[1] as f32, -a[2] as f32];
            let d =
                ((want[0] - b[0]).powi(2) + (want[1] - b[1]).powi(2) + (want[2] - b[2]).powi(2))
                    .sqrt();
            worst = worst.max(d);
        }
    }
    assert!(worst < 4.0, "worst vertex error over the walk: {worst}");

    // The bone overlay follows the mesh: each vertex keeps its distance to
    // the pivot of the bone it follows, frame by frame.
    let Renderable::PackCreature { model, rig, .. } = &right else {
        unreachable!()
    };
    let bones = rs2convert::validate::vertex_bones(model, rig);
    let rest = skeleton(&right, None, 0, None).unwrap();
    assert_eq!(rest.pivots.len(), rig.bones.len());
    assert!(
        skeleton(&left, Some(walk), 0, None).is_some(),
        "cache NPCs show their inferred rig"
    );
    let mut moved = false;
    for f in 0..delays.len() {
        let s = skeleton(&right, Some(walk), f, None).unwrap();
        moved |= s != rest;
        let Some(Posed::Pack(p, _)) = posed(&right, walk, f) else {
            unreachable!()
        };
        for ((v, q), &b) in model.vertices.iter().zip(&p).zip(&bones) {
            let at_rest = Vec3::from(v.map(f32::from)).distance(rest.pivots[b]);
            let now = Vec3::from(*q).distance(s.pivots[b]);
            assert!(
                (at_rest - now).abs() < 0.5,
                "frame {f}: a vertex on bone {b} drifts from its pivot ({at_rest} vs {now})"
            );
        }
    }
    assert!(moved, "the skeleton moves over the walk");
}

/// An NPC loads with inferred sequences after the ones it names, each
/// with its evidence, and they play like the named ones.
#[test]
fn npc_plays_its_inferred_sequences() {
    let Some(dir) = std::env::var_os("RS2_CACHE").map(PathBuf::from) else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let cache = open_cache(&dir).unwrap();
    let Source::Cache { records, .. } = &cache else {
        unreachable!()
    };
    let npc = common::walking_npc(records);
    let (_, r) = load(&cache, &Sel::Record(RecordType::Npc, npc)).unwrap();
    let r = r.unwrap();
    let Renderable::Creature { inferred, .. } = &r else {
        panic!("npc {npc} plays")
    };
    assert!(!inferred.is_empty(), "npc {npc} gets inferred sequences");
    if !inferred[0].shared() {
        assert!(
            inferred
                .windows(2)
                .all(|w| w[0].role != w[1].role || w[0].score >= w[1].score),
            "best first within each role"
        );
    }
    let seqs = sequences(&r);
    // Named first, inferred after, in rank order.
    let named = seqs.len() - inferred.len();
    assert_eq!(
        seqs[named..],
        inferred.iter().map(|c| c.seq).collect::<Vec<_>>()[..]
    );
    for c in inferred {
        assert!(rs2ool::explorer::playback::inferred(&r, c.seq).is_some());
        let (delays, _) = timing(&r, c.seq).unwrap();
        assert!(!delays.is_empty());
        assert!(matches!(posed(&r, c.seq, 0), Some(Posed::Cache(_, _))));
        assert!(c.role.is_some(), "seq {} has a role", c.seq);
    }
    assert!(rs2ool::explorer::playback::inferred(&r, seqs[0]).is_none());
}

/// A humanoid on the shared pool gets the whole pool, grouped by role:
/// deaths, then actions, then idles, then others.
#[test]
fn humanoids_get_the_whole_pool_grouped_by_role() {
    use rs2convert::infer::Role;
    let Some(dir) = std::env::var_os("RS2_CACHE").map(PathBuf::from) else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let cache = open_cache(&dir).unwrap();
    let (_, r) = load(&cache, &Sel::Record(RecordType::Npc, 1)).unwrap();
    let Some(r) = r else { panic!("npc 1 plays") };
    let Renderable::Creature { inferred, .. } = &r else {
        panic!("npc 1 plays")
    };
    assert!(
        inferred.iter().all(|c| c.shared()),
        "npc 1 is on the shared pool"
    );
    assert!(inferred.len() > 1000, "the whole pool: {}", inferred.len());
    let group = |r: Option<Role>| r.map_or(usize::MAX, |r| r.order());
    assert!(
        inferred
            .windows(2)
            .all(|w| group(w[0].role) <= group(w[1].role))
    );
    assert_eq!(inferred[0].role, Some(Role::Death));
    // The picker lists them in the same order.
    let seqs = sequences(&r);
    let named = seqs.len() - inferred.len();
    assert!(seqs[named..].iter().zip(inferred).all(|(s, c)| *s == c.seq));
}

/// A cache NPC shows the bones conversion would infer for it, each pivot
/// following the centroid of its vertices through the animation.
#[test]
fn cache_npcs_show_their_inferred_bones() {
    use rs2ool::explorer::bones::skeleton;
    let Some(dir) = std::env::var_os("RS2_CACHE").map(PathBuf::from) else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let cache = open_cache(&dir).unwrap();
    let Source::Cache { records, .. } = &cache else {
        unreachable!()
    };
    let npc = common::walking_npc(records);
    let (_, r) = load(&cache, &Sel::Record(RecordType::Npc, npc)).unwrap();
    let r = r.unwrap();
    let Renderable::Creature {
        creature,
        rig: Some(rig),
        ..
    } = &r
    else {
        panic!("npc {npc} has an inferred rig")
    };
    let labels = creature.model.vertex_labels.clone().unwrap();
    let bones = rig.vertex_bones(&labels);
    let walk = sequences(&r)[1];
    let rest = skeleton(&r, None, 0, None).unwrap();
    assert_eq!(rest.pivots.len(), rig.bones.len());
    let mut moved = false;
    for f in 0..timing(&r, walk).unwrap().0.len() {
        let Some(Posed::Cache(v, _)) = posed(&r, walk, f) else {
            panic!("frame {f} poses")
        };
        let s = skeleton(&r, Some(walk), f, None).unwrap();
        moved |= s != rest;
        // Each bone with vertices sits at their centroid (cache space,
        // integer division, then y and z negated for the world).
        for (b, bone) in rig.bones.iter().enumerate().skip(1) {
            let mine: Vec<[i32; 3]> = v
                .iter()
                .zip(&labels)
                .filter(|(_, l)| bone.labels.contains(l))
                .map(|(p, _)| *p)
                .collect();
            if mine.is_empty() {
                continue;
            }
            let n = mine.len() as i64;
            let c =
                [0, 1, 2].map(|a| (mine.iter().map(|p| i64::from(p[a])).sum::<i64>() / n) as f32);
            assert_eq!(
                s.pivots[b].to_array(),
                [c[0], -c[1], -c[2]],
                "frame {f}, bone {b}"
            );
        }
    }
    assert!(moved, "the bones move with the walk");
    // Every vertex follows a real bone.
    assert!(bones.iter().all(|&b| b < rig.bones.len()));
}

/// A humanoid's action with an interleave list plays over its walk: the
/// picker offers the walk, the cache side mixes the two as the original
/// client does, and the converted side keeps up through its clip's moving
/// bones.
#[test]
fn an_action_plays_over_the_walk_in_step_with_its_conversion() {
    use rs2ool::explorer::playback::{Playback, movers, posed_moving};
    let Some(dir) = std::env::var_os("RS2_CACHE").map(PathBuf::from) else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let cache = open_cache(&dir).unwrap();
    let Source::Cache { records, .. } = &cache else {
        unreachable!()
    };
    let (_, left) = load(&cache, &Sel::Record(RecordType::Npc, 1)).unwrap();
    let left = left.unwrap();
    let Record::Npc(def) = records.decode(RecordType::Npc, 1).unwrap() else {
        unreachable!()
    };
    let walk = u32::from(def.walk_anim.unwrap());
    assert!(movers(&left, walk).is_empty(), "the walk plays alone");
    // An action that shares the walk's frame base.
    let Renderable::Creature { creature, .. } = &left else {
        panic!("npc 1 plays")
    };
    let base = |id: u32| {
        let s = creature.sequences.iter().find(|s| s.def.id == id)?;
        creature.frame(s, 0).map(|(p, _)| p.base.clone())
    };
    let action = sequences(&left)
        .into_iter()
        .find(|&s| movers(&left, s).contains(&walk) && base(s) == base(walk))
        .expect("npc 1 can play an action over its walk");

    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("d6_playback_moving");
    let _ = std::fs::remove_dir_all(&out);
    let png = |_: &std::path::Path, _: u32, _: u32, _: &[u8]| Ok(());
    let conv = rs2convert::run::npc(records, 0, 1, true, &[action as u16], None).unwrap();
    let written = rs2convert::run::write(&conv, &out, png).unwrap();
    let (_, _, models) = written.iter().find(|(k, ..)| *k == Kind::Models).unwrap();
    let pack = open_pack(models).unwrap();
    let id = rs2convert::pipeline::ids::CREATURE_MODEL | 1;
    let (_, right) = load(&pack, &Sel::PackRecord(id)).unwrap();
    let right = right.unwrap();
    assert!(movers(&right, action).contains(&walk));

    // Frame pairs mixed on both sides agree within the mix threshold, and
    // mixing changes the pose.
    let (n, m) = (
        timing(&left, action).unwrap().0.len(),
        timing(&left, walk).unwrap().0.len(),
    );
    let (mut worst, mut changed): (f32, bool) = (0.0, false);
    for i in 0..n {
        for j in [0, m / 2] {
            let (Some(Posed::Cache(c, _)), Some(Posed::Pack(p, _))) = (
                posed_moving(&left, action, i, Some((walk, j))),
                posed_moving(&right, action, i, Some((walk, j))),
            ) else {
                panic!("frame {i} over walk frame {j} poses on both sides")
            };
            let Some(Posed::Cache(alone, _)) = posed(&left, action, i) else {
                unreachable!()
            };
            changed |= alone != c;
            for (a, b) in c.iter().zip(&p) {
                let want = [a[0] as f32, -a[1] as f32, -a[2] as f32];
                let d = ((want[0] - b[0]).powi(2)
                    + (want[1] - b[1]).powi(2)
                    + (want[2] - b[2]).powi(2))
                .sqrt();
                worst = worst.max(d);
            }
        }
    }
    assert!(changed, "the walk changes the action's pose");
    assert!(
        f64::from(worst) <= rs2convert::validate::MIX_THRESHOLD,
        "worst vertex error mixed: {worst}"
    );

    // Choosing a sequence that doesn't mix stops the movement.
    let mut pb = Playback::default();
    pb.seqs = sequences(&left);
    let at = |s| pb.seqs.iter().position(|&x| x == s).unwrap();
    let (a, w) = (at(action), at(walk));
    pb.choose(a, &left);
    pb.set_moving(Some(walk), &left);
    assert_eq!(pb.moving_at(), Some((walk, 0)));
    pb.choose(w, &left);
    assert_eq!(pb.moving, None);
}

/// A spot animation that fades its faces fades on both sides: the cache
/// side as the original client fades it, the converted side through its
/// clip's fades, face for face.
#[test]
fn a_fading_spot_animation_fades_on_both_sides() {
    let Some(dir) = std::env::var_os("RS2_CACHE").map(PathBuf::from) else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let cache = open_cache(&dir).unwrap();
    let Source::Cache { records, .. } = &cache else {
        unreachable!()
    };
    // The first spot animation whose sequence fades its model.
    let (id, seq) = records
        .ids(RecordType::SpotAnim)
        .unwrap()
        .into_iter()
        .find_map(|id| {
            let (_, r) = load(&cache, &Sel::Record(RecordType::SpotAnim, id)).ok()?;
            let Renderable::Creature { creature, .. } = r? else {
                return None;
            };
            let s = creature.sequences.first()?;
            (0..s.def.frames.len())
                .any(|i| creature.faded(0, i).is_some())
                .then_some((id, s.def.id))
        })
        .expect("a spot animation that fades");
    let (_, left) = load(&cache, &Sel::Record(RecordType::SpotAnim, id)).unwrap();
    let left = left.unwrap();

    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("d6_playback_fades");
    let _ = std::fs::remove_dir_all(&out);
    let png = |_: &std::path::Path, _: u32, _: u32, _: &[u8]| Ok(());
    let conv = rs2convert::run::spotanims(records, 0, Some(id)).unwrap();
    let written = rs2convert::run::write(&conv, &out, png).unwrap();
    let (_, _, models) = written.iter().find(|(k, ..)| *k == Kind::Models).unwrap();
    let pack = open_pack(models).unwrap();
    let (_, right) = load(
        &pack,
        &Sel::PackRecord(rs2convert::pipeline::ids::SPOTANIM_MODEL | id),
    )
    .unwrap();
    let right = right.unwrap();

    let (delays, _) = timing(&left, seq).unwrap();
    let mut faded = 0;
    for f in 0..delays.len() {
        let (Some(Posed::Cache(_, a)), Some(Posed::Pack(_, o))) =
            (posed(&left, seq, f), posed(&right, seq, f))
        else {
            panic!("frame {f} poses on both sides")
        };
        assert_eq!(
            a.is_some(),
            o.is_some(),
            "frame {f}: both sides fade or neither"
        );
        if let (Some(a), Some(o)) = (a, o) {
            faded += 1;
            let opacity: Vec<u8> = a.iter().map(|t| 255 - t).collect();
            assert_eq!(opacity, o, "frame {f}");
        }
    }
    assert!(faded > 0, "spot animation {id} fades");
}
