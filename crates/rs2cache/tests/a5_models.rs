//! Every model decodes with its sections filling the file exactly, and an
//! animated NPC's models have plausible bounds and vertex labels.

mod common;

use rs2cache::config::{self, npc::Npc};
use rs2cache::model::{self, Header, Layout};
use rs2cache::types::config::NpcDef;

#[test]
fn every_model_decodes() {
    let Some(cache) = common::cache() else { return };
    let mut failures = Vec::new();
    let mut slack = Vec::new();
    let mut decoded = 0;
    for id in 0..cache.file_count(1).unwrap() as u32 {
        let data = cache.read(1, id).unwrap();
        let h = Header::read(&data).unwrap();
        let end = Layout::compute(&h).end;
        if end + 18 != data.len() {
            slack.push((id, data.len() as isize - end as isize - 18));
        }
        match model::decode(&data) {
            Ok(m) => {
                assert_eq!(m.vertices.len(), usize::from(h.vertex_count));
                assert_eq!(m.faces.len(), usize::from(h.face_count));
                decoded += 1;
            }
            Err(e) => failures.push(format!("{id}: {e}")),
        }
    }
    eprintln!(
        "{decoded} models decoded; {} with section slack {:?}",
        slack.len(),
        &slack[..slack.len().min(10)]
    );
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures[..failures.len().min(20)].join("\n")
    );
    assert!(slack.is_empty(), "sections do not fill the file");
}

/// The first NPC with at least two models, stand and walk sequences, and a
/// one-tile footprint: a jointed figure about one tile across.
fn rigged_npc(cache: &rs2cache::Cache) -> NpcDef {
    config::decode_all::<Npc>(&cache.archive(config::ARCHIVE).unwrap())
        .unwrap()
        .into_iter()
        .find(|n| {
            n.models.len() >= 2 && n.stand_anim.is_some() && n.walk_anim.is_some() && n.size == 1
        })
        .expect("no NPC with two models, stand and walk sequences, and size 1")
}

#[test]
fn npc_models_have_plausible_bounds() {
    let Some(cache) = common::cache() else { return };
    let npc = rigged_npc(&cache);
    for &id in &npc.models {
        let m = cache.model(id.into()).unwrap();
        let (lo, hi) = m.bounds().unwrap();
        eprintln!(
            "npc {} model {id}: {} verts, {} faces, bounds {lo:?}..{hi:?}, labels {}",
            npc.id,
            m.vertices.len(),
            m.faces.len(),
            m.vertex_labels.is_some()
        );
        // A tile is 128 units; a one-tile figure stays well inside ±200.
        for a in [0, 2] {
            assert!(
                lo[a] >= -200 && hi[a] <= 200,
                "model {id} axis {a} out of ±200"
            );
        }
        // y is down-negative: a standing figure reaches up a few hundred
        // units at most.
        assert!(lo[1] > -300 && hi[1] < 300, "model {id} height");
        // An animated figure's parts carry vertex labels for its frames.
        assert!(
            m.vertex_labels.is_some(),
            "npc {} model {id} has no labels",
            npc.id
        );
    }
}
