//! `rs2explorer`: browse, render, compare, export and convert a 317 cache.
//!
//! Every cache read and conversion runs on Bevy's `AsyncComputeTaskPool`
//! through [`jobs`] or [`sources`]; UI systems only poll finished tasks.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy_egui::EguiPlugin;

use rs2explorer::selection::{Sel, Selected};
use rs2explorer::sources::{Kind, Source, Sources};
use rs2explorer::viewport3d::Compare;
use rs2explorer::{
    classic, jobs, map_view, playback, selection, sources, tree, ui, viewport2d, viewport3d,
};
use rs2intermediate::record::RecordType;

/// `rs2explorer [cache-dir] [--pack file]... [--select type:id] [--compare]
/// [--classic]`: open a cache (default `RS2_CACHE`) and any packs at
/// startup, optionally select a record in the cache, and with `--compare`
/// show its other version (original or converted) beside it.
#[derive(Resource, Default)]
struct Startup {
    cache: Option<PathBuf>,
    packs: Vec<PathBuf>,
    select: Option<(RecordType, u32)>,
    compare: bool,
    classic: bool,
}

fn parse_args() -> Startup {
    let mut st = Startup {
        cache: std::env::var_os("RS2_CACHE").map(PathBuf::from),
        ..Default::default()
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--classic" {
            st.classic = true;
        } else if a == "--compare" {
            st.compare = true;
        } else if a == "--pack" {
            st.packs.extend(args.next().map(PathBuf::from));
        } else if a == "--select" {
            st.select = args.next().and_then(|v| {
                let (t, id) = v.split_once(':')?;
                Some((t.parse().ok()?, id.parse().ok()?))
            });
        } else {
            st.cache = Some(PathBuf::from(a));
        }
    }
    st
}

fn open_startup(
    st: Res<Startup>,
    mut sources: ResMut<Sources>,
    mut mode: ResMut<classic::RenderMode>,
) {
    if st.classic {
        *mode = classic::RenderMode::Classic;
    }
    if let Some(c) = &st.cache {
        sources.open_path(Kind::Cache, c.clone());
    }
    for p in &st.packs {
        sources.open_path(Kind::Pack, p.clone());
    }
}

/// Once every startup source is open (sources open in parallel, so the
/// cache is found by kind), select the record and turn on compare mode
/// for `--compare`.
fn select_startup(
    st: Res<Startup>,
    sources: Res<Sources>,
    mut selected: ResMut<Selected>,
    mut compare: ResMut<Compare>,
    mut done: Local<bool>,
) {
    let expected = usize::from(st.cache.is_some()) + st.packs.len();
    if *done || sources.pending() > 0 || sources.list.len() + sources.errors.len() < expected {
        return;
    }
    *done = true;
    let cache = sources
        .list
        .iter()
        .position(|s| matches!(s, Source::Cache { .. }));
    let (Some((t, id)), Some(si)) = (st.select, cache) else {
        return;
    };
    compare.enabled = st.compare;
    *selected = Selected {
        source: Some(si),
        sel: Some(Sel::Record(t, id)),
    };
}

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.12, 0.12, 0.14)))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "rs2explorer".into(),
                resolution: (1400, 900).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin::default())
        .add_plugins((
            ui::plugin,
            sources::plugin,
            selection::plugin,
            tree::plugin,
            viewport3d::plugin,
            viewport2d::plugin,
            playback::plugin,
            classic::plugin,
            map_view::plugin,
            jobs::plugin,
        ))
        .insert_resource(parse_args())
        .add_systems(bevy::app::Startup, open_startup)
        .add_systems(Update, select_startup)
        .run();
}
