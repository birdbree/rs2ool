//! The classic renderer view: the selected model drawn by `rs2render` (a
//! reimplementation of the 317 client's software renderer) into a pixel
//! buffer, shown nearest-neighbour over the viewport.

use std::sync::Arc;

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use rs2intermediate::record::{Record, RecordType};
use rs2render::{Camera, Framebuffer, LitModel, Textures, light};

use crate::selection::{Loaded, LoadedData};
use crate::sources::{Source, Sources};
use crate::ui::ViewportRect;
use crate::viewport3d::{OrbitCamera, Shown};

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
    /// The lit model and its vertices centred on the origin.
    pub model: Option<(Arc<LitModel>, Vec<[i32; 3]>)>,
    pub textures: Option<Arc<Textures>>,
    /// Screen pixels per classic pixel.
    pub scale: u32,
    pub background: u32,
    loading: Option<Task<Option<Textures>>>,
    texture: Option<egui::TextureHandle>,
}

impl Default for ClassicView {
    fn default() -> Self {
        Self {
            model: None,
            textures: None,
            scale: 2,
            background: 0x000000,
            loading: None,
            texture: None,
        }
    }
}

/// Centre a model's vertices on the origin, as the orbit camera expects.
pub fn centred(v: &[[i32; 3]]) -> Vec<[i32; 3]> {
    let (mut lo, mut hi) = ([i32::MAX; 3], [i32::MIN; 3]);
    for p in v {
        for a in 0..3 {
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a]);
        }
    }
    let c = [0, 1, 2].map(|a| {
        if lo[a] <= hi[a] {
            (lo[a] + hi[a]) / 2
        } else {
            0
        }
    });
    v.iter()
        .map(|p| [p[0] - c[0], p[1] - c[1], p[2] - c[2]])
        .collect()
}

/// The classic camera for an orbit state: radians to 2048-unit angles.
pub fn camera(o: &OrbitCamera) -> Camera {
    let units = |r: f32| (r / std::f32::consts::TAU * 2048.0).round() as i32;
    Camera::orbit(units(-o.yaw), units(o.pitch), o.distance.round() as i32)
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
    let Some(data) = &loaded.data else { return };
    let LoadedData::Record { preview, .. } = &**data else {
        return;
    };
    view.model = preview.as_ref().map(|m| {
        (
            Arc::new(light(m, &rs2render::light::NPC)),
            centred(&m.vertices),
        )
    });
}

/// Hide the Bevy model while the classic renderer is showing.
fn visibility(mode: Res<RenderMode>, shown: Res<Shown>, mut q: Query<&mut Visibility>) {
    let Some(e) = shown.entity else { return };
    if let Ok(mut v) = q.get_mut(e) {
        let want = if *mode == RenderMode::Classic {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if *v != want {
            *v = want;
        }
    }
}

/// Render and paint the classic view over the viewport.
fn draw(
    mut contexts: EguiContexts,
    mode: Res<RenderMode>,
    mut view: ResMut<ClassicView>,
    rect: Res<ViewportRect>,
    orbit: Query<&OrbitCamera>,
    window: Single<&Window, With<PrimaryWindow>>,
) -> Result {
    if *mode != RenderMode::Classic {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?.clone();
    let scale = view.scale.max(1);
    let (w, h) = (
        (rect.size.x / scale).max(1) as usize,
        (rect.size.y / scale).max(1) as usize,
    );
    let mut fb = Framebuffer::new(w, h);
    fb.clear(view.background);
    if let (Some((lit, verts)), Some(o)) = (&view.model, orbit.iter().next()) {
        let empty = Textures::default();
        let tex = view.textures.as_deref().unwrap_or(&empty);
        rs2render::render(&mut fb, lit, verts, &camera(o), tex);
    }
    let image = egui::ColorImage::from_rgba_unmultiplied([w, h], &fb.to_rgba());
    let handle = match &mut view.texture {
        Some(t) => {
            t.set(image, egui::TextureOptions::NEAREST);
            t.clone()
        }
        None => {
            let t = ctx.load_texture("classic", image, egui::TextureOptions::NEAREST);
            view.texture = Some(t.clone());
            t
        }
    };
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
    Ok(())
}

pub fn plugin(app: &mut App) {
    app.init_resource::<RenderMode>()
        .init_resource::<ClassicView>()
        .add_systems(Update, (load_textures, prepare, visibility))
        .add_systems(EguiPrimaryContextPass, draw.after(crate::ui::Layout));
}
