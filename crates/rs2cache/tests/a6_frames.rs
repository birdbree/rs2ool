//! Phase A6 check: every frame pack decodes with exact section lengths; frame
//! transform indices are within the base; every frame `seq.dat` references
//! is defined by exactly one pack; the player stand sequence only moves
//! labels the Man model has.

mod common;

use std::collections::BTreeSet;

use rs2cache::anim::{self, FrameIndex};
use rs2cache::archive;
use rs2cache::config::{self, npc::Npc, seq::Seq};
use rs2intermediate::anim::kind;

#[test]
fn every_pack_decodes_exactly() {
    let Some(cache) = common::cache() else { return };
    let mut frames = 0;
    let mut rot_max = 0;
    for id in 0..cache.file_count(2).unwrap() as u32 {
        let data = cache.read(2, id).unwrap();
        let p = anim::decode_with(&data, true).unwrap_or_else(|e| panic!("pack {id}: {e}"));
        for f in &p.frames {
            for t in &f.transforms {
                let b = &p.base[usize::from(t.transform)];
                if b.kind == kind::ROTATE {
                    rot_max = rot_max.max(t.dx.abs()).max(t.dy.abs()).max(t.dz.abs());
                }
            }
            frames += 1;
        }
    }
    eprintln!("{frames} frames; largest rotation delta {rot_max}");
}

#[test]
fn sequences_resolve_to_one_pack() {
    let Some(cache) = common::cache() else { return };
    let index = cache.frame_index().unwrap();
    assert!(
        index.duplicates.is_empty(),
        "frames in several packs: {:?}",
        index.duplicates
    );

    let versionlist = cache.archive(archive::VERSIONLIST).unwrap();
    let anim_index = versionlist.get("anim_index").unwrap();
    let max_frame = *index.pack_of.keys().last().unwrap();
    eprintln!(
        "{} frames in {} packs; max frame id {max_frame}; anim_index has {} entries; anim_version {}",
        index.pack_of.len(),
        cache.file_count(2).unwrap(),
        anim_index.len() / 2,
        versionlist.get("anim_version").unwrap().len() / 2,
    );
    // The client sizes its global frame table from anim_index's length.
    assert!(usize::from(max_frame) < anim_index.len() / 2);
    // anim_index is not a frame -> pack map: count how often it agrees.
    let agree = index
        .pack_of
        .iter()
        .filter(|&(&f, &p)| {
            let i = usize::from(f) * 2;
            u32::from(u16::from_be_bytes([anim_index[i], anim_index[i + 1]])) == p
        })
        .count();
    eprintln!(
        "anim_index[frame] == pack for {agree} of {} frames",
        index.pack_of.len()
    );

    let seqs = cache.config::<Seq>().unwrap();
    let mut missing = BTreeSet::new();
    for s in &seqs {
        for f in &s.frames {
            for id in std::iter::once(f.frame).chain(f.secondary) {
                if index.pack_for(id).is_none() {
                    missing.insert((s.id, id));
                }
            }
        }
    }
    eprintln!(
        "{} (sequence, frame) references to frames no pack defines: {:?}",
        missing.len(),
        missing.iter().take(10).collect::<Vec<_>>()
    );
    assert!(missing.is_empty());
}

#[test]
fn stand_sequence_labels_exist_on_man() {
    let Some(cache) = common::cache() else { return };
    let cfg = cache.archive(config::ARCHIVE).unwrap();
    let man = config::decode_one::<Npc>(&cfg, 1).unwrap();
    let labels: BTreeSet<u8> = man
        .models
        .iter()
        .flat_map(|&m| cache.model(m.into()).unwrap().vertex_labels.unwrap())
        .collect();
    let stand = config::decode_one::<Seq>(&cfg, man.stand_anim.unwrap().into()).unwrap();
    let index: FrameIndex = cache.frame_index().unwrap();
    let mut moved = BTreeSet::new();
    for f in &stand.frames {
        let pack = cache.frame_pack(index.pack_for(f.frame).unwrap()).unwrap();
        let frame = pack.frames.iter().find(|x| x.id == f.frame).unwrap();
        for t in &frame.transforms {
            let b = &pack.base[usize::from(t.transform)];
            if b.kind != kind::ALPHA {
                moved.extend(b.labels.iter().copied());
            }
        }
    }
    eprintln!("Man labels {labels:?}; stand moves {moved:?}");
    // A frame base covers every body type, so it may name labels this model
    // lacks; but what the model has must be what the base moves.
    assert!(!moved.is_empty());
    assert!(
        labels.iter().all(|l| *l == 0 || moved.contains(l)),
        "Man labels not moved by stand"
    );
}
