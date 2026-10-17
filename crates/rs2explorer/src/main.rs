//! `rs2explorer`: browse, render, compare, export and convert a 317 cache.
//!
//! Every cache read and conversion runs on Bevy's `AsyncComputeTaskPool`
//! through [`jobs`] or [`sources`]; UI systems only poll finished tasks.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy_egui::EguiPlugin;

use rs2explorer::selection::{Sel, Selected};
use rs2explorer::sources::{Kind, Sources};
use rs2explorer::{classic, jobs, map_view, selection, sources, tree, ui, viewport3d};
use rs2intermediate::record::RecordType;

/// `rs2explorer [cache-dir] [--select type:id] [--classic]`: open a cache at startup
/// (default `RS2_CACHE`) and optionally select a record in it.
#[derive(Resource, Default)]
struct Startup {
    cache: Option<PathBuf>,
    select: Option<(RecordType, u32)>,
    classic: bool,
}

fn parse_args() -> Startup {
    let mut st = Startup {
        cache: std::env::var_os("RS2_CACHE").map(PathBuf::from),
        select: None,
        classic: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--classic" {
            st.classic = true;
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
}

fn select_startup(
    st: Res<Startup>,
    sources: Res<Sources>,
    mut selected: ResMut<Selected>,
    mut done: Local<bool>,
) {
    if *done || sources.list.is_empty() {
        return;
    }
    *done = true;
    if let Some((t, id)) = st.select {
        *selected = Selected {
            source: Some(0),
            sel: Some(Sel::Record(t, id)),
        };
    }
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
            classic::plugin,
            map_view::plugin,
            jobs::plugin,
        ))
        .insert_resource(parse_args())
        .add_systems(bevy::app::Startup, open_startup)
        .add_systems(Update, select_startup)
        .run();
}
