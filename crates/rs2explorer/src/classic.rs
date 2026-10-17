//! The classic renderer view: the selected model drawn by `rs2render` (a
//! reimplementation of the 317 client's software renderer) into a pixel
//! buffer, shown nearest-neighbour over the viewport.

use std::sync::Arc;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use rs2intermediate::record::{Record, RecordType};
use rs2render::{Framebuffer, LitModel, Textures, light};

use crate::map_view::{ClassicScene, RegionScene};
use crate::selection::Loaded;
use crate::sources::{Source, Sources};
use crate::ui::ViewportRect;
use crate::viewport3d::{Compare, OrbitCamera, Shown, WorldCamera};

/// Which renderer draws the model.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderMode {
    /// Bevy, with client or engine lighting.
    #[default]
    Modern,
    /// The client's software renderer.
    Classic,
}

#[derive(Resource)]
pub struct ClassicView {
    /// The lit model and its vertices (cache space, where the modern
    /// view draws them too).
    pub model: Option<(Arc<LitModel>, Vec<[i32; 3]>)>,
    /// The model at the playing animation frame, relit, when one plays.
    pub pose: Option<(Arc<LitModel>, Vec<[i32; 3]>)>,
    pub textures: Option<Arc<Textures>>,
    /// Screen pixels per classic pixel.
    pub scale: u32,
    pub background: u32,
    loading: Option<Task<Option<Textures>>>,
    texture: Option<egui::TextureHandle>,
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

/// Load the 50 textures from the first cache source, once, off the UI
/// thread.
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
            rs2pack::palette::DEFAULT_BRIGHTNESS,
        ))
    }));
}

/// Light the selected model when the selection changes.
fn prepare(loaded: Res<Loaded>, mut view: ResMut<ClassicView>) {
    if !loaded.is_changed() {
        return;
    }
    let model = loaded.renderable.as_ref().and_then(|r| r.model());
    view.model = model.map(|m| {
        (
            Arc::new(light(m, &rs2render::light::NPC)),
            m.vertices.clone(),
        )
    });
}

/// Whether the classic renderer is drawing.
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

/// Hide the Bevy model while the classic renderer is showing.
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
/// [`mesh::world`]: crate::mesh::world
pub fn scene_view(o: &OrbitCamera) -> rs2render::View {
    let to_cache = |v: Vec3| {
        [
            v.x.round() as i32,
            (-v.y).round() as i32,
            (-v.z).round() as i32,
        ]
    };
    let eye = to_cache(o.transform().translation);
    let target = to_cache(o.target);
    let d = [
        f64::from(target[0] - eye[0]),
        f64::from(target[1] - eye[1]),
        f64::from(target[2] - eye[2]),
    ];
    let units = |r: f64| (r / std::f64::consts::TAU * 2048.0).round() as i32;
    let yaw = units((-d[0]).atan2(d[2]));
    let pitch = units(d[1].atan2((d[0] * d[0] + d[2] * d[2]).sqrt()));
    rs2render::View { eye, yaw, pitch }
}

/// The classic projection's focal length in classic pixels (`x << 9 / z`).
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
    let half = height as f32 / scale as f32 / 2.0;
    2.0 * (half / FOCAL).atan()
}

/// Draw a region scene: terrain planes, then objects far to near.
pub fn render_scene(
    fb: &mut Framebuffer,
    scene: &ClassicScene,
    planes: [bool; 4],
    view: &rs2render::View,
    textures: &Textures,
) {
    for (plane, lit, verts) in &scene.terrain {
        if planes[usize::from(*plane)] {
            rs2render::render_in_view(fb, lit, verts, view, textures);
        }
    }
    let mut order: Vec<(i32, usize)> = scene
        .objects
        .iter()
        .enumerate()
        .filter(|(_, (plane, _, _))| planes[usize::from(*plane)])
        .map(|(i, (_, _, v))| (v.first().map_or(0, |p| view.to_camera(*p)[2]), i))
        .collect();
    order.sort_by_key(|&(d, i)| (std::cmp::Reverse(d), i));
    for (_, i) in order {
        let (_, lit, verts) = &scene.objects[i];
        rs2render::render_in_view(fb, lit, verts, view, textures);
    }
}

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
        rs2render::render_in_view(&mut fb, lit, verts, &scene_view(&o), tex);
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

/// Paint the classic frame over the viewport.
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

/// What a classic frame depends on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameKey {
    size: (usize, usize),
    orbit: [u32; 6],
    planes: [bool; 4],
    scene: Option<usize>,
    model: Option<usize>,
    textures: bool,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<RenderMode>()
        .init_resource::<ClassicView>()
        .add_systems(Update, (load_textures, prepare, visibility, match_fov))
        .add_systems(EguiPrimaryContextPass, draw.after(crate::ui::Layout));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_fov_matches_the_projection() {
        // A frame 1024 classic pixels tall spans ±512 at focal length 512.
        assert!((classic_fov(1024, 1) - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert!((classic_fov(2048, 2) - classic_fov(1024, 1)).abs() < 1e-6);
    }
}
