//! Phase D6 check: Man's walk sequence plays at the client's speed (its
//! frame delays at 20 ms per cycle), and the converted Man plays the same
//! clip in step: at every frame the skinned pack vertices sit where the
//! client's deformation puts the cache vertices.

use std::path::PathBuf;

use rs2cache::types::record::{Record, RecordType};
use rs2explorer::playback::{CYCLE_MS, Clock, Posed, posed, sequences, timing};
use rs2explorer::renderable::Renderable;
use rs2explorer::selection::{Sel, load};
use rs2explorer::sources::{Source, open_cache, open_pack};
use rs2pack::Kind;

#[test]
fn man_walks_at_client_speed_and_in_step_with_its_conversion() {
    let Some(dir) = std::env::var_os("RS2_CACHE").map(PathBuf::from) else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let cache = open_cache(&dir).unwrap();
    let Source::Cache { records, .. } = &cache else {
        unreachable!()
    };
    let Record::Npc(man) = records.decode(RecordType::Npc, 1).unwrap() else {
        unreachable!()
    };
    let walk = u32::from(man.walk_anim.unwrap());

    let (_, left) = load(&cache, &Sel::Record(RecordType::Npc, 1)).unwrap();
    let left = left.unwrap();
    assert!(matches!(left, Renderable::Creature { .. }), "Man plays");
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

    // The converted Man, written as `rs2conv npc --id 1 --with-anims` would.
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("d6_playback");
    let _ = std::fs::remove_dir_all(&out);
    let png = |_: &std::path::Path, _: u32, _: u32, _: &[u8]| Ok(());
    let conv = rs2convert::run::npc(records, 0, 1, true, &[], None).unwrap();
    let written = rs2convert::run::write(&conv, &out, png).unwrap();
    let (_, _, models) = written.iter().find(|(k, ..)| *k == Kind::Models).unwrap();
    let pack = open_pack(models).unwrap();
    let id = rs2convert::pipeline::ids::CREATURE_MODEL | 1;
    let (_, right) = load(&pack, &Sel::PackRecord(id)).unwrap();
    let right = right.unwrap();
    assert!(
        matches!(right, Renderable::PackCreature { .. }),
        "the converted Man finds its rig and clips beside its model"
    );
    assert!(sequences(&right).contains(&walk));
    let (pack_delays, _) = timing(&right, walk).unwrap();
    assert_eq!(pack_delays, delays, "both sides keep the same time");

    // In step: frame by frame, the two poses agree.
    let mut worst: f32 = 0.0;
    for f in 0..delays.len() {
        let (Some(Posed::Cache(c)), Some(Posed::Pack(p))) =
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
}
