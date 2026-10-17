//! Phase D1 check: opening a cache runs on the task pool without blocking
//! the app's update loop, and the tree then lists the five indices with
//! their file counts.

use std::path::PathBuf;

use bevy::prelude::*;
use rs2explorer::sources::{self, Kind, Sources};
use rs2explorer::tree;

#[test]
fn opens_a_cache_off_the_ui_thread() {
    let Some(dir) = std::env::var_os("RS2_CACHE").map(PathBuf::from) else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, sources::plugin));
    app.world_mut()
        .resource_mut::<Sources>()
        .open_path(Kind::Cache, dir.clone());
    // The open is pending; updates keep running while it completes.
    let mut frames = 0;
    while app.world().resource::<Sources>().pending() > 0 {
        app.update();
        frames += 1;
        assert!(frames < 100_000, "open never finished");
        std::thread::yield_now();
    }
    let sources = app.world().resource::<Sources>();
    assert!(sources.errors.is_empty(), "{:?}", sources.errors);
    assert_eq!(sources.list.len(), 1);
    let rows = tree::index_rows(&sources.list[0]);
    assert_eq!(rows.len(), 5);
    let cache = rs2cache::Cache::open(&dir).unwrap();
    for r in &rows {
        assert_eq!(r.files, cache.file_count(r.index).unwrap());
    }
    eprintln!("opened after {frames} updates: {rows:?}");
}
