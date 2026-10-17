//! Map regions as terrain: one mesh per plane from corner heights, tiles
//! coloured from their `flo.dat` underlay and overlay, and locations placed
//! as their models. A viewer only; neighbouring regions are not stitched.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Mesh, PrimitiveTopology};
use rs2intermediate::config::{FloDef, LocDef};
use rs2intermediate::map::{Location, Plane, REGION_SIZE, Region};
use rs2pack::palette::split;

/// World units per tile.
pub const TILE: f32 = 128.0;

fn flo_colour(flos: &[FloDef], id: u8) -> Option<[f32; 4]> {
    if id == 0 {
        return None;
    }
    let f = flos.get(usize::from(id) - 1)?;
    let c = f.rgb?;
    let [r, g, b] = split(c);
    Some([
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
        1.0,
    ])
}

/// World position of a tile corner (y up; north is −z).
pub fn corner(plane: &Plane, x: usize, y: usize) -> [f32; 3] {
    let (cx, cy) = (x.min(REGION_SIZE - 1), y.min(REGION_SIZE - 1));
    let h = plane.heights[cx * REGION_SIZE + cy];
    [x as f32 * TILE, -h as f32, -(y as f32) * TILE]
}

/// The terrain mesh of one plane, or `None` when the plane has no floor
/// tiles. Plane 0 draws every tile; upper planes only tiles with a floor.
pub fn plane_mesh(region: &Region, plane: usize, flos: &[FloDef]) -> Option<Mesh> {
    let p = region.planes.get(plane)?;
    let (mut pos, mut col) = (Vec::new(), Vec::new());
    for x in 0..REGION_SIZE {
        for y in 0..REGION_SIZE {
            let i = x * REGION_SIZE + y;
            let under = flo_colour(flos, p.underlays[i]);
            let over = flo_colour(flos, p.overlays[i]);
            if plane > 0 && under.is_none() && over.is_none() {
                continue;
            }
            let base = under.or(over).unwrap_or([0.25, 0.3, 0.2, 1.0]);
            // Full overlays cover the tile; shaped ones cover one triangle.
            let (first, second) = match (over, p.shapes[i]) {
                (Some(o), 0) => (o, o),
                (Some(o), _) => (o, base),
                (None, _) => (base, base),
            };
            let (a, b, c, d) = (
                corner(p, x, y),
                corner(p, x + 1, y),
                corner(p, x + 1, y + 1),
                corner(p, x, y + 1),
            );
            for (tri, colour) in [([a, b, c], first), ([a, c, d], second)] {
                for v in tri {
                    pos.push(v);
                    col.push(colour);
                }
            }
        }
    }
    if pos.is_empty() {
        return None;
    }
    let normals = pos
        .chunks(3)
        .flat_map(|t| {
            let e1 = [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]];
            let e2 = [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
            let n = [n[0] / l, n[1] / l, n[2] / l];
            [n, n, n]
        })
        .collect::<Vec<_>>();
    Some(
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col),
    )
}

/// Where a location sits: the centre of its footprint at the ground height
/// of its tile, rotated by `rotation × 90°`.
pub fn placement(region: &Region, l: &Location, def: Option<&LocDef>) -> ([f32; 3], f32) {
    let (w, h) = def.map_or((1, 1), |d| {
        if l.rotation & 1 == 1 {
            (d.size_y, d.size_x)
        } else {
            (d.size_x, d.size_y)
        }
    });
    let p = &region.planes[usize::from(l.plane)];
    let i = usize::from(l.x) * REGION_SIZE + usize::from(l.y);
    let x = (f32::from(l.x) + f32::from(w) / 2.0) * TILE;
    let z = -(f32::from(l.y) + f32::from(h) / 2.0) * TILE;
    (
        [x, -p.heights[i] as f32, z],
        f32::from(l.rotation) * std::f32::consts::FRAC_PI_2,
    )
}

// ---------------------------------------------------------------------
// Bevy side: build a region scene off the UI thread, spawn it, navigate.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use rs2cache::Records;
use rs2intermediate::model::Model;
use rs2intermediate::record::{Record, RecordType};

use crate::mesh::{self, NPC_LIGHTING, Shading};
use crate::selection::{Loaded, LoadedData, Sel, Selected};
use crate::sources::{Source, Sources};
use crate::ui::{Nav, ViewportRect};
use crate::viewport3d::{Limits, OrbitCamera};

/// Everything a region scene needs, computed on the task pool.
pub struct Built {
    pub region: Region,
    pub flos: Vec<FloDef>,
    pub defs: BTreeMap<u16, LocDef>,
    /// The model each `(location id, type)` draws, where it draws one.
    pub models: BTreeMap<(u16, u8), Model>,
    /// The same scene prepared for the classic renderer.
    pub classic: Arc<ClassicScene>,
}

/// A region prepared for the classic renderer, in cache coordinates (y
/// down, z along the tile y axis).
pub struct ClassicScene {
    /// One flat-RGB terrain model per plane, with its vertices.
    pub terrain: Vec<(u8, rs2render::LitModel, Vec<[i32; 3]>)>,
    /// Each placed object: its plane, lit model and world vertices.
    pub objects: Vec<(u8, Arc<rs2render::LitModel>, Vec<[i32; 3]>)>,
    /// The region's centre, for the camera target.
    pub centre: [i32; 3],
}

/// Rotate a vertex by `steps` quarter turns as the client places objects
/// (`Model.method473`: x' = z, z' = −x per step).
fn quarter_turns(v: [i32; 3], steps: u8) -> [i32; 3] {
    let [mut x, y, mut z] = v;
    for _ in 0..steps & 3 {
        let t = x;
        x = z;
        z = -t;
    }
    [x, y, z]
}

/// Prepare a built region for the classic renderer.
pub fn classic_scene(
    region: &Region,
    flos: &[FloDef],
    defs: &BTreeMap<u16, LocDef>,
    models: &BTreeMap<(u16, u8), Model>,
) -> ClassicScene {
    let mut terrain = Vec::new();
    for (pi, p) in region.planes.iter().enumerate() {
        let (mut verts, mut faces, mut rgb) = (Vec::new(), Vec::new(), Vec::new());
        for x in 0..REGION_SIZE {
            for y in 0..REGION_SIZE {
                let i = x * REGION_SIZE + y;
                let colour =
                    flo_colour(flos, p.overlays[i]).or_else(|| flo_colour(flos, p.underlays[i]));
                if pi > 0 && colour.is_none() {
                    continue;
                }
                let c = colour.unwrap_or([0.25, 0.3, 0.2, 1.0]);
                let packed = ((c[0] * 255.0) as u32) << 16
                    | ((c[1] * 255.0) as u32) << 8
                    | (c[2] * 255.0) as u32;
                let h = |cx: usize, cy: usize| {
                    p.heights[cx.min(REGION_SIZE - 1) * REGION_SIZE + cy.min(REGION_SIZE - 1)]
                };
                let base = verts.len() as u16;
                for (cx, cy) in [(x, y), (x + 1, y), (x + 1, y + 1), (x, y + 1)] {
                    verts.push([cx as i32 * 128, h(cx, cy), cy as i32 * 128]);
                }
                faces.push([base, base + 1, base + 2]);
                faces.push([base, base + 2, base + 3]);
                rgb.extend([packed, packed]);
            }
        }
        if !faces.is_empty() {
            terrain.push((pi as u8, rs2render::LitModel::flat_rgb(faces, rgb), verts));
        }
    }
    let lit: BTreeMap<(u16, u8), Arc<rs2render::LitModel>> = models
        .iter()
        .map(|(k, m)| (*k, Arc::new(rs2render::light(m, &rs2render::light::OBJECT))))
        .collect();
    let mut objects = Vec::new();
    for l in &region.locations {
        let (Some(m), Some(lm)) = (models.get(&(l.id, l.kind)), lit.get(&(l.id, l.kind))) else {
            continue;
        };
        let (w, h) = defs.get(&l.id).map_or((1, 1), |d| {
            if l.rotation & 1 == 1 {
                (d.size_y, d.size_x)
            } else {
                (d.size_x, d.size_y)
            }
        });
        let i = usize::from(l.x) * REGION_SIZE + usize::from(l.y);
        let ground = region.planes[usize::from(l.plane)].heights[i];
        let (ox, oz) = (
            i32::from(l.x) * 128 + i32::from(w) * 64,
            i32::from(l.y) * 128 + i32::from(h) * 64,
        );
        let verts = m.vertices.iter().map(|v| {
            let r = quarter_turns(*v, l.rotation);
            [r[0] + ox, r[1] + ground, r[2] + oz]
        });
        objects.push((l.plane, lm.clone(), verts.collect()));
    }
    let half = (REGION_SIZE as i32 * 128) / 2;
    let mid = region.planes[0].heights[REGION_SIZE * REGION_SIZE / 2 + REGION_SIZE / 2];
    ClassicScene {
        terrain,
        objects,
        centre: [half, mid, half],
    }
}

/// Decode what a region scene needs.
pub fn build(records: &Records, region: Region) -> Result<Built, String> {
    let e = |e: rs2cache::Error| e.to_string();
    let flos = records
        .ids(RecordType::Flo)
        .map_err(e)?
        .into_iter()
        .map(|id| match records.decode(RecordType::Flo, id) {
            Ok(Record::Flo(f)) => Ok(f),
            Ok(_) => unreachable!(),
            Err(x) => Err(e(x)),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut defs = BTreeMap::new();
    let mut models = BTreeMap::new();
    let wanted: BTreeSet<(u16, u8)> = region.locations.iter().map(|l| (l.id, l.kind)).collect();
    for (id, kind) in wanted {
        if let std::collections::btree_map::Entry::Vacant(v) = defs.entry(id) {
            if let Ok(Record::Loc(d)) = records.decode(RecordType::Loc, id.into()) {
                v.insert(d);
            } else {
                continue;
            }
        }
        if let Some(m) = rs2convert::preview::location_model(records, &defs[&id], kind)
            .map_err(|x| x.to_string())?
        {
            models.insert((id, kind), m);
        }
    }
    let classic = Arc::new(classic_scene(&region, &flos, &defs, &models));
    Ok(Built {
        classic,
        region,
        flos,
        defs,
        models,
    })
}

/// Marks a spawned region entity with its plane.
#[derive(Component)]
pub struct RegionPart {
    pub plane: u8,
}

#[derive(Resource)]
pub struct RegionScene {
    /// The region on screen (`x << 8 | y`).
    pub region: Option<u16>,
    pub planes: [bool; 4],
    entities: Vec<Entity>,
    pending: Option<(u16, Task<Result<Built, String>>)>,
    pub error: Option<String>,
    /// The region prepared for the classic renderer.
    pub classic: Option<Arc<ClassicScene>>,
}

impl Default for RegionScene {
    fn default() -> Self {
        Self {
            region: None,
            planes: [true; 4],
            entities: Vec::new(),
            pending: None,
            error: None,
            classic: None,
        }
    }
}

fn start(
    loaded: Res<Loaded>,
    selected: Res<Selected>,
    sources: Res<Sources>,
    mut scene: ResMut<RegionScene>,
    mut commands: Commands,
) {
    if !loaded.is_changed() {
        return;
    }
    let Some(data) = &loaded.data else { return };
    let LoadedData::Record { record, .. } = &**data else {
        return;
    };
    let Record::Map(region) = &**record else {
        // Something else is selected: clear the scene.
        for e in scene.entities.drain(..) {
            commands.entity(e).despawn();
        }
        scene.region = None;
        scene.pending = None;
        scene.classic = None;
        return;
    };
    let Some(Source::Cache { records, .. }) = selected.source.and_then(|s| sources.list.get(s))
    else {
        return;
    };
    let (records, region) = (records.clone(), region.clone());
    let id = region.id;
    let task = AsyncComputeTaskPool::get().spawn(async move { build(&records, region) });
    scene.pending = Some((id, task));
}

fn spawn(
    mut scene: ResMut<RegionScene>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cams: Query<&mut OrbitCamera>,
) {
    let Some((id, task)) = &mut scene.bypass_change_detection().pending else {
        return;
    };
    let Some(result) = block_on(poll_once(task)) else {
        return;
    };
    let id = *id;
    scene.pending = None;
    for e in scene.entities.drain(..) {
        commands.entity(e).despawn();
    }
    let built = match result {
        Ok(b) => b,
        Err(e) => {
            scene.error = Some(e);
            return;
        }
    };
    scene.error = None;
    scene.region = Some(id);
    scene.classic = Some(built.classic.clone());
    let material = materials.add(StandardMaterial {
        perceptual_roughness: 1.0,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let unlit = materials.add(StandardMaterial {
        unlit: true,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    for plane in 0..4u8 {
        if let Some(m) = plane_mesh(&built.region, plane.into(), &built.flos) {
            let e = commands
                .spawn((
                    Mesh3d(meshes.add(m)),
                    MeshMaterial3d(material.clone()),
                    Transform::IDENTITY,
                    RegionPart { plane },
                ))
                .id();
            scene.entities.push(e);
        }
    }
    let handles: BTreeMap<(u16, u8), Handle<Mesh>> = built
        .models
        .iter()
        .map(|(k, m)| {
            (
                *k,
                meshes.add(mesh::mesh_from_model(
                    m,
                    &m.vertices,
                    Shading::Client(NPC_LIGHTING),
                    false,
                )),
            )
        })
        .collect();
    for l in &built.region.locations {
        let Some(h) = handles.get(&(l.id, l.kind)) else {
            continue;
        };
        let (pos, rot) = placement(&built.region, l, built.defs.get(&l.id));
        let t =
            Transform::from_translation(Vec3::from(pos)).with_rotation(Quat::from_rotation_y(-rot));
        let e = commands
            .spawn((
                Mesh3d(h.clone()),
                MeshMaterial3d(unlit.clone()),
                t,
                RegionPart { plane: l.plane },
            ))
            .id();
        scene.entities.push(e);
    }
    let half = REGION_SIZE as f32 * TILE / 2.0;
    let extent = REGION_SIZE as f32 * TILE;
    // Plane 0's lowest ground (y up): the target never sinks below it, and
    // with the pitch held above the horizon neither does the eye.
    let floor = built.region.planes[0]
        .heights
        .iter()
        .map(|&h| -h as f32)
        .fold(f32::INFINITY, f32::min);
    for mut o in &mut cams {
        o.target = Vec3::new(half, 0.0, -half);
        o.distance = 7000.0;
        o.pitch = 0.9;
        o.bound(Limits {
            lo: Vec3::new(0.0, floor, -extent),
            hi: Vec3::new(extent, floor + 3000.0, 0.0),
            near: 300.0,
            far: 14_000.0,
            min_pitch: 0.08,
        });
    }
}

fn plane_visibility(scene: Res<RegionScene>, mut q: Query<(&RegionPart, &mut Visibility)>) {
    if !scene.is_changed() {
        return;
    }
    for (p, mut v) in &mut q {
        let want = if scene.planes[usize::from(p.plane)] {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *v != want {
            *v = want;
        }
    }
}

/// The region one step from `id` (north is +y).
pub fn neighbour(id: u16, dx: i32, dy: i32) -> Option<u16> {
    let (x, y) = (i32::from(id >> 8) + dx, i32::from(id & 0xff) + dy);
    ((0..256).contains(&x) && (0..256).contains(&y)).then_some((x << 8 | y) as u16)
}

/// The overlay over the viewport: region name, steps, plane toggles.
fn overlay(
    mut contexts: EguiContexts,
    mut scene: ResMut<RegionScene>,
    mut selected: ResMut<Selected>,
    mut nav: ResMut<Nav>,
    rect: Res<ViewportRect>,
    window: Single<&Window, With<PrimaryWindow>>,
) -> Result {
    let Some(id) = scene.region.or(scene.pending.as_ref().map(|p| p.0)) else {
        return Ok(());
    };
    let ctx = contexts.ctx_mut()?;
    let s = window.scale_factor();
    let pos = egui::pos2(
        rect.position.x as f32 / s + 8.0,
        rect.position.y as f32 / s + 8.0,
    );
    let mut go = None;
    egui::Area::new(egui::Id::new("region-overlay"))
        .fixed_pos(pos)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong(format!("region {}_{}", id >> 8, id & 0xff));
                    if scene.pending.is_some() {
                        ui.spinner();
                    }
                });
                egui::Grid::new("steps").show(ui, |ui| {
                    ui.label("");
                    if ui.button("N").clicked() {
                        go = neighbour(id, 0, 1);
                    }
                    ui.label("");
                    ui.end_row();
                    if ui.button("W").clicked() {
                        go = neighbour(id, -1, 0);
                    }
                    ui.label("");
                    if ui.button("E").clicked() {
                        go = neighbour(id, 1, 0);
                    }
                    ui.end_row();
                    ui.label("");
                    if ui.button("S").clicked() {
                        go = neighbour(id, 0, -1);
                    }
                    ui.end_row();
                });
                ui.horizontal(|ui| {
                    ui.label("planes");
                    for p in 0..4 {
                        ui.checkbox(&mut scene.planes[p], p.to_string());
                    }
                });
                if let Some(e) = &scene.error {
                    ui.colored_label(egui::Color32::LIGHT_RED, e);
                }
            });
        });
    if let (Some(next), Some(si)) = (go, selected.source) {
        let old = std::mem::replace(
            &mut *selected,
            Selected {
                source: Some(si),
                sel: Some(Sel::Record(RecordType::Map, next.into())),
            },
        );
        nav.back.push(old);
        nav.reveal = Some((si, RecordType::Map, next.into()));
    }
    Ok(())
}

// ---------------------------------------------------------------------
// World map: every region's plane 0 as one overview image.

/// One pixel per tile, north up, from overlay (else underlay) colours.
pub struct WorldImage {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
    /// Region coordinates of the image's top-left region.
    pub origin: (u16, u16),
    pub regions: BTreeSet<u16>,
}

pub fn world_image(records: &Records) -> Result<WorldImage, String> {
    let e = |e: rs2cache::Error| e.to_string();
    let flos: Vec<FloDef> = records
        .ids(RecordType::Flo)
        .map_err(e)?
        .into_iter()
        .filter_map(|id| match records.decode(RecordType::Flo, id) {
            Ok(Record::Flo(f)) => Some(f),
            _ => None,
        })
        .collect();
    let ids: Vec<u16> = records
        .ids(RecordType::Map)
        .map_err(e)?
        .into_iter()
        .map(|i| i as u16)
        .collect();
    let (xs, ys): (Vec<u16>, Vec<u16>) = ids.iter().map(|i| (i >> 8, i & 0xff)).unzip();
    let (x0, x1) = (
        *xs.iter().min().unwrap_or(&0),
        *xs.iter().max().unwrap_or(&0),
    );
    let (y0, y1) = (
        *ys.iter().min().unwrap_or(&0),
        *ys.iter().max().unwrap_or(&0),
    );
    let (w, h) = (
        (usize::from(x1 - x0) + 1) * REGION_SIZE,
        (usize::from(y1 - y0) + 1) * REGION_SIZE,
    );
    let mut rgba = vec![0u8; w * h * 4];
    for &id in &ids {
        let Ok(Record::Map(r)) = records.decode(RecordType::Map, id.into()) else {
            continue;
        };
        let p = &r.planes[0];
        let (rx, ry) = (usize::from(r.x - x0 as u8), usize::from(r.y - y0 as u8));
        for x in 0..REGION_SIZE {
            for y in 0..REGION_SIZE {
                let i = x * REGION_SIZE + y;
                let c = flo_colour(&flos, p.overlays[i])
                    .or_else(|| flo_colour(&flos, p.underlays[i]))
                    .unwrap_or([0.0, 0.0, 0.0, 1.0]);
                let (px, py) = (rx * REGION_SIZE + x, h - 1 - (ry * REGION_SIZE + y));
                let o = (py * w + px) * 4;
                rgba[o..o + 4].copy_from_slice(&[
                    (c[0] * 255.0) as u8,
                    (c[1] * 255.0) as u8,
                    (c[2] * 255.0) as u8,
                    255,
                ]);
            }
        }
    }
    Ok(WorldImage {
        width: w,
        height: h,
        rgba,
        origin: (x0, y1),
        regions: ids.into_iter().collect(),
    })
}

/// egui's largest texture side.
const MAX_TEXTURE: usize = 2048;

/// Upload the world image as tiles no larger than egui allows.
fn tiles(ctx: &egui::Context, img: &WorldImage) -> Vec<(egui::TextureHandle, egui::Vec2)> {
    let mut out = Vec::new();
    for ty in (0..img.height).step_by(MAX_TEXTURE) {
        for tx in (0..img.width).step_by(MAX_TEXTURE) {
            let (w, h) = (
                MAX_TEXTURE.min(img.width - tx),
                MAX_TEXTURE.min(img.height - ty),
            );
            let mut rgba = Vec::with_capacity(w * h * 4);
            for y in ty..ty + h {
                rgba.extend_from_slice(
                    &img.rgba[(y * img.width + tx) * 4..(y * img.width + tx + w) * 4],
                );
            }
            let image = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
            let t = ctx.load_texture(
                format!("world-{tx}-{ty}"),
                image,
                egui::TextureOptions::NEAREST,
            );
            out.push((t, egui::vec2(tx as f32, ty as f32)));
        }
    }
    out
}

#[derive(Resource, Default)]
pub struct WorldMap {
    pub open: bool,
    image: Option<Arc<WorldImage>>,
    pending: Option<Task<Result<WorldImage, String>>>,
    /// The image cut into tiles within egui's 2048-pixel texture limit,
    /// with each tile's offset in the image.
    tiles: Vec<(egui::TextureHandle, egui::Vec2)>,
    error: Option<String>,
}

fn world_window(
    mut contexts: EguiContexts,
    mut world: ResMut<WorldMap>,
    sources: Res<Sources>,
    scene: Res<RegionScene>,
    mut selected: ResMut<Selected>,
    mut nav: ResMut<Nav>,
) -> Result {
    if !world.open {
        return Ok(());
    }
    let Some((si, records)) = sources.list.iter().enumerate().find_map(|(i, s)| match s {
        Source::Cache { records, .. } => Some((i, records.clone())),
        _ => None,
    }) else {
        return Ok(());
    };
    if world.image.is_none() && world.pending.is_none() && world.error.is_none() {
        world.pending =
            Some(AsyncComputeTaskPool::get().spawn(async move { world_image(&records) }));
    }
    if let Some(t) = &mut world.pending
        && let Some(r) = block_on(poll_once(t))
    {
        world.pending = None;
        match r {
            Ok(img) => world.image = Some(Arc::new(img)),
            Err(e) => world.error = Some(e),
        }
    }
    let ctx = contexts.ctx_mut()?.clone();
    let mut open = world.open;
    let mut go = None;
    egui::Window::new("World map")
        .open(&mut open)
        .default_size([700.0, 600.0])
        .show(&ctx, |ui| {
            if let Some(e) = &world.error {
                ui.colored_label(egui::Color32::LIGHT_RED, e);
                return;
            }
            let Some(img) = world.image.clone() else {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("decoding every region…");
                });
                return;
            };
            if world.tiles.is_empty() {
                world.tiles = tiles(&ctx, &img);
            }
            let tiles = world.tiles.clone();
            ui.label("Click a region to open it.");
            egui::ScrollArea::both().show(ui, |ui| {
                let size = egui::vec2(img.width as f32, img.height as f32);
                let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
                for (tex, offset) in &tiles {
                    let r = egui::Rect::from_min_size(rect.min + *offset, tex.size_vec2());
                    ui.painter().image(
                        tex.id(),
                        r,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }
                // Outline the region on screen.
                if let Some(id) = scene.region {
                    let (rx, ry) = (
                        f32::from(id >> 8) - f32::from(img.origin.0),
                        f32::from(img.origin.1) - f32::from(id & 0xff),
                    );
                    let r = egui::Rect::from_min_size(
                        resp.rect.min + egui::vec2(rx * 64.0, ry * 64.0),
                        egui::vec2(64.0, 64.0),
                    );
                    ui.painter().rect_stroke(
                        r,
                        0.0,
                        egui::Stroke::new(2.0, egui::Color32::YELLOW),
                        egui::StrokeKind::Inside,
                    );
                }
                if resp.clicked()
                    && let Some(p) = resp.interact_pointer_pos()
                {
                    let d = p - resp.rect.min;
                    let rx = img.origin.0 + (d.x / 64.0) as u16;
                    let ry = img.origin.1.saturating_sub((d.y / 64.0) as u16);
                    let id = rx << 8 | ry;
                    if img.regions.contains(&id) {
                        go = Some(id);
                    }
                }
            });
        });
    world.open = open;
    if let Some(id) = go {
        let old = std::mem::replace(
            &mut *selected,
            Selected {
                source: Some(si),
                sel: Some(Sel::Record(RecordType::Map, id.into())),
            },
        );
        nav.back.push(old);
        nav.reveal = Some((si, RecordType::Map, id.into()));
    }
    Ok(())
}

pub fn plugin(app: &mut App) {
    app.init_resource::<RegionScene>()
        .init_resource::<WorldMap>()
        .add_systems(Update, (start, spawn, plane_visibility).chain())
        .add_systems(
            EguiPrimaryContextPass,
            (overlay, world_window).after(crate::ui::Layout),
        );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_tiles_stay_within_the_texture_limit() {
        let ctx = egui::Context::default();
        let img = WorldImage {
            width: 2112,
            height: 7808,
            rgba: vec![0; 2112 * 7808 * 4],
            origin: (0, 0),
            regions: BTreeSet::new(),
        };
        let tiles = tiles(&ctx, &img);
        assert_eq!(tiles.len(), 2 * 4);
        let area: f32 = tiles
            .iter()
            .map(|(t, _)| t.size_vec2().x * t.size_vec2().y)
            .sum();
        assert_eq!(area, 2112.0 * 7808.0);
        assert!(
            tiles
                .iter()
                .all(|(t, _)| t.size()[0] <= MAX_TEXTURE && t.size()[1] <= MAX_TEXTURE)
        );
    }

    #[test]
    fn neighbours() {
        assert_eq!(neighbour(50 << 8 | 50, 1, 0), Some(51 << 8 | 50));
        assert_eq!(neighbour(50 << 8 | 50, 0, -1), Some(50 << 8 | 49));
        assert_eq!(neighbour(0, -1, 0), None);
    }
}
