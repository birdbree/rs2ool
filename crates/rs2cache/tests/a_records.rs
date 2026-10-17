//! Every record type lists ids, decodes its first and last id, and produces
//! a metadata sidecar that survives a JSON round trip.

mod common;

use rs2cache::types::record::{Meta, RecordType, TypeSummary};
use rs2cache::{decode_any, ids, meta};

#[test]
fn every_type_decodes_and_summarises() {
    let Some(cache) = common::cache() else { return };
    for &t in RecordType::ALL {
        let list = ids(&cache, t).unwrap_or_else(|e| panic!("{t}: {e}"));
        assert!(!list.is_empty(), "{t} has no ids");
        for &id in [list[0], *list.last().unwrap()].iter() {
            let r = decode_any(&cache, t, id).unwrap_or_else(|e| panic!("{t} {id}: {e}"));
            assert_eq!(r.record_type(), t);
        }
        let m = meta(&cache, t, list[0]).unwrap_or_else(|e| panic!("{t} meta: {e}"));
        assert!(!m.files.is_empty());
        let json = serde_json::to_string(&m).unwrap();
        let back: Meta = serde_json::from_str(&json)
            .unwrap_or_else(|e| panic!("{t} meta round trip: {e}\n{json:.300}"));
        assert_eq!(back, m);
        eprintln!(
            "{t}: {} ids, first {} ({} file(s), {} bytes of JSON)",
            list.len(),
            list[0],
            m.files.len(),
            json.len()
        );
    }
}

#[test]
fn model_sidecar_counts_match_the_model() {
    let Some(cache) = common::cache() else { return };
    let id = ids(&cache, RecordType::Model).unwrap()[0];
    let m = meta(&cache, RecordType::Model, id).unwrap();
    let model = cache.model(id).unwrap();
    let TypeSummary::Model {
        vertex_count,
        face_count,
        ..
    } = m.summary
    else {
        panic!()
    };
    assert_eq!(
        (vertex_count, face_count),
        (model.vertices.len(), model.faces.len())
    );
    assert_eq!(m.files[0].compression, "gzip");
}
