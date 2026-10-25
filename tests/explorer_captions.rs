//! The explorer's caption job against a stand-in model server on this
//! machine: it captions the selected NPC's candidates into the shared
//! store, reports progress, stops when asked, and a review saved from the
//! window survives the job. Needs `RS2_CACHE`.

#![cfg(all(feature = "explorer", feature = "caption"))]

mod common;
#[path = "common/server.rs"]
mod server;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use rs2ool::caption::store::{Review, Store, lock};
use rs2ool::explorer::captions::caption_job;
use rs2ool::explorer::jobs::{Jobs, ProgressHandle};
use rs2ool::explorer::sources::{Source, open_cache};
use rs2ool::settings::Caption;

const REPLY: &str = r#"{"message": {"content": "Shrinks away.\nvanish"}}"#;

#[test]
fn the_caption_job_fills_the_shared_store() {
    let Some(dir) = std::env::var_os("RS2_CACHE").map(std::path::PathBuf::from) else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let Source::Cache { records, .. } = open_cache(&dir).unwrap() else {
        unreachable!()
    };
    let npc = common::walking_npc(&records);
    let data =
        std::env::temp_dir().join(format!("rs2ool-explorer-captions-{}", std::process::id()));
    let store = Arc::new(Mutex::new(Store::open(&data, 9).unwrap()));
    let (url, count) = server::ollama(REPLY);
    let c = Caption {
        enabled: true,
        endpoint: url,
        model: "stand-in".into(),
        ..Caption::default()
    };

    // Run it as the explorer does: a job on the compute pool.
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(rs2ool::explorer::jobs::plugin);
    let stop = Arc::new(AtomicBool::new(false));
    {
        let (records, store, c, stop) = (records.clone(), store.clone(), c.clone(), stop.clone());
        app.world_mut()
            .resource_mut::<Jobs>()
            .spawn_job("caption", 0, move |p| {
                caption_job(&records, &c, &store, npc, 2, stop, &p)
            });
    }
    for _ in 0..10_000 {
        app.update();
        if app.world().resource::<Jobs>().running() == 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let jobs = app.world().resource::<Jobs>();
    let job = &jobs.list[0];
    let done = job.done.as_ref().expect("finished").as_ref().unwrap();
    assert_eq!(done.message, format!("npc {npc}: 2 of 2 captioned"));
    assert_eq!(job.fraction(), 1.0);
    assert_eq!(count.load(Ordering::SeqCst), 2);

    // Review one from the window, then caption more: the review stays.
    let first = lock(&store).entries().next().unwrap().seq;
    let right = Review {
        right: true,
        class: None,
    };
    {
        let mut st = lock(&store);
        assert!(st.review(npc, first, "stand-in", Some(right.clone())));
        st.save().unwrap();
    }
    // Outside a job, as a plain call.
    let progress = ProgressHandle::default();
    let msg = caption_job(&records, &c, &store, npc, 2, stop.clone(), &progress).unwrap();
    assert_eq!(msg.message, format!("npc {npc}: 2 of 2 captioned"));
    let back = Store::open(&data, 9).unwrap();
    assert_eq!(back.entries().count(), 4);
    assert_eq!(
        back.lookup(npc, first, "stand-in").unwrap().review,
        Some(right)
    );
    assert_eq!(
        back.lookup(npc, first, "stand-in")
            .unwrap()
            .reviewed_class(),
        Some("vanish")
    );

    // Stopped before it starts: nothing more is asked.
    stop.store(true, Ordering::Relaxed);
    let msg = caption_job(&records, &c, &store, npc, 2, stop, &progress).unwrap();
    assert_eq!(
        msg.message,
        format!("npc {npc}: stopped after 0 of 2 captions")
    );
    assert_eq!(count.load(Ordering::SeqCst), 4);
    let _ = std::fs::remove_dir_all(data);
}
