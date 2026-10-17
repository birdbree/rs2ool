//! Building a rig record from an inferred rig: rest positions that fit an
//! `i16` encode, and ones that don't are an error rather than a silent
//! wrap. Synthetic, so it runs without a cache.

use std::collections::BTreeMap;

use rs2convert::rig::{InferredBone, InferredRig, LabelSet};

fn rig(offset: i32) -> InferredRig {
    InferredRig {
        bones: vec![
            InferredBone {
                name: "root".into(),
                labels: LabelSet::new(),
                parent: None,
                pivot: [0, 0, 0],
            },
            InferredBone {
                name: "bone_1".into(),
                labels: LabelSet::from([1]),
                parent: Some(0),
                pivot: [offset, -10, 20],
            },
        ],
        label_bone: BTreeMap::from([(1, 1)]),
    }
}

#[test]
fn rest_positions_in_range_encode() {
    let r = rig(1000).record().unwrap();
    // Pack space negates y and z.
    assert_eq!(r.bones[1].rest_pos, [1000, 10, -20]);
}

#[test]
fn rest_positions_past_i16_are_an_error() {
    let err = rig(40_000).record().unwrap_err().to_string();
    assert!(err.contains("bone 1") && err.contains("40000"), "{err}");
}
