//! Phase C1 check: a pack holding one synthetic model reads back with the
//! record byte-identical.

use contentpack::records::model::{AttachPoint, PackModel};
use contentpack::{Kind, PackReader, PackWriter};

#[test]
fn one_synthetic_model_round_trips() {
    let model = PackModel {
        vertices: vec![[0, 0, 0], [128, 0, 0], [0, 240, 0], [0, 0, 128]],
        faces: vec![[0, 1, 2], [0, 2, 3], [0, 3, 1], [1, 3, 2]],
        material_slots: vec![0; 4],
        palette_indices: vec![1, 2, 3, 4],
        uvs: None,
        alphas: Some(vec![255, 255, 128, 0]),
        priorities: None,
        labels: Some(vec![0, 1, 1, 2]),
        attach_points: vec![AttachPoint {
            name_hash: 7,
            position: [0, 240, 0],
            yaw: 0,
        }],
    };
    let record = model.encode().unwrap();
    let mut w = PackWriter::new(Kind::Models, 1);
    w.insert(215, record.clone());
    let (name, bytes) = w.bake();
    let r = PackReader::from_bytes(&bytes).unwrap();
    assert_eq!(r.hash, name);
    assert_eq!(r.manifest.record_count, 1);
    let back = r.get(215).unwrap().unwrap();
    assert_eq!(back, record);
    assert_eq!(PackModel::decode(&back).unwrap(), model);
}
