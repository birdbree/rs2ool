//! Shared helpers for tests that need the real cache.

use rs2cache::Cache;

/// Open the cache named by `RS2_CACHE`, or `None` (with a note on stderr) so
/// the calling test can skip on machines without one.
pub fn cache() -> Option<Cache> {
    let Some(dir) = std::env::var_os("RS2_CACHE") else {
        eprintln!("skipping: RS2_CACHE is not set");
        return None;
    };
    Some(Cache::open(&dir).unwrap_or_else(|e| panic!("RS2_CACHE={}: {e}", dir.display())))
}
