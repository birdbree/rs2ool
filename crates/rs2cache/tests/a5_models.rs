//! Every model decodes with its sections filling the file exactly, and a
//! few individual models have plausible bounds.

mod common;

use rs2cache::model::{self, Header, Layout};

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

#[test]
fn known_models_have_plausible_bounds() {
    let Some(cache) = common::cache() else { return };
    let man = rs2cache::config::decode_one::<rs2cache::config::npc::Npc>(
        &cache.archive(rs2cache::config::ARCHIVE).unwrap(),
        1,
    )
    .unwrap();
    for id in std::iter::once(0).chain(man.models.iter().map(|&m| u32::from(m))) {
        let m = cache.model(id).unwrap();
        let (lo, hi) = m.bounds().unwrap();
        eprintln!(
            "model {id}: {} verts, {} faces, bounds {lo:?}..{hi:?}, labels {}",
            m.vertices.len(),
            m.faces.len(),
            m.vertex_labels.is_some()
        );
        for a in [0, 2] {
            assert!(
                lo[a] >= -200 && hi[a] <= 200,
                "model {id} axis {a} out of ±200"
            );
        }
        // y is down-negative: standing models reach up to about -230.
        assert!(lo[1] > -300 && hi[1] < 300, "model {id} height");
    }
    for &id in &man.models {
        assert!(
            cache.model(id.into()).unwrap().vertex_labels.is_some(),
            "Man model {id} has no labels"
        );
    }
}
