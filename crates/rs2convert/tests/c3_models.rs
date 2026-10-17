//! Phase C3 check: every model converts with matching counts, the pack of
//! all of them reads back with one record per model, and the Man model is
//! right side up (y up) after conversion.

mod common;

use rayon::prelude::*;
use rs2cache::Records;
use rs2cache::types::record::{Record, RecordType};
use rs2convert::model::convert;
use rs2convert::palette::{Palette, corpus};
use rs2pack::records::model::PackModel;
use rs2pack::{Kind, PackReader, PackWriter};

#[test]
fn every_model_converts() {
    let Some(cache) = common::cache() else { return };
    let records = Records::new(&cache);
    let corpus = corpus(&records).unwrap();
    let mut palette = Palette::build(&corpus);
    palette.prime(&corpus);
    let ids = records.ids(RecordType::Model).unwrap();
    let converted: Vec<(u32, Vec<u8>)> = ids
        .par_iter()
        .map(|&id| {
            let Record::Model(m) = records.decode(RecordType::Model, id).unwrap() else {
                unreachable!()
            };
            let p = convert(&m, &palette).unwrap_or_else(|e| panic!("model {id}: {e}"));
            assert_eq!(p.vertices.len(), m.vertices.len());
            assert_eq!(p.faces.len(), m.faces.len());
            assert_eq!(p.labels.is_some(), m.vertex_labels.is_some());
            (id, p.encode().unwrap())
        })
        .collect();
    let mut w = PackWriter::new(Kind::Models, 0);
    for (id, r) in &converted {
        w.insert(*id, r.clone());
    }
    let (_, bytes) = w.bake();
    let r = PackReader::from_bytes(&bytes).unwrap();
    assert_eq!(r.manifest.record_count as usize, ids.len());
    let textured = converted
        .iter()
        .filter(|(_, b)| PackModel::decode(b).unwrap().uvs.is_some())
        .count();
    eprintln!(
        "{} models converted, {textured} with textured faces",
        ids.len()
    );

    // Man's head model (215) sits near the top of the figure: y up.
    let man = PackModel::decode(&r.get(215).unwrap().unwrap()).unwrap();
    let min_y = man.vertices.iter().map(|v| v[1]).min().unwrap();
    assert!(
        min_y > 150,
        "head should be high above the feet; lowest y {min_y}"
    );
}
