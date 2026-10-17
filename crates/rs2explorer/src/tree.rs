//! The data behind the left-hand tree, kept apart from egui so it can be
//! tested.

use crate::sources::Source;

/// One index of a cache with its file count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexRow {
    pub index: u8,
    pub files: usize,
    pub present: usize,
}

/// The indices of a cache source.
pub fn index_rows(source: &Source) -> Vec<IndexRow> {
    let Source::Cache { records, .. } = source else {
        return Vec::new();
    };
    let cache = records.cache();
    (0..cache.index_count())
        .filter_map(|i| {
            let e = cache.entries(i).ok()?;
            Some(IndexRow {
                index: i,
                files: e.len(),
                present: e.iter().filter(|x| !x.is_absent()).count(),
            })
        })
        .collect()
}
