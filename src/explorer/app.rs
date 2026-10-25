//! The explorer's window and the startup options the command line sets.
//!
//! [`run`] builds the Bevy app from every module's plugin and blocks until
//! the window closes.

use std::path::PathBuf;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy_egui::EguiPlugin;

use super::jobs::Jobs;
use super::map_view::RegionScene;
use super::selection::{Loaded, Sel, Selected};
use super::sources::{Kind, Source, Sources};
use super::viewport3d::{Compare, OrbitCamera, WorldCamera};
use super::{
    actions, bones, classic, jobs, kit, map_view, playback, selection, sources, tree, ui,
    viewport2d, viewport3d,
};

/// What to open at startup: a cache and any packs, optionally a record in
/// the cache to select, with `compare` showing its other version (original
/// or converted) beside it, and `classic` starting in the classic renderer.
/// With `screenshot`, the window is saved there once everything has loaded
/// and closes.
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
    /// Where to point the camera once everything has loaded.
    pub camera: Option<CameraView>,
    /// Save a screenshot of the window here once everything has loaded
    /// (and the camera has moved), then close.
    pub screenshot: Option<PathBuf>,
}

/// A camera position for [`Startup::camera`]: yaw and pitch in degrees,
/// as [`OrbitCamera`] takes them in radians, the distance, and optionally
/// the point looked at in world units (the selection's framing otherwise).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraView {
    /// Turn around the vertical; 0 looks from the +z side.
    pub yaw: f32,
    /// Above the horizon.
    pub pitch: f32,
    /// From the target to the eye.
    pub distance: f32,
    /// What it looks at.
    pub target: Option<Vec3>,
}

/// Parses a `--camera` value: `YAW,PITCH,DISTANCE` or
/// `YAW,PITCH,DISTANCE,X,Y,Z`.
pub fn parse_camera(v: &str) -> Result<CameraView, String> {
    let n: Vec<f32> = v
        .split(',')
        .map(|p| p.trim().parse().map_err(|e| format!("{p}: {e}")))
        .collect::<Result<_, _>>()?;
    match n[..] {
        [yaw, pitch, distance] => Ok(CameraView {
            yaw,
            pitch,
            distance,
            target: None,
        }),
        [yaw, pitch, distance, x, y, z] => Ok(CameraView {
            yaw,
            pitch,
            distance,
            target: Some(Vec3::new(x, y, z)),
        }),
        _ => Err("expected YAW,PITCH,DISTANCE or YAW,PITCH,DISTANCE,X,Y,Z".into()),
    }
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

/// Frames with nothing loading before the startup camera and screenshot
/// count everything as loaded: meshes and textures reach the GPU a few
/// frames after their jobs finish.
const SETTLE_FRAMES: u32 = 60;

/// Everything that loads after startup.
#[derive(SystemParam)]
struct Loading<'w> {
    sources: Res<'w, Sources>,
    jobs: Res<'w, Jobs>,
    selected: Res<'w, Selected>,
    loaded: Res<'w, Loaded>,
    region: Res<'w, RegionScene>,
}

impl Loading<'_> {
    /// Whether anything is still loading: a source, a job, the startup
    /// selection, its data or a region.
    fn busy(&self, st: &Startup) -> bool {
        let selecting = st.select.is_some() && st.cache.is_some() && self.selected.sel.is_none();
        self.sources.pending() > 0
            || self.jobs.running() > 0
            || selecting
            || self.loaded.is_loading()
            || self.region.is_loading()
    }
}

/// Once nothing has been [loading](Loading::busy) for [`SETTLE_FRAMES`],
/// points the camera at [`Startup::camera`]; that many frames later,
/// saves [`Startup::screenshot`] and closes.
fn startup_camera_and_screenshot(
    mut commands: Commands,
    st: Res<Startup>,
    loading: Loading,
    mut cams: Query<&mut OrbitCamera, With<WorldCamera>>,
    mut idle: Local<u32>,
    mut step: Local<u8>,
) {
    if *step > 1 || (st.camera.is_none() && st.screenshot.is_none()) {
        return;
    }
    if loading.busy(&st) {
        *idle = 0;
        return;
    }
    *idle += 1;
    if *idle < SETTLE_FRAMES {
        return;
    }
    *idle = 0;
    if *step == 0 {
        if let Some(v) = st.camera {
            for mut o in &mut cams {
                o.yaw = v.yaw.to_radians();
                o.pitch = v.pitch.to_radians();
                o.distance = v.distance;
                if let Some(t) = v.target {
                    o.target = t;
                }
            }
        }
        *step = 1;
        return;
    }
    *step = 2;
    let Some(path) = st.screenshot.clone() else {
        return;
    };
    info!("saving a screenshot to {}", path.display());
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path))
        .observe(
            |_: On<bevy::render::view::screenshot::ScreenshotCaptured>,
             mut exit: MessageWriter<AppExit>| {
                exit.write(AppExit::Success);
            },
        );
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
        .add_systems(Update, (select_startup, startup_camera_and_screenshot));
    #[cfg(feature = "caption")]
    app.add_plugins(crate::explorer::captions::plugin);
    app.run();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_views_parse() {
        let v = parse_camera("34, -10,2600").unwrap();
        assert_eq!(
            (v.yaw, v.pitch, v.distance, v.target),
            (34.0, -10.0, 2600.0, None)
        );
        let v = parse_camera("0,50,900,1024,400,-2560").unwrap();
        assert_eq!(v.target, Some(Vec3::new(1024.0, 400.0, -2560.0)));
        assert!(parse_camera("1,2").is_err());
        assert!(parse_camera("1,2,x").is_err());
    }
}
