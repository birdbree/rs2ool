//! Every file in every index reads without a sector error.

mod common;

use rs2cache::CacheError;

#[test]
fn every_file_reads() {
    let Some(cache) = common::cache() else { return };
    assert_eq!(cache.index_count(), 5);
    let mut failures = Vec::new();
    for index in 0..cache.index_count() {
        let count = cache.file_count(index).unwrap();
        let (mut present, mut absent, mut empty) = (0, 0, 0);
        for id in 0..count as u32 {
            match cache.read_raw(index, id) {
                Ok(data) if data.is_empty() => empty += 1,
                Ok(data) => {
                    assert_eq!(data.len() as u32, cache.entry(index, id).unwrap().size);
                    present += 1;
                }
                Err(CacheError::Absent { .. }) => absent += 1,
                Err(e) => failures.push(e.to_string()),
            }
        }
        eprintln!("index {index}: {count} ids, {present} present, {empty} empty, {absent} absent");
        assert!(present > 0, "index {index} has no readable files");
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
