//! The 3D viewport: a world camera whose viewport follows the central rect
//! (under a separate camera that draws only egui), an orbit controller,
//! the selected model, and scale furniture.

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{CameraOutputMode, Viewport};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::render::render_resource::BlendState;
use bevy::window::PrimaryWindow;
use bevy_egui::input::{egui_wants_any_keyboard_input, egui_wants_any_pointer_input};
use bevy_egui::{EguiGlobalSettings, PrimaryEguiContext};

use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParam;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use rs2classic::texture::TexturePage;

use crate::explorer::classic::ClassicView;
use crate::explorer::mesh::{self, NPC_LIGHTING, Shading};
use crate::explorer::renderable::Renderable;
use crate::explorer::renderable::find_match;
use crate::explorer::selection::{CompareSelected, Loaded, Selected};
use crate::explorer::sources::Sources;
use crate::explorer::ui::ViewportRect;
use contentpack::records::model::PackModel;

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
    pub limits: Limits,
}

/// Where the camera may go: the target stays in a box around the scene,
/// the distance in a range sized to it, and the pitch above `min_pitch`
/// (a region keeps the eye above the ground).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    pub lo: Vec3,
    pub hi: Vec3,
    pub near: f32,
    pub far: f32,
    pub min_pitch: f32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            lo: Vec3::splat(-20_000.0),
            hi: Vec3::splat(20_000.0),
            near: 20.0,
            far: 20_000.0,
            min_pitch: -PITCH_LIMIT,
        }
    }
}

impl Limits {
    /// Limits for a scene spanning `lo..hi`, with `margin` of slack around
    /// it for the target.
    pub fn around(lo: Vec3, hi: Vec3, margin: f32, min_pitch: f32) -> Self {
        let size = (hi - lo).length().max(50.0);
        Self {
            lo: lo - Vec3::splat(margin),
            hi: hi + Vec3::splat(margin),
            near: (size * 0.05).max(60.0),
            far: size * 4.0,
            min_pitch,
        }
    }
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            yaw: 0.6,
            pitch: 0.35,
            distance: 450.0,
            target: Vec3::new(0.0, 100.0, 0.0),
            limits: Limits::default(),
        }
    }
}

pub const PITCH_LIMIT: f32 = 85.0 * std::f32::consts::PI / 180.0;
const KEY_SPEED: f32 = 1.0;
const DRAG_SPEED: f32 = 0.008;
/// Fly speed, in multiples of the orbit distance per second.
const FLY_SPEED: f32 = 0.4;
/// Zoom per scroll line, and per pixel for trackpads.
const ZOOM_PER_LINE: f32 = 0.04;
const ZOOM_PER_PIXEL: f32 = 0.0015;

impl OrbitCamera {
    pub fn transform(&self) -> Transform {
        let offset = Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        ) * self.distance;
        Transform::from_translation(self.target + offset).looking_at(self.target, Vec3::Y)
    }

    /// Turn by `dyaw`, `dpitch` (pitch clamped to ±85° and the limits).
    pub fn turn(&mut self, dyaw: f32, dpitch: f32) {
        self.yaw += dyaw;
        self.pitch = (self.pitch + dpitch).clamp(self.limits.min_pitch, PITCH_LIMIT);
    }

    /// Zoom by `factor` (below 1 moves in), within the limits.
    pub fn zoom(&mut self, factor: f32) {
        self.distance = (self.distance * factor).clamp(self.limits.near, self.limits.far);
    }

    /// Move the target by `right`, `forward` (on the ground, relative to
    /// the view direction) and `up`, within the limits.
    pub fn fly(&mut self, right: f32, forward: f32, up: f32) {
        let (s, c) = self.yaw.sin_cos();
        let d = Vec3::new(c, 0.0, -s) * right + Vec3::new(-s, 0.0, -c) * forward + Vec3::Y * up;
        self.target = (self.target + d).clamp(self.limits.lo, self.limits.hi);
    }

    /// Look at a model spanning `lo..hi` from a distance that fits it,
    /// bounded around it.
    pub fn frame(&mut self, (lo, hi): (Vec3, Vec3)) {
        self.target = (lo + hi) / 2.0;
        self.distance = ((hi - lo).length() * 1.6).max(100.0);
        let margin = (hi - lo).max_element() * 0.5;
        self.bound(Limits::around(lo, hi, margin, -PITCH_LIMIT));
    }

    /// Set new limits and pull the state inside them.
    pub fn bound(&mut self, limits: Limits) {
        self.limits = limits;
        self.target = self.target.clamp(limits.lo, limits.hi);
        self.zoom(1.0);
        self.turn(0.0, 0.0);
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

/// One drawn part of a shown model: the faces with one texture (or none),
/// as a child entity of [`ShownModel::entity`].
#[derive(Debug, Clone)]
pub struct ShownPart {
    pub entity: Entity,
    pub texture: Option<u16>,
    pub client: Handle<Mesh>,
    pub real: Handle<Mesh>,
    /// Whether its model has per-face alpha (see [`Shown::unlit_blend`]).
    pub blend: bool,
}

/// One model on screen: a parent entity and its parts.
#[derive(Debug, Clone, Default)]
pub struct ShownModel {
    pub entity: Option<Entity>,
    pub parts: Vec<ShownPart>,
}

/// The models on screen, the main one and compare mode's right one: per
/// part, one mesh per lighting mode, built once, so switching mode swaps
/// handles without rebuilding anything.
#[derive(Resource, Default)]
pub struct Shown {
    pub main: ShownModel,
    pub compare: ShownModel,
    pub unlit: Handle<StandardMaterial>,
    pub lit: Handle<StandardMaterial>,
    /// Blended variants, only for models with per-face alpha: the blended
    /// pass does not write depth, so an opaque model drawn with it shows
    /// its inner faces through its outer ones.
    pub unlit_blend: Handle<StandardMaterial>,
    pub lit_blend: Handle<StandardMaterial>,
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
    // Compare mode's right pane: layer 1, following the main camera.
    commands.spawn((
        WorldCamera,
        ComparePane,
        Camera3d::default(),
        // A clear clears the whole target, not just this viewport, so only
        // the main camera clears.
        Camera {
            is_active: false,
            order: 1,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        RenderLayers::layer(COMPARE_LAYER),
        orbit.transform(),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 8000.0,
            ..default()
        },
        Transform::from_xyz(300.0, 600.0, 400.0).looking_at(Vec3::ZERO, Vec3::Y),
        RenderLayers::from_layers(&[0, COMPARE_LAYER]),
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

/// The system that spawns the shown models; posing runs after it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ShowSet;

/// The right pane's camera in compare mode.
#[derive(Component)]
pub struct ComparePane;

/// The render layer of compare mode's right-hand model.
pub const COMPARE_LAYER: usize = 1;

/// Compare mode: the right pane shows the other version of the selected
/// model (original beside converted), kept in [`CompareSelected`] by
/// [`follow_match`].
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Compare {
    pub enabled: bool,
}

/// Keep the right pane on the selection's match: a cache model or NPC and
/// its converted pack record, either way round. Nothing when compare mode
/// is off or no open source has the other version.
pub fn follow_match(
    compare: Res<Compare>,
    selected: Res<Selected>,
    sources: Res<Sources>,
    mut right: ResMut<CompareSelected>,
) {
    if !(compare.is_changed() || selected.is_changed() || sources.is_changed()) {
        return;
    }
    let matched = selected
        .source
        .zip(selected.sel.as_ref())
        .filter(|_| compare.enabled)
        .and_then(|(si, sel)| find_match(&sources.list, si, sel));
    let want = matched.map_or_else(Selected::default, |(si, sel)| Selected {
        source: Some(si),
        sel: Some(sel),
    });
    if right.0 != want {
        right.0 = want;
    }
}

/// The viewport of each world camera: the whole rect, or its left and
/// right halves in compare mode.
pub fn pane_viewport(rect: &ViewportRect, compare: bool, right: bool) -> Viewport {
    let (mut position, mut size) = (rect.position, rect.size);
    if compare {
        let left = size.x / 2;
        size.x = if right { size.x - left } else { left };
        if right {
            position.x += left;
        }
    }
    Viewport {
        physical_position: position,
        physical_size: size.max(UVec2::ONE),
        ..default()
    }
}

fn follow_rect(
    rect: Res<ViewportRect>,
    compare: Res<Compare>,
    mut cameras: Query<(&mut Camera, Has<ComparePane>), With<WorldCamera>>,
) {
    if !(rect.is_changed() || compare.is_changed()) || rect.size == UVec2::ZERO {
        return;
    }
    for (mut cam, right) in &mut cameras {
        cam.viewport = Some(pane_viewport(&rect, compare.enabled, right));
        if right {
            cam.is_active = compare.enabled;
        }
    }
}

/// The right pane's camera takes the main camera's transform, so both
/// panes orbit together from the one [`OrbitCamera`] state.
pub fn sync_compare_camera(
    main: Query<&Transform, (With<OrbitCamera>, Without<ComparePane>)>,
    mut right: Query<&mut Transform, With<ComparePane>>,
) {
    let Some(t) = main.iter().next() else { return };
    for mut r in &mut right {
        if *r != *t {
            *r = *t;
        }
    }
}

/// Arrow keys orbit: left/right yaw, up/down pitch. W/A/S/D move the
/// target across the ground relative to the view, Space raises it and left
/// Shift lowers it.
pub fn orbit_keys(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut q: Query<&mut OrbitCamera>,
) {
    // Real elapsed time, capped so a stalled frame doesn't jump the camera.
    let dt = time.delta_secs().min(0.05);
    let axis = |pos: KeyCode, neg: KeyCode| {
        f32::from(u8::from(keys.pressed(pos))) - f32::from(u8::from(keys.pressed(neg)))
    };
    let (right, forward, up) = (
        axis(KeyCode::KeyD, KeyCode::KeyA),
        axis(KeyCode::KeyW, KeyCode::KeyS),
        axis(KeyCode::Space, KeyCode::ShiftLeft),
    );
    if right != 0.0 || forward != 0.0 || up != 0.0 {
        for mut o in &mut q {
            let speed = o.distance * FLY_SPEED * dt;
            o.fly(right * speed, forward * speed, up * speed);
        }
    }
    let step = KEY_SPEED * dt;
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
            let per = match scroll.unit {
                MouseScrollUnit::Line => ZOOM_PER_LINE,
                MouseScrollUnit::Pixel => ZOOM_PER_PIXEL,
            };
            o.zoom((-scroll.delta.y * per).exp());
        }
    }
}

pub fn apply_orbit(mut q: Query<(&OrbitCamera, &mut Transform), Changed<OrbitCamera>>) {
    for (o, mut t) in &mut q {
        *t = o.transform();
    }
}

/// What decides how the selections are drawn.
#[derive(SystemParam)]
struct ShowInputs<'w> {
    loaded: Res<'w, Loaded>,
    right: Res<'w, Loaded<CompareSelected>>,
    compare: Res<'w, Compare>,
    overlays: Res<'w, Overlays>,
    mode: Res<'w, LightingMode>,
    textures: Res<'w, TextureMaterials>,
}

/// Show the selected model (or the model a selected definition draws) when
/// it changes, and in compare mode the right pane's on layer 1. A new main
/// model recentres the camera.
fn show_selection(
    input: ShowInputs,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut shown: ResMut<Shown>,
    mut cams: Query<&mut OrbitCamera>,
) {
    let ShowInputs {
        loaded,
        right,
        compare,
        overlays,
        mode,
        textures,
    } = input;
    let mut cx = ShowContext {
        commands: &mut commands,
        meshes: &mut meshes,
        labels: overlays.labels,
        mode: *mode,
        textures: &textures,
    };
    if (loaded.is_changed() || overlays.is_changed())
        && let Some(bounds) = cx.show(&mut shown, false, loaded.renderable.as_deref())
    {
        for mut o in &mut cams {
            o.frame(bounds);
        }
    }
    if right.is_changed() || overlays.is_changed() || compare.is_changed() {
        let r = right.renderable.as_deref().filter(|_| compare.enabled);
        cx.show(&mut shown, true, r);
    }
}

/// What spawning a shown model needs.
struct ShowContext<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    meshes: &'a mut Assets<Mesh>,
    labels: bool,
    mode: LightingMode,
    textures: &'a TextureMaterials,
}

impl ShowContext<'_, '_, '_> {
    /// Replace the main (or compare) model with `r`; returns its bounds.
    fn show(
        &mut self,
        shown: &mut Shown,
        right: bool,
        r: Option<&Renderable>,
    ) -> Option<(Vec3, Vec3)> {
        let slot = if right {
            &mut shown.compare
        } else {
            &mut shown.main
        };
        if let Some(e) = slot.entity.take() {
            self.commands.entity(e).despawn();
        }
        slot.parts.clear();
        let (client, real, blend) = renderable_parts(r?, self.labels);
        let bounds = real
            .iter()
            .filter_map(|p| mesh::bounds(&p.mesh))
            .map(|(lo, hi)| (Vec3::from(lo), Vec3::from(hi)))
            .reduce(|(a, b), (c, d)| (a.min(c), b.max(d)));
        let layer = RenderLayers::layer(if right { COMPARE_LAYER } else { 0 });
        let parent = self
            .commands
            .spawn((Transform::IDENTITY, Visibility::default(), layer.clone()))
            .id();
        let mut parts = Vec::new();
        for (c, r) in client.into_iter().zip(real) {
            let mut part = ShownPart {
                entity: Entity::PLACEHOLDER,
                texture: c.texture,
                client: self.meshes.add(c.mesh),
                real: self.meshes.add(r.mesh),
                blend,
            };
            let (mesh, mat) = handles(shown, &part, self.mode, self.textures);
            part.entity = self
                .commands
                .spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(mat),
                    Transform::IDENTITY,
                    layer.clone(),
                    ChildOf(parent),
                ))
                .id();
            parts.push(part);
        }
        let slot = if right {
            &mut shown.compare
        } else {
            &mut shown.main
        };
        *slot = ShownModel {
            entity: Some(parent),
            parts,
        };
        bounds
    }
}

/// A renderable's parts at rest; see [`posed_parts`].
pub fn renderable_parts(r: &Renderable, labels: bool) -> (Vec<mesh::Part>, Vec<mesh::Part>, bool) {
    posed_parts(r, labels, Pose::Rest)
}

/// Which vertices to draw a renderable with.
#[derive(Debug, Clone, Copy)]
pub enum Pose<'a> {
    Rest,
    /// Cache-space vertices of a cache model (an animation frame).
    Cache(&'a [[i32; 3]]),
    /// Pack-space vertices of a pack model (a skinned clip frame).
    Pack(&'a [[f32; 3]]),
}

/// A renderable's parts with client and raw shading (both shadings group
/// faces the same way, so the parts pair up), and whether it has face
/// alpha. A pose that does not fit the renderable draws it at rest.
pub fn posed_parts(
    r: &Renderable,
    labels: bool,
    pose: Pose,
) -> (Vec<mesh::Part>, Vec<mesh::Part>, bool) {
    match r {
        Renderable::CacheModel(m)
        | Renderable::DumpedModel(m)
        | Renderable::Creature { model: m, .. } => {
            let verts = match pose {
                Pose::Cache(v) if v.len() == m.vertices.len() => v,
                _ => &m.vertices,
            };
            let parts = |shading| {
                if labels {
                    let mesh = mesh::mesh_from_model(m, verts, shading, true);
                    vec![mesh::Part {
                        texture: None,
                        mesh,
                    }]
                } else {
                    mesh::model_parts(m, verts, shading)
                }
            };
            let blend = m
                .face_alphas
                .as_ref()
                .is_some_and(|a| a.iter().any(|&x| x != 0));
            (
                parts(Shading::Client(NPC_LIGHTING)),
                parts(Shading::Raw),
                blend,
            )
        }
        Renderable::PackModel { model, palette }
        | Renderable::PackCreature { model, palette, .. } => {
            let posed;
            let model = match pose {
                Pose::Pack(v) if v.len() == model.vertices.len() => {
                    posed = PackModel {
                        vertices: v.iter().map(|p| p.map(|c| c.round() as i16)).collect(),
                        ..model.clone()
                    };
                    &posed
                }
                _ => model,
            };
            let palette = palette.as_deref().map(Vec::as_slice);
            let part = |shading| {
                vec![mesh::Part {
                    texture: None,
                    mesh: mesh::mesh_from_pack_model(model, palette, shading),
                }]
            };
            let blend = model
                .alphas
                .as_ref()
                .is_some_and(|a| a.iter().any(|&x| x != 255));
            (
                part(Shading::Client(NPC_LIGHTING)),
                part(Shading::Raw),
                blend,
            )
        }
        // Drawn by the 2D viewport.
        _ => (Vec::new(), Vec::new(), false),
    }
}

/// A part's mesh and material for a lighting mode: its texture's material
/// once the textures are loaded, otherwise the plain one.
pub fn handles(
    shown: &Shown,
    part: &ShownPart,
    mode: LightingMode,
    textures: &TextureMaterials,
) -> (Handle<Mesh>, Handle<StandardMaterial>) {
    let (mesh, opaque, blend) = match mode {
        LightingMode::Client => (&part.client, &shown.unlit, &shown.unlit_blend),
        LightingMode::Real => (&part.real, &shown.lit, &shown.lit_blend),
    };
    let plain = || if part.blend { blend } else { opaque }.clone();
    let material = part
        .texture
        .and_then(|t| textures.get(t, mode))
        .unwrap_or_else(plain);
    (mesh.clone(), material)
}

/// Swap the parts' meshes and materials when the lighting mode changes or
/// the textures finish loading.
fn switch_lighting(
    mode: Res<LightingMode>,
    shown: Res<Shown>,
    textures: Res<TextureMaterials>,
    mut q: Query<(&mut Mesh3d, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    if !mode.is_changed() && !textures.is_changed() {
        return;
    }
    for part in shown.main.parts.iter().chain(&shown.compare.parts) {
        if let Ok((mut m, mut mat)) = q.get_mut(part.entity) {
            let (mesh, material) = handles(&shown, part, *mode, &textures);
            m.0 = mesh;
            mat.0 = material;
        }
    }
}

/// A material per texture and lighting mode, built once the classic view
/// has loaded the textures. Empty until then.
#[derive(Resource, Default)]
pub struct TextureMaterials {
    pub unlit: Vec<Option<Handle<StandardMaterial>>>,
    pub lit: Vec<Option<Handle<StandardMaterial>>>,
}

impl TextureMaterials {
    pub fn get(&self, id: u16, mode: LightingMode) -> Option<Handle<StandardMaterial>> {
        let list = match mode {
            LightingMode::Client => &self.unlit,
            LightingMode::Real => &self.lit,
        };
        list.get(usize::from(id)).cloned().flatten()
    }
}

/// A texture page's full-brightness copy as an image, sampled the way the
/// client samples it: u clamped, v repeating, nearest texel. Zero texels
/// are transparent unless the page is opaque.
pub fn texture_image(page: &TexturePage) -> Image {
    let data = page.texels[..TEXTURE_SIZE * TEXTURE_SIZE]
        .iter()
        .flat_map(|&t| {
            let [_, r, g, b] = t.to_be_bytes();
            let a = if page.opaque || t != 0 { 255 } else { 0 };
            [r, g, b, a]
        })
        .collect();
    let mut image = Image::new(
        Extent3d {
            width: TEXTURE_SIZE as u32,
            height: TEXTURE_SIZE as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::nearest()
    });
    image
}

const TEXTURE_SIZE: usize = 128;

fn build_texture_materials(
    view: Option<Res<ClassicView>>,
    mut built: Local<bool>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut out: ResMut<TextureMaterials>,
) {
    let Some(textures) = view
        .as_ref()
        .and_then(|v| v.textures.as_ref())
        .filter(|_| !*built)
    else {
        return;
    };
    *built = true;
    let (mut unlit, mut lit) = (Vec::new(), Vec::new());
    for page in &textures.pages {
        let Some(page) = page else {
            unlit.push(None);
            lit.push(None);
            continue;
        };
        let image = images.add(texture_image(page));
        let base = StandardMaterial {
            base_color_texture: Some(image),
            alpha_mode: if page.opaque {
                AlphaMode::Opaque
            } else {
                AlphaMode::Mask(0.5)
            },
            double_sided: true,
            cull_mode: None,
            ..default()
        };
        lit.push(Some(materials.add(StandardMaterial {
            perceptual_roughness: 1.0,
            ..base.clone()
        })));
        unlit.push(Some(materials.add(StandardMaterial {
            unlit: true,
            ..base
        })));
    }
    *out = TextureMaterials { unlit, lit };
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
        .init_resource::<TextureMaterials>()
        .init_resource::<Compare>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                follow_rect,
                orbit_keys.run_if(not(egui_wants_any_keyboard_input)),
                orbit_mouse
                    .run_if(not(egui_wants_any_pointer_input))
                    .run_if(not(crate::explorer::viewport2d::active)),
                apply_orbit,
                sync_compare_camera,
                follow_match,
                build_texture_materials,
                show_selection.in_set(ShowSet),
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
