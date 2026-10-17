//! Phase A2 check: every non-empty index-1 and index-2 file is gzip and
//! inflates.

mod common;

use rs2cache::compress::{inflate, is_gzip};

#[test]
fn models_and_frame_packs_are_gzip() {
    let Some(cache) = common::cache() else { return };
    for index in [1, 2] {
        let mut not_gzip = Vec::new();
        let mut inflated = 0usize;
        for id in 0..cache.file_count(index).unwrap() as u32 {
            let Ok(raw) = cache.read_raw(index, id) else {
                continue;
            };
            if raw.is_empty() {
                continue;
            }
            if !is_gzip(&raw) {
                not_gzip.push(id);
                continue;
            }
            let data = inflate(&raw).unwrap_or_else(|e| panic!("{index}/{id}: {e}"));
            assert!(!data.is_empty(), "{index}/{id} inflates to nothing");
            inflated += 1;
        }
        eprintln!("index {index}: {inflated} inflated");
        assert!(not_gzip.is_empty(), "index {index} not gzip: {not_gzip:?}");
    }
}
