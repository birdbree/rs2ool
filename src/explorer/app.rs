//! The explorer's window and the startup options the command line sets.
//!
//! [`run`] builds the Bevy app from every module's plugin and blocks until
//! the window closes.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy_egui::EguiPlugin;

use super::selection::{Sel, Selected};
use super::sources::{Kind, Source, Sources};
use super::viewport3d::Compare;
use super::{
    actions, bones, classic, jobs, kit, map_view, playback, selection, sources, tree, ui,
    viewport2d, viewport3d,
};

/// What to open at startup: a cache and any packs, optionally a record in
/// the cache to select, with `compare` showing its other version (original
/// or converted) beside it, and `classic` starting in the classic renderer.
#[derive(Resource, Debug, Default)]
pub struct Startup {
    /// A cache directory to open (`--cache` or `RS2_CACHE`).
    pub cache: Option<PathBuf>,
    /// Pack files to open.
    pub packs: Vec<PathBuf>,
    /// What to select in the cache once every startup source has opened.
    /// Ignored without a cache.
    pub select: Option<Sel>,
    /// Turn compare mode on with that selection.
    pub compare: bool,
    /// Start in the classic renderer.
    pub classic: bool,
}

/// Parses a `--select` value: `type:id` (e.g. `npc:1`), or `kit` for the
/// player kit.
pub fn parse_select(v: &str) -> Result<Sel, String> {
    if v == "kit" {
        return Ok(Sel::PlayerKit);
    }
    let (t, id) = v.split_once(':').ok_or("expected TYPE:ID or kit")?;
    Ok(Sel::Record(
        t.parse()
            .map_err(|e: rs2cache::types::record::UnknownRecordType| e.to_string())?,
        id.parse().map_err(|e| format!("{id}: {e}"))?,
    ))
}

/// Starts opening the startup sources, and picks the renderer.
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
    // Wait until every startup source has either opened or failed.
    if *done || sources.pending() > 0 || sources.list.len() + sources.errors.len() < expected {
        return;
    }
    *done = true;
    let cache = sources
        .list
        .iter()
        .position(|s| matches!(s, Source::Cache { .. }));
    let (Some(sel), Some(si)) = (st.select.clone(), cache) else {
        return;
    };
    compare.enabled = st.compare;
    *selected = Selected {
        source: Some(si),
        sel: Some(sel),
    };
}

/// Open the explorer window; returns when it closes.
pub fn run(startup: Startup) {
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgb(0.12, 0.12, 0.14)))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "rs2ool explorer".into(),
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
            bones::plugin,
            kit::plugin,
            actions::plugin,
            classic::plugin,
            map_view::plugin,
            jobs::plugin,
        ))
        .insert_resource(startup)
        .add_systems(bevy::app::Startup, open_startup)
        .add_systems(Update, select_startup);
    #[cfg(feature = "caption")]
    app.add_plugins(crate::explorer::captions::plugin);
    app.run();
}
