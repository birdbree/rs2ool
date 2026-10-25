//! Fades against the cache in `RS2_CACHE` (skipped without one).
//!
//! Converted clips carry their sequences' alpha transforms as fades,
//! exactly: every spot animation's clip, alone, and the player's actions
//! that fade, mixed with the walk and run.

use rs2cache::Records;
use rs2cache::types::record::{Record, RecordType};
use rs2convert::palette::Palette;
use rs2convert::{pipeline, preview, run};

mod common;

#[test]
fn spot_animation_fades_are_exact() {
    let Some(cache) = common::cache() else { return };
    let records = Records::new(&cache);
    let out = run::spotanims(&records, 0, None).unwrap();
    let (mut clips, mut fading, mut keys) = (0, 0, 0);
    for (key, c) in &out.creatures {
        for (_, clip, report) in &c.clips {
            clips += 1;
            assert_eq!(report.fade_mismatches, 0, "{key} seq {}", report.sequence);
            if !clip.fades.is_empty() {
                fading += 1;
                keys += clip.fades.len();
                assert!(
                    c.model.face_groups.is_some(),
                    "{key}: a clip fades a model with no face groups"
                );
            }
        }
    }
    println!("{clips} spot animation clips, {fading} fade, {keys} fades");
    assert!(fading > 100, "{fading} clips fade");
}

#[test]
fn fading_actions_mix_with_exact_fades() {
    let Some(cache) = common::cache() else { return };
    let records = Records::new(&cache);
    let index = cache.frame_index().unwrap();
    let walk_base = {
        let c = preview::player_body(&records, &index, false, &[])
            .unwrap()
            .creature;
        let w = c.sequences.iter().find(|s| s.def.id == 819).unwrap();
        c.frame(w, 0).unwrap().0.base.clone()
    };
    let actions: Vec<u16> = records
        .ids(RecordType::Seq)
        .unwrap()
        .into_iter()
        .filter(|&s| matches!(records.decode(RecordType::Seq, s), Ok(Record::Seq(d)) if d.interleave_labels.is_some()))
        .map(|s| s as u16)
        .collect();
    let mut checked = 0;
    for a in actions {
        let c = preview::player_body(&records, &index, false, &[a])
            .unwrap()
            .creature;
        let s = c
            .sequences
            .iter()
            .position(|q| q.def.id == u32::from(a))
            .unwrap();
        let seq = &c.sequences[s];
        let fades = (0..seq.def.frames.len()).any(|i| c.faded(s, i).is_some());
        if !fades || c.frame(seq, 0).unwrap().0.base != walk_base {
            continue;
        }
        checked += 1;
        let out = pipeline::convert(c, &Palette::from_rgb(vec![[0, 0, 0]]), 0, true, None).unwrap();
        let (_, clip, report) = &out.clips[s];
        assert!(!clip.fades.is_empty(), "seq {a}");
        assert_eq!(report.fade_mismatches, 0, "seq {a}");
        assert!(!report.moving.is_empty(), "seq {a} is checked mixed");
        for m in &report.moving {
            assert_eq!(m.fade_mismatches, 0, "seq {a} over seq {}", m.sequence);
        }
    }
    println!("{checked} fading actions mixed");
    assert!(checked > 0, "some player actions fade");
}
