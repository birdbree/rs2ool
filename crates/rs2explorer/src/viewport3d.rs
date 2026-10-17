//! The 3D viewport: a world camera whose viewport follows the central rect
//! (under a separate camera that draws only egui), an orbit controller,
//! the selected model, and scale furniture.

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{CameraOutputMode, Viewport};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use bevy::render::render_resource::BlendState;
use bevy::window::PrimaryWindow;
use bevy_egui::input::{egui_wants_any_keyboard_input, egui_wants_any_pointer_input};
use bevy_egui::{EguiContext, EguiGlobalSettings, PrimaryEguiContext};
use rs2intermediate::record::Record;

use crate::mesh::{self, NPC_LIGHTING, Shading};
use crate::selection::{Loaded, LoadedData};
use crate::ui::ViewportRect;

/// Marks the world camera(s).
#[derive(Component)]
pub struct WorldCamera;

/// Orbit state: the camera looks at `target` from `distance` away, at
/// `yaw` around y and `pitch` above the horizon (radians).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct OrbitCamera {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub target: Vec3,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            yaw: 0.6,
            pitch: 0.35,
            distance: 450.0,
            target: Vec3::new(0.0, 100.0, 0.0),
        }
    }
}

pub const PITCH_LIMIT: f32 = 85.0 * std::f32::consts::PI / 180.0;
const KEY_SPEED: f32 = 1.5;
const DRAG_SPEED: f32 = 0.008;

impl OrbitCamera {
    pub fn transform(&self) -> Transform {
        let offset = Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        ) * self.distance;
        Transform::from_translation(self.target + offset).looking_at(self.target, Vec3::Y)
    }

    /// Turn by `dyaw`, `dpitch` (pitch clamped to ±85°).
    pub fn turn(&mut self, dyaw: f32, dpitch: f32) {
        self.yaw += dyaw;
        self.pitch = (self.pitch + dpitch).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }
}

/// Which lighting the model is drawn with.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LightingMode {
    /// The client's fixed directional term, baked, drawn unlit.
    #[default]
    Client,
    /// Engine lighting.
    Real,
}

/// Overlay toggles.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct Overlays {
    pub labels: bool,
    pub bones: bool,
}

/// The model on screen: one mesh per lighting mode, built once, so
/// switching mode swaps handles without rebuilding anything.
#[derive(Resource, Default)]
pub struct Shown {
    pub entity: Option<Entity>,
    pub client: Handle<Mesh>,
    pub real: Handle<Mesh>,
    pub unlit: Handle<StandardMaterial>,
    pub lit: Handle<StandardMaterial>,
    /// Blended variants, only for models with per-face alpha: the blended
    /// pass does not write depth, so an opaque model drawn with it shows
    /// its inner faces through its outer ones.
    pub unlit_blend: Handle<StandardMaterial>,
    pub lit_blend: Handle<StandardMaterial>,
    pub blend: bool,
    pub labels: bool,
}

fn setup(
    mut commands: Commands,
    mut egui_settings: ResMut<EguiGlobalSettings>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut shown: ResMut<Shown>,
) {
    egui_settings.auto_create_primary_context = false;
    let orbit = OrbitCamera::default();
    commands.spawn((WorldCamera, orbit, Camera3d::default(), orbit.transform()));
    commands.spawn((
        DirectionalLight {
            illuminance: 8000.0,
            ..default()
        },
        Transform::from_xyz(300.0, 600.0, 400.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.insert_resource(GlobalAmbientLight {
        brightness: 400.0,
        ..default()
    });
    let unlit = StandardMaterial {
        unlit: true,
        double_sided: true,
        cull_mode: None,
        ..default()
    };
    let lit = StandardMaterial {
        perceptual_roughness: 1.0,
        double_sided: true,
        cull_mode: None,
        ..default()
    };
    shown.unlit_blend = materials.add(StandardMaterial {
        alpha_mode: AlphaMode::Blend,
        ..unlit.clone()
    });
    shown.lit_blend = materials.add(StandardMaterial {
        alpha_mode: AlphaMode::Blend,
        ..lit.clone()
    });
    shown.unlit = materials.add(unlit);
    shown.lit = materials.add(lit);
    commands.spawn((
        PrimaryEguiContext,
        Camera2d,
        RenderLayers::none(),
        Camera {
            order: 10,
            output_mode: CameraOutputMode::Write {
                blend_state: Some(BlendState::ALPHA_BLENDING),
                clear_color: ClearColorConfig::None,
            },
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
    ));
}

fn follow_rect(
    rect: Res<ViewportRect>,
    mut cameras: Query<&mut Camera, (With<WorldCamera>, Without<EguiContext>)>,
) {
    if !rect.is_changed() || rect.size == UVec2::ZERO {
        return;
    }
    for mut cam in &mut cameras {
        cam.viewport = Some(Viewport {
            physical_position: rect.position,
            physical_size: rect.size,
            ..default()
        });
    }
}

/// Arrow keys orbit: left/right yaw, up/down pitch.
pub fn orbit_keys(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut q: Query<&mut OrbitCamera>,
) {
    let step = KEY_SPEED * time.delta_secs().max(1.0 / 120.0);
    let (mut dy, mut dp) = (0.0, 0.0);
    if keys.pressed(KeyCode::ArrowLeft) {
        dy -= step;
    }
    if keys.pressed(KeyCode::ArrowRight) {
        dy += step;
    }
    if keys.pressed(KeyCode::ArrowUp) {
        dp += step;
    }
    if keys.pressed(KeyCode::ArrowDown) {
        dp -= step;
    }
    if dy != 0.0 || dp != 0.0 {
        for mut o in &mut q {
            o.turn(dy, dp);
        }
    }
}

/// Whether a physical cursor position lies in the viewport rect. The
/// panels are drawn on egui's background layer, so egui's "wants pointer"
/// flag does not cover them; the rect is the reliable test.
pub fn in_viewport(cursor: Option<Vec2>, rect: &ViewportRect) -> bool {
    let Some(p) = cursor else { return false };
    let (lo, hi) = (
        rect.position.as_vec2(),
        (rect.position + rect.size).as_vec2(),
    );
    p.x >= lo.x && p.y >= lo.y && p.x < hi.x && p.y < hi.y
}

/// Dragging orbits and scrolling zooms, only with the cursor over the
/// viewport; a drag that starts there may continue past its edge.
fn orbit_mouse(
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    rect: Res<ViewportRect>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut dragging: Local<bool>,
    mut q: Query<&mut OrbitCamera>,
) {
    let inside = in_viewport(window.physical_cursor_position(), &rect);
    if buttons.just_pressed(MouseButton::Left) {
        *dragging = inside;
    }
    if !buttons.pressed(MouseButton::Left) {
        *dragging = false;
    }
    for mut o in &mut q {
        if *dragging && motion.delta != Vec2::ZERO {
            o.turn(-motion.delta.x * DRAG_SPEED, motion.delta.y * DRAG_SPEED);
        }
        if inside && scroll.delta.y != 0.0 {
            o.distance = (o.distance * (1.0 - scroll.delta.y * 0.1)).clamp(20.0, 20_000.0);
        }
    }
}

fn apply_orbit(mut q: Query<(&OrbitCamera, &mut Transform), Changed<OrbitCamera>>) {
    for (o, mut t) in &mut q {
        *t = o.transform();
    }
}

/// Show the selected model (or the first model of a selected definition's
/// list, decoded already by the selection) when it changes.
fn show_selection(
    loaded: Res<Loaded>,
    overlays: Res<Overlays>,
    mode: Res<LightingMode>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut shown: ResMut<Shown>,
    mut cams: Query<&mut OrbitCamera>,
) {
    if !loaded.is_changed() && !overlays.is_changed() {
        return;
    }
    let Some(data) = &loaded.data else { return };
    let LoadedData::Record { record, .. } = &**data else {
        return;
    };
    let Record::Model(m) = &**record else { return };
    if let Some(e) = shown.entity.take() {
        commands.entity(e).despawn();
    }
    let client = mesh::mesh_from_model(
        m,
        &m.vertices,
        Shading::Client(NPC_LIGHTING),
        overlays.labels,
    );
    let real = mesh::mesh_from_model(m, &m.vertices, Shading::Raw, overlays.labels);
    if let Some((lo, hi)) = mesh::bounds(&real) {
        let (lo, hi) = (Vec3::from(lo), Vec3::from(hi));
        for mut o in &mut cams {
            o.target = (lo + hi) / 2.0;
            o.distance = ((hi - lo).length() * 1.6).max(100.0);
        }
    }
    shown.client = meshes.add(client);
    shown.real = meshes.add(real);
    shown.labels = overlays.labels;
    shown.blend = m
        .face_alphas
        .as_ref()
        .is_some_and(|a| a.iter().any(|&x| x != 0));
    let (mesh, mat) = handles(&shown, *mode);
    shown.entity = Some(
        commands
            .spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::IDENTITY))
            .id(),
    );
}

/// The mesh and material for a lighting mode.
pub fn handles(shown: &Shown, mode: LightingMode) -> (Handle<Mesh>, Handle<StandardMaterial>) {
    let (mesh, opaque, blend) = match mode {
        LightingMode::Client => (&shown.client, &shown.unlit, &shown.unlit_blend),
        LightingMode::Real => (&shown.real, &shown.lit, &shown.lit_blend),
    };
    (
        mesh.clone(),
        if shown.blend { blend } else { opaque }.clone(),
    )
}

fn switch_lighting(
    mode: Res<LightingMode>,
    shown: Res<Shown>,
    mut q: Query<(&mut Mesh3d, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    if !mode.is_changed() {
        return;
    }
    let Some(e) = shown.entity else { return };
    if let Ok((mut m, mut mat)) = q.get_mut(e) {
        let (mesh, material) = handles(&shown, *mode);
        m.0 = mesh;
        mat.0 = material;
    }
}

/// A 128-unit grid floor, a 128 × 240 × 128 box (one tile, one storey) and
/// axes, so scale errors show.
fn furniture(mut gizmos: Gizmos) {
    let grey = Color::srgba(0.5, 0.5, 0.55, 0.35);
    gizmos.grid(
        Isometry3d::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
        UVec2::new(16, 16),
        Vec2::splat(128.0),
        grey,
    );
    let (w, h) = (64.0, 240.0);
    let c = [
        Vec3::new(-w, 0.0, -w),
        Vec3::new(w, 0.0, -w),
        Vec3::new(w, 0.0, w),
        Vec3::new(-w, 0.0, w),
    ];
    let box_colour = Color::srgba(0.9, 0.8, 0.2, 0.5);
    for i in 0..4 {
        let (a, b) = (c[i], c[(i + 1) % 4]);
        gizmos.line(a, b, box_colour);
        gizmos.line(a + Vec3::Y * h, b + Vec3::Y * h, box_colour);
        gizmos.line(a, a + Vec3::Y * h, box_colour);
    }
    gizmos.axes(Transform::IDENTITY, 64.0);
}

pub fn plugin(app: &mut App) {
    app.init_resource::<LightingMode>()
        .init_resource::<Overlays>()
        .init_resource::<Shown>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                follow_rect,
                orbit_keys.run_if(not(egui_wants_any_keyboard_input)),
                orbit_mouse.run_if(not(egui_wants_any_pointer_input)),
                apply_orbit,
                show_selection,
                switch_lighting,
                furniture,
            )
                .chain(),
        );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_hit_test() {
        let rect = ViewportRect {
            position: UVec2::new(100, 50),
            size: UVec2::new(200, 100),
        };
        assert!(in_viewport(Some(Vec2::new(150.0, 60.0)), &rect));
        assert!(
            !in_viewport(Some(Vec2::new(50.0, 60.0)), &rect),
            "over the left panel"
        );
        assert!(
            !in_viewport(Some(Vec2::new(150.0, 160.0)), &rect),
            "over the bottom panel"
        );
        assert!(!in_viewport(None, &rect));
    }
}
