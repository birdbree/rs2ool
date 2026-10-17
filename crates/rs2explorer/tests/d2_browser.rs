//! Phase D2 check: selecting `npc 1` decodes off the UI thread, and the
//! inspector then shows "Man", its models and its sequences; the npc list
//! loads with names and filters by name.

use std::path::PathBuf;

use bevy::prelude::*;
use rs2explorer::inspector;
use rs2explorer::selection::{self, Loaded, Sel, Selected};
use rs2explorer::sources::{self, Kind, Sources};
use rs2explorer::tree::{self, ListKey, Lists};
use rs2intermediate::record::RecordType;

fn pump(app: &mut App, done: impl Fn(&World) -> bool) {
    for _ in 0..200_000 {
        if done(app.world()) {
            return;
        }
        app.update();
        std::thread::yield_now();
    }
    panic!("timed out");
}

#[test]
fn selecting_npc_1_shows_man() {
    let Some(dir) = std::env::var_os("RS2_CACHE").map(PathBuf::from) else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        sources::plugin,
        selection::plugin,
        tree::plugin,
    ));
    app.world_mut()
        .resource_mut::<Sources>()
        .open_path(Kind::Cache, dir);
    pump(&mut app, |w| !w.resource::<Sources>().list.is_empty());

    // The npc list loads with names.
    {
        let world = app.world_mut();
        let source = world.resource::<Sources>();
        let s = rs2explorer::sources::detach(&source.list[0]);
        world
            .resource_mut::<Lists>()
            .request(ListKey::Type(0, RecordType::Npc), &s);
    }
    pump(&mut app, |w| {
        w.resource::<Lists>()
            .rows
            .contains_key(&ListKey::Type(0, RecordType::Npc))
    });
    let lists = app.world().resource::<Lists>();
    let rows = lists.rows[&ListKey::Type(0, RecordType::Npc)]
        .as_ref()
        .unwrap();
    assert!(
        rows.iter()
            .filter(|r| r.matches("man"))
            .any(|r| r.id == 1 && r.name == "Man")
    );

    *app.world_mut().resource_mut::<Selected>() = Selected {
        source: Some(0),
        sel: Some(Sel::Record(RecordType::Npc, 1)),
    };
    pump(&mut app, |w| {
        w.resource::<Loaded>().data.is_some() || w.resource::<Loaded>().error.is_some()
    });
    let loaded = app.world().resource::<Loaded>();
    assert!(loaded.error.is_none(), "{:?}", loaded.error);
    let (meta, fields) = inspector::sections(loaded.data.as_ref().unwrap());
    let get = |k: &str| {
        fields
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.clone())
    };
    assert_eq!(get("name").as_deref(), Some("Man"));
    assert!(get("models").is_some_and(|m| m.contains("215")));
    assert!(get("stand_anim").is_some() && get("walk_anim").is_some());
    assert!(meta.iter().any(|(k, v)| k == "record_type" && v == "npc"));
}

#[test]
fn definitions_draw_their_models() {
    let Some(dir) = std::env::var_os("RS2_CACHE").map(PathBuf::from) else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, sources::plugin, selection::plugin, tree::plugin));
    app.world_mut().resource_mut::<Sources>().open_path(Kind::Cache, dir);
    pump(&mut app, |w| !w.resource::<Sources>().list.is_empty());
    for (t, id, min_faces) in [(RecordType::Npc, 1, 400), (RecordType::Loc, 1276, 50), (RecordType::Obj, 1038, 10)] {
        *app.world_mut().resource_mut::<Selected>() = Selected { source: Some(0), sel: Some(Sel::Record(t, id)) };
        pump(&mut app, |w| {
            let l = w.resource::<Loaded>();
            l.for_sel.as_ref().is_some_and(|(_, s)| *s == Sel::Record(t, id)) && !l.is_loading()
        });
        let loaded = app.world().resource::<Loaded>();
        let Some(data) = &loaded.data else { panic!("{t} {id}: {:?}", loaded.error) };
        let rs2explorer::selection::LoadedData::Record { preview, .. } = &**data else { panic!() };
        let faces = preview.as_ref().map_or(0, |m| m.faces.len());
        assert!(faces >= min_faces, "{t} {id} draws {faces} faces");
    }
}
