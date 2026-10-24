//! The classic renderer view: the selected model drawn by `rs2classic` (a
//! reimplementation of the original client's software renderer) into a
//! pixel buffer, shown nearest-neighbour over the viewport.
//!
//! It shares the modern view's [`OrbitCamera`]: [`scene_view`] turns the
//! orbit into a classic camera, and [`classic_fov`] gives the modern
//! cameras the matching field of view, so flipping renderers keeps the
//! framing. A frame is only redrawn when something it depends on changes
//! (see [`FrameKey`]).

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use rs2cache::types::map::REGION_SIZE;
use rs2cache::types::record::{Record, RecordType};
use rs2classic::{Framebuffer, LitModel, Textures, light};

use crate::explorer::map_view::{ClassicScene, RegionScene};
use crate::explorer::selection::Loaded;
use crate::explorer::sources::{Source, Sources};
use crate::explorer::ui::ViewportRect;
use crate::explorer::viewport3d::{Compare, OrbitCamera, Shown, WorldCamera};

/// Which renderer draws the model.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderMode {
    /// Bevy, with 317 or engine lighting. The default.
    #[default]
    Modern,
    /// The original client's software renderer, via `rs2classic`.
    Classic,
}

/// The classic renderer's state: what to draw and the last frame.
#[derive(Resource)]
pub struct ClassicView {
    /// The selection's lit model and its vertices, in cache space. `None`
    /// when the selection has no cache-format model.
    pub model: Option<(Arc<LitModel>, Vec<[i32; 3]>)>,
    /// The model at the playing animation frame, relit, when one plays.
    /// Drawn instead of `model`.
    pub pose: Option<(Arc<LitModel>, Vec<[i32; 3]>)>,
    /// The 50 textures from the first open cache, once loaded. Missing
    /// ones are left empty.
    pub textures: Option<Arc<Textures>>,
    /// Screen pixels per classic pixel, 1–4.
    pub scale: u32,
    /// Clear colour, `0xRRGGBB`.
    pub background: u32,
    // The texture load, while it runs.
    loading: Option<Task<Option<Textures>>>,
    // The frame as an egui texture, reused between frames.
    texture: Option<egui::TextureHandle>,
    // What the current frame was drawn from.
    last: Option<FrameKey>,
}

impl Default for ClassicView {
    fn default() -> Self {
        Self {
            model: None,
            pose: None,
            textures: None,
            scale: 2,
            background: 0x000000,
            loading: None,
            texture: None,
            last: None,
        }
    }
}

/// Loads the 50 textures from the first cache source, once, off the UI
/// thread. Waits quietly until a cache is open.
fn load_textures(sources: Res<Sources>, mut view: ResMut<ClassicView>) {
    if view.textures.is_some() {
        return;
    }
    if let Some(task) = &mut view.loading {
        if let Some(t) = block_on(poll_once(task)) {
            view.loading = None;
            view.textures = Some(Arc::new(t.unwrap_or_default()));
        }
        return;
    }
    let Some(Source::Cache { records, .. }) = sources
        .list
        .iter()
        .find(|s| matches!(s, Source::Cache { .. }))
    else {
        return;
    };
    let records = records.clone();
    view.loading = Some(AsyncComputeTaskPool::get().spawn(async move {
        let sheets: Vec<_> = (0..50)
            .map(|t| match records.decode(RecordType::Texture, t) {
                Ok(Record::Texture(s)) => Some(s),
                _ => None,
            })
            .collect();
        Some(Textures::from_sheets(
            &sheets,
            contentpack::palette::DEFAULT_BRIGHTNESS,
        ))
    }));
}

/// Lights the selected model when the selection changes, with the NPC
/// lighting.
fn prepare(loaded: Res<Loaded>, mut view: ResMut<ClassicView>) {
    if !loaded.is_changed() {
        return;
    }
    let model = loaded.renderable.as_ref().and_then(|r| r.model());
    view.model = model.map(|m| {
        (
            Arc::new(light(m, &rs2classic::light::NPC)),
            m.vertices.clone(),
        )
    });
}

/// Whether the classic renderer is drawing.
///
/// A system parameter, so systems can ask without fetching both resources.
#[derive(SystemParam)]
pub struct ClassicOn<'w> {
    mode: Res<'w, RenderMode>,
    compare: Res<'w, Compare>,
}

impl ClassicOn<'_> {
    /// Classic mode is chosen and compare mode is off: compare mode draws
    /// both panes with the modern renderer (pack models have no classic
    /// form).
    pub fn active(&self) -> bool {
        *self.mode == RenderMode::Classic && !self.compare.enabled
    }
}

/// Hides the Bevy model while the classic renderer is showing.
fn visibility(on: ClassicOn, shown: Res<Shown>, mut q: Query<&mut Visibility>) {
    let Some(e) = shown.main.entity else { return };
    if let Ok(mut v) = q.get_mut(e) {
        let want = if on.active() {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if *v != want {
            *v = want;
        }
    }
}

/// The classic world camera for an orbit state: the eye and target in
/// cache coordinates (Bevy's y and z are negated, see [`mesh::world`]),
/// with yaw and pitch pointing the eye at the target. Single models and
/// regions both draw through it, so the classic and modern views match.
///
/// [`mesh::world`]: crate::explorer::mesh::world
pub fn scene_view(o: &OrbitCamera) -> rs2classic::View {
    // World to cache: negate y and z back, to whole units.
    let to_cache = |v: Vec3| {
        [
            v.x.round() as i32,
            (-v.y).round() as i32,
            (-v.z).round() as i32,
        ]
    };
    let eye = to_cache(o.transform().translation);
    let target = to_cache(o.target);
    // Eye-to-target direction, in cache space.
    let d = [
        f64::from(target[0] - eye[0]),
        f64::from(target[1] - eye[1]),
        f64::from(target[2] - eye[2]),
    ];
    // Radians to the original client's 2048-per-turn angle units.
    let units = |r: f64| (r / std::f64::consts::TAU * 2048.0).round() as i32;
    // Yaw from the ground direction, pitch from the drop over the
    // horizontal distance (y is down, so looking down is positive).
    let yaw = units((-d[0]).atan2(d[2]));
    let pitch = units(d[1].atan2((d[0] * d[0] + d[2] * d[2]).sqrt()));
    rs2classic::View { eye, yaw, pitch }
}

/// The classic projection's focal length in classic pixels: it projects
/// `x << 9 / z`, and `1 << 9` is 512.
const FOCAL: f32 = 512.0;

/// Give the modern cameras the classic projection's vertical field of view
/// for the current viewport and pixel size, so both renderers frame the
/// same view.
fn match_fov(
    rect: Res<ViewportRect>,
    view: Res<ClassicView>,
    mut q: Query<&mut Projection, With<WorldCamera>>,
) {
    if !(rect.is_changed() || view.is_changed()) || rect.size.y == 0 {
        return;
    }
    let fov = classic_fov(rect.size.y, view.scale.max(1));
    for mut p in &mut q {
        if let Projection::Perspective(pp) = &mut *p
            && (pp.fov - fov).abs() > 1e-6
        {
            pp.fov = fov;
        }
    }
}

/// The vertical field of view of a classic frame `height` physical pixels
/// tall drawn at `scale` physical pixels per classic pixel.
pub fn classic_fov(height: u32, scale: u32) -> f32 {
    // Half the frame height in classic pixels; the half-angle's tangent is
    // that over the focal length.
    let half = height as f32 / scale as f32 / 2.0;
    2.0 * (half / FOCAL).atan()
}

/// One step of drawing a region scene, from [`draw_order`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Draw {
    /// One tile's ground on a plane: an index into
    /// [`Ground::tile_faces`](rs2classic::terrain::Ground::tile_faces).
    Ground {
        /// The plane.
        plane: u8,
        /// The tile, `x * 64 + y`.
        tile: usize,
    },
    /// A placed object: an index into [`ClassicScene::objects`].
    Object(usize),
}

/// The order the original client draws a scene in: plane by plane, lowest
/// first; within a plane, tile by tile from the far edges in towards the
/// camera's tile; within a tile, its ground and then the objects standing
/// there, far to near.
///
/// Tiles are visited furthest first along x from the camera's tile, then
/// along y, west before east and south before north. An object bigger than
/// a tile is drawn at the tile of its footprint nearest the camera, once
/// all the ground under it is down. Planes switched off are left out.
pub fn draw_order(scene: &ClassicScene, planes: [bool; 4], view: &rs2classic::View) -> Vec<Draw> {
    let n = REGION_SIZE as i32;
    let camera = |c: i32| (c.div_euclid(128)).clamp(0, n - 1);
    let (cx, cy) = (camera(view.eye[0]), camera(view.eye[2]));
    let mut tiles: Vec<(i32, i32)> = (0..n).flat_map(|x| (0..n).map(move |y| (x, y))).collect();
    tiles.sort_by_key(|&(x, y)| {
        (
            std::cmp::Reverse((x - cx).abs()),
            std::cmp::Reverse((y - cy).abs()),
            x > cx,
            y > cy,
        )
    });
    // Each object's tile: the one of its footprint nearest the camera.
    let mut at: BTreeMap<(u8, i32, i32), Vec<(i32, usize)>> = BTreeMap::new();
    for (i, o) in scene.objects.iter().enumerate() {
        let near = |lo: u8, len: u8, c: i32| {
            c.clamp(i32::from(lo), i32::from(lo) + i32::from(len.max(1)) - 1)
        };
        let tile = (near(o.tile.0, o.size.0, cx), near(o.tile.1, o.size.1, cy));
        // Depth: the first vertex's camera-space z.
        let depth = o.vertices.first().map_or(0, |v| view.to_camera(*v)[2]);
        at.entry((o.plane, tile.0, tile.1))
            .or_default()
            .push((depth, i));
    }
    for list in at.values_mut() {
        list.sort_by_key(|&(d, i)| (std::cmp::Reverse(d), i));
    }
    let mut out = Vec::new();
    for plane in 0..4u8 {
        if !planes[usize::from(plane)] {
            continue;
        }
        let ground = scene
            .terrain
            .iter()
            .find(|(p, _)| *p == plane)
            .map(|(_, g)| g);
        for &(x, y) in &tiles {
            let tile = (x * n + y) as usize;
            if ground.is_some_and(|g| !g.tile_faces[tile].is_empty()) {
                out.push(Draw::Ground { plane, tile });
            }
            if let Some(list) = at.get(&(plane, x, y)) {
                out.extend(list.iter().map(|&(_, i)| Draw::Object(i)));
            }
        }
    }
    out
}

/// Draws a region scene in [`draw_order`], skipping planes that are
/// switched off.
pub fn render_scene(
    fb: &mut Framebuffer,
    scene: &ClassicScene,
    planes: [bool; 4],
    view: &rs2classic::View,
    textures: &Textures,
) {
    // Each plane's ground is projected once, then drawn tile by tile.
    let projected: Vec<_> = scene
        .terrain
        .iter()
        .map(|(p, g)| {
            (
                *p,
                rs2classic::render::project_in_view(fb, &g.vertices, view),
            )
        })
        .collect();
    for d in draw_order(scene, planes, view) {
        match d {
            Draw::Ground { plane, tile } => {
                let Some(((_, g), (_, p))) = scene
                    .terrain
                    .iter()
                    .zip(&projected)
                    .find(|((q, _), _)| *q == plane)
                else {
                    continue;
                };
                rs2classic::render::draw_in_order(
                    fb,
                    &g.model,
                    p,
                    g.tile_faces[tile].clone(),
                    textures,
                );
            }
            Draw::Object(i) => {
                let o = &scene.objects[i];
                rs2classic::render_in_view(fb, &o.model, &o.vertices, view, textures);
            }
        }
    }
}

/// Renders and paints the classic frame, when the classic renderer is on
/// and there's a model or a region to draw. A single model wins over a
/// region.
fn draw(
    mut contexts: EguiContexts,
    on: ClassicOn,
    mut view: ResMut<ClassicView>,
    region: Res<RegionScene>,
    rect: Res<ViewportRect>,
    orbit: Query<&OrbitCamera>,
    window: Single<&Window, With<PrimaryWindow>>,
) -> Result {
    let scene = region.classic.clone().filter(|_| view.model.is_none());
    if !on.active() || (view.model.is_none() && scene.is_none()) {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?.clone();
    let scale = view.scale.max(1);
    // The frame in classic pixels: the viewport divided by the pixel size.
    let (w, h) = (
        (rect.size.x / scale).max(1) as usize,
        (rect.size.y / scale).max(1) as usize,
    );
    let Some(o) = orbit.iter().next().copied() else {
        return Ok(());
    };
    // Redraw only when something that affects the frame changed.
    let key = Some(FrameKey {
        size: (w, h),
        orbit: [
            o.yaw, o.pitch, o.distance, o.target.x, o.target.y, o.target.z,
        ]
        .map(f32::to_bits),
        planes: region.planes,
        scene: scene.as_ref().map(|s| Arc::as_ptr(s) as usize),
        model: view
            .pose
            .as_ref()
            .or(view.model.as_ref())
            .map(|m| Arc::as_ptr(&m.0) as usize),
        textures: view.textures.is_some(),
    });
    // Same frame as last time: just paint the old texture again.
    if key == view.last && view.texture.is_some() {
        paint(&ctx, &view, &rect, &window, w, h, scale);
        return Ok(());
    }
    view.last = key;
    let mut fb = Framebuffer::new(w, h);
    fb.clear(view.background);
    let empty = Textures::default();
    let tex = view.textures.clone();
    let tex = tex.as_deref().unwrap_or(&empty);
    if let Some((lit, verts)) = view.pose.as_ref().or(view.model.as_ref()) {
        rs2classic::render_in_view(&mut fb, lit, verts, &scene_view(&o), tex);
    } else if let Some(scene) = &scene {
        render_scene(&mut fb, scene, region.planes, &scene_view(&o), tex);
    }
    let image = egui::ColorImage::from_rgba_unmultiplied([w, h], &fb.to_rgba());
    match &mut view.texture {
        Some(t) => t.set(image, egui::TextureOptions::NEAREST),
        None => {
            view.texture = Some(ctx.load_texture("classic", image, egui::TextureOptions::NEAREST))
        }
    }
    paint(&ctx, &view, &rect, &window, w, h, scale);
    Ok(())
}

/// Paints the classic frame over the viewport, `scale` physical pixels per
/// classic pixel, from the top-left (a few pixels may be left uncovered at
/// the right and bottom).
fn paint(
    ctx: &egui::Context,
    view: &ClassicView,
    rect: &ViewportRect,
    window: &Window,
    w: usize,
    h: usize,
    scale: u32,
) {
    let Some(handle) = &view.texture else { return };
    let s = window.scale_factor();
    let min = egui::pos2(rect.position.x as f32 / s, rect.position.y as f32 / s);
    let size = egui::vec2((w as u32 * scale) as f32 / s, (h as u32 * scale) as f32 / s);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::new("classic"),
    ));
    painter.image(
        handle.id(),
        egui::Rect::from_min_size(min, size),
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
}

/// What a classic frame depends on. Floats are kept as bits so the key can
/// be `Eq`; models and scenes by `Arc` address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameKey {
    size: (usize, usize),
    orbit: [u32; 6],
    planes: [bool; 4],
    scene: Option<usize>,
    model: Option<usize>,
    textures: bool,
}

/// Adds the classic renderer's resources and systems. Drawing runs after
/// the panel layout.
pub fn plugin(app: &mut App) {
    app.init_resource::<RenderMode>()
        .init_resource::<ClassicView>()
        .add_systems(Update, (load_textures, prepare, visibility, match_fov))
        .add_systems(
            EguiPrimaryContextPass,
            draw.after(crate::explorer::ui::Layout),
        );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scene with ground everywhere on plane 0 and on a patch of plane 2
    /// (a roof over tiles 4–6), objects on plane 0 under the roof and
    /// elsewhere, and a 3 × 3 object.
    fn scene() -> ClassicScene {
        use crate::explorer::map_view::SceneObject;
        use rs2cache::types::map::Plane;
        use rs2classic::terrain::{FloorColour, Tiles, ground};
        let n = REGION_SIZE * REGION_SIZE;
        let mut p = Plane {
            heights: vec![0; n],
            height_derived: vec![false; n],
            overlays: vec![0; n],
            shapes: vec![0; n],
            rotations: vec![0; n],
            flags: vec![0; n],
            underlays: vec![1; n],
        };
        let floors = [FloorColour::from_rgb(0x40a040)];
        let low = ground(&Tiles::region(&p), &floors, None).unwrap();
        p.underlays = vec![0; n];
        for x in 4..7 {
            for y in 4..7 {
                p.underlays[x * REGION_SIZE + y] = 1;
            }
        }
        p.heights = vec![-480; n];
        let roof = ground(&Tiles::region(&p), &floors, None).unwrap();
        let object = |tile: (u8, u8), size| SceneObject {
            plane: 0,
            tile,
            size,
            model: Arc::new(LitModel::flat_rgb(Vec::new(), Vec::new())),
            vertices: vec![[i32::from(tile.0) * 128, 0, i32::from(tile.1) * 128]],
        };
        ClassicScene {
            terrain: vec![(0, low), (2, roof)],
            objects: vec![
                object((5, 5), (1, 1)),
                object((40, 40), (1, 1)),
                object((20, 20), (3, 3)),
            ],
            centre: [0; 3],
        }
    }

    #[test]
    fn roofs_draw_after_everything_beneath_them() {
        let s = scene();
        // The camera over tile (30, 30).
        let view = rs2classic::View {
            eye: [30 * 128, -2000, 30 * 128],
            yaw: 0,
            pitch: 256,
        };
        let order = draw_order(&s, [true; 4], &view);
        let pos = |d: Draw| order.iter().position(|&x| x == d).unwrap();
        let ground = |plane, x: usize, y: usize| Draw::Ground {
            plane,
            tile: x * REGION_SIZE + y,
        };
        // Every roof tile comes after every plane-0 object.
        let last_object = (0..3).map(|i| pos(Draw::Object(i))).max().unwrap();
        for x in 4..7 {
            for y in 4..7 {
                assert!(pos(ground(2, x, y)) > last_object);
            }
        }
        // A tile's ground goes down before what stands on it.
        assert!(pos(ground(0, 5, 5)) < pos(Draw::Object(0)));
        assert!(pos(ground(0, 40, 40)) < pos(Draw::Object(1)));
        // A big object waits for all the ground under it.
        for x in 20..23 {
            for y in 20..23 {
                assert!(pos(ground(0, x, y)) < pos(Draw::Object(2)), "tile {x}, {y}");
            }
        }
        // Far tiles first: the corner tile (0, 0) is drawn before the
        // camera's own tile.
        assert!(pos(ground(0, 0, 0)) < pos(ground(0, 30, 30)));
        // Plane 2 switched off: no roof.
        let order = draw_order(&s, [true, true, false, true], &view);
        assert!(!order.contains(&ground(2, 5, 5)));
        assert_eq!(order.len(), 64 * 64 + 3);
    }

    #[test]
    fn classic_fov_matches_the_projection() {
        // A frame 1024 classic pixels tall spans ±512 at focal length 512.
        assert!((classic_fov(1024, 1) - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert!((classic_fov(2048, 2) - classic_fov(1024, 1)).abs() < 1e-6);
    }
}
