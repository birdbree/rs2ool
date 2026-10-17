//! Selecting an NPC definition decodes off the UI thread and fills the
//! inspector with its name, models and sequences; the npc list loads with
//! names and filters by name.

#![cfg(feature = "explorer")]

use std::path::PathBuf;

use bevy::prelude::*;
use rs2cache::types::record::{Record, RecordType};
use rs2cache::{Cache, Records};
use rs2ool::explorer::inspector;
use rs2ool::explorer::selection::{self, Loaded, Sel, Selected};
use rs2ool::explorer::sources::{self, Kind, Sources};
use rs2ool::explorer::tree::{self, ListKey, Lists};

mod common;

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
fn selecting_an_npc_shows_its_definition() {
    let Some(dir) = std::env::var_os("RS2_CACHE").map(PathBuf::from) else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let cache = Cache::open(&dir).unwrap();
    let records = Records::new(&cache);
    let npc = common::walking_npc(&records);
    let Ok(Record::Npc(def)) = records.decode(RecordType::Npc, npc) else {
        unreachable!()
    };
    let name = def.name.clone().unwrap();
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
        let s = rs2ool::explorer::sources::detach(&source.list[0]);
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
    // A lower-case search for the name finds it; one for its id does too.
    let query = name.to_lowercase();
    assert!(
        rows.iter()
            .filter(|r| r.matches(&query))
            .any(|r| r.id == npc && r.name == name),
        "searching {query:?} finds npc {npc}"
    );
    assert!(
        rows.iter()
            .filter(|r| r.matches(&npc.to_string()))
            .any(|r| r.id == npc)
    );
    assert!(
        rows.iter().any(|r| !r.matches(&query)),
        "the search filters something out"
    );

    *app.world_mut().resource_mut::<Selected>() = Selected {
        source: Some(0),
        sel: Some(Sel::Record(RecordType::Npc, npc)),
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
    assert_eq!(get("name"), Some(name));
    let first_model = def.models[0].to_string();
    assert!(get("models").is_some_and(|m| m.contains(&first_model)));
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
    app.add_plugins((
        MinimalPlugins,
        sources::plugin,
        selection::plugin,
        tree::plugin,
    ));
    app.world_mut()
        .resource_mut::<Sources>()
        .open_path(Kind::Cache, dir.clone());
    pump(&mut app, |w| !w.resource::<Sources>().list.is_empty());

    let cache = Cache::open(&dir).unwrap();
    let records = Records::new(&cache);
    let faces_of = |m: u16| cache.model(m.into()).unwrap().faces.len();
    // An NPC draws all its body models merged.
    let npc = common::walking_npc(&records);
    let Ok(Record::Npc(def)) = records.decode(RecordType::Npc, npc) else {
        unreachable!()
    };
    let npc_faces = def.models.iter().map(|&m| faces_of(m)).sum();
    // The first named location bigger than one tile with a single model
    // drawn as a plain object (type 10, or untyped).
    let (loc, loc_faces) = records
        .ids(RecordType::Loc)
        .unwrap()
        .into_iter()
        .find_map(|id| match records.decode(RecordType::Loc, id) {
            Ok(Record::Loc(d))
                if d.name.as_deref().is_some_and(|n| !n.is_empty())
                    && (d.size_x, d.size_y) != (1, 1)
                    && d.models.len() == 1
                    && d.models[0].kind.is_none_or(|k| k == 10) =>
            {
                Some((id, faces_of(d.models[0].model)))
            }
            _ => None,
        })
        .expect("a named multi-tile location with one model");
    // The first named item worn by the male body; it draws its inventory
    // model.
    let (obj, obj_faces) = records
        .ids(RecordType::Obj)
        .unwrap()
        .into_iter()
        .find_map(|id| match records.decode(RecordType::Obj, id) {
            Ok(Record::Obj(d))
                if d.name.as_deref().is_some_and(|n| !n.is_empty()) && d.male_equip1.is_some() =>
            {
                Some((id, faces_of(d.inventory_model)))
            }
            _ => None,
        })
        .expect("a named item with an equipment model");

    for (t, id, want) in [
        (RecordType::Npc, npc, npc_faces),
        (RecordType::Loc, loc, loc_faces),
        (RecordType::Obj, obj, obj_faces),
    ] {
        *app.world_mut().resource_mut::<Selected>() = Selected {
            source: Some(0),
            sel: Some(Sel::Record(t, id)),
        };
        pump(&mut app, |w| {
            let l = w.resource::<Loaded>();
            l.for_sel
                .as_ref()
                .is_some_and(|(_, s)| *s == Sel::Record(t, id))
                && !l.is_loading()
        });
        let loaded = app.world().resource::<Loaded>();
        assert!(loaded.data.is_some(), "{t} {id}: {:?}", loaded.error);
        let model = loaded.renderable.as_ref().and_then(|r| r.model());
        let faces = model.map_or(0, |m| m.faces.len());
        assert!(faces > 0, "{t} {id} draws nothing");
        assert_eq!(faces, want, "{t} {id} draws its models' faces");
    }
}
