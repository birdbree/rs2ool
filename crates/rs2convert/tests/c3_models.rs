//! Every model converts with matching counts, a pack of all of them reads
//! back with one record per model, and a converted model comes out right
//! side up (y up).

mod common;

use contentpack::records::model::PackModel;
use contentpack::{Kind, PackReader, PackWriter};
use rayon::prelude::*;
use rs2cache::Records;
use rs2cache::types::record::{Record, RecordType};
use rs2convert::model::convert;
use rs2convert::palette::{Palette, corpus};

/// A model's id, its pack record, and its cache-space y range.
type Converted = (u32, Vec<u8>, Option<(i32, i32)>);

#[test]
fn every_model_converts() {
    let Some(cache) = common::cache() else { return };
    let records = Records::new(&cache);
    let corpus = corpus(&records).unwrap();
    let mut palette = Palette::build(&corpus);
    palette.prime(&corpus);
    let ids = records.ids(RecordType::Model).unwrap();
    let converted: Vec<Converted> = ids
        .par_iter()
        .map(|&id| {
            let Record::Model(m) = records.decode(RecordType::Model, id).unwrap() else {
                unreachable!()
            };
            let p = convert(&m, &palette).unwrap_or_else(|e| panic!("model {id}: {e}"));
            assert_eq!(p.vertices.len(), m.vertices.len());
            assert_eq!(p.faces.len(), m.faces.len());
            assert_eq!(p.labels.is_some(), m.vertex_labels.is_some());
            let ys = m.vertices.iter().map(|v| v[1]);
            let range = ys.clone().min().zip(ys.max());
            (id, p.encode().unwrap(), range)
        })
        .collect();
    let mut w = PackWriter::new(Kind::Models, 0);
    for (id, r, _) in &converted {
        w.insert(*id, r.clone());
    }
    let (_, bytes) = w.bake();
    let r = PackReader::from_bytes(&bytes).unwrap();
    assert_eq!(r.manifest.record_count as usize, ids.len());
    let textured = converted
        .iter()
        .filter(|(_, b, _)| PackModel::decode(b).unwrap().uvs.is_some())
        .count();
    eprintln!(
        "{} models converted, {textured} with textured faces",
        ids.len()
    );

    // The first model that sits wholly above its origin, higher up than it
    // is tall (cache y points down, so every y is negative and the gap
    // below its lowest point beats its height), the way a head part sits
    // on a figure. After conversion it must be above the origin with y up,
    // top and bottom swapped.
    let (id, (cache_top, cache_bottom)) = converted
        .iter()
        .find_map(|&(id, _, range)| {
            range
                .filter(|&(lo, hi)| lo < hi && -hi > hi - lo)
                .map(|range| (id, range))
        })
        .expect("no model sits above its origin by more than its height");
    let model = PackModel::decode(&r.get(id).unwrap().unwrap()).unwrap();
    let min_y = model
        .vertices
        .iter()
        .map(|v| i32::from(v[1]))
        .min()
        .unwrap();
    let max_y = model
        .vertices
        .iter()
        .map(|v| i32::from(v[1]))
        .max()
        .unwrap();
    eprintln!("model {id}: cache y {cache_top}..={cache_bottom}, pack y {min_y}..={max_y}");
    assert!(
        min_y > 0,
        "model {id} should be above the origin with y up; lowest y {min_y}"
    );
    assert_eq!(
        (min_y, max_y),
        (-cache_bottom, -cache_top),
        "model {id}: pack y is cache y negated"
    );
}
