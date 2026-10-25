//! Map regions: each plane's ground as the original client draws it
//! (built by `rs2classic::terrain`, shadows and all), and locations placed
//! as their models. A viewer only. One region is shown at a time, but its
//! ground is built stitched to its neighbours so the edges meet theirs.
//!
//! The first half is plain data and geometry (testable without Bevy). The second
//! builds a region off the UI thread, spawns it, keeps the camera inside
//! it and draws the N/W/E/S overlay. The last part is the world map: plane
//! 0 of every region as one image.
//!
//! Tile `(x, y)` of a region sits at world `(x · 128, −height, −y · 128)`:
//! the cache's y-down, z-along-tile-y space with y and z negated, so y is
//! up and north (tile +y) is −z. Heights are indexed `x * 64 + y`.

use bevy::mesh::Mesh;
use contentpack::palette::{DEFAULT_BRIGHTNESS, split};
use rs2cache::types::config::{FloDef, LocDef};
use rs2cache::types::map::{Location, REGION_SIZE, Region};
use rs2cache::types::sprite::SpriteSheet;
use rs2classic::terrain::{FloorColour, Ground, Shadows, Tiles, horizontal_radius};

/// World units per tile.
pub const TILE: f32 = 128.0;

/// The floors, by floor definition id: their colour terms for the ground,
/// and the flat colour each draws with on the world map.
///
/// On the world map a floor with an RGB colour uses it. A textured floor (water, for one)
/// has no RGB, so it uses its texture's average colour, as the original
/// client does wherever it can't draw the texture itself. Without that,
/// textured floors fell through to whatever was underneath.
#[derive(Debug, Clone, Default)]
pub struct Floors {
    colours: Vec<Option<[f32; 4]>>,
    /// Each floor's colour terms for the ground builder, by floor id.
    pub terrain: Vec<FloorColour>,
}

impl Floors {
    /// Builds the table from the floor definitions and the textures.
    pub fn new(flos: &[FloDef], textures: &BTreeMap<u8, SpriteSheet>) -> Self {
        let colours = flos
            .iter()
            .map(|f| {
                let rgb = f.rgb.or_else(|| {
                    let t = textures.get(&f.texture?)?;
                    Some(rs2classic::texture::average_colour(t, DEFAULT_BRIGHTNESS))
                })?;
                let [r, g, b] = split(rgb);
                Some([
                    f32::from(r) / 255.0,
                    f32::from(g) / 255.0,
                    f32::from(b) / 255.0,
                    1.0,
                ])
            })
            .collect();
        let terrain = flos.iter().map(FloorColour::new).collect();
        Self { colours, terrain }
    }

    /// Decodes every floor definition and texture and builds the table.
    /// A floor that won't decode is an error; a missing texture just leaves
    /// its floors without a colour.
    pub fn load(records: &Records) -> Result<Self, String> {
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
        let textures = records
            .ids(RecordType::Texture)
            .map_err(e)?
            .into_iter()
            .filter_map(|id| match records.decode(RecordType::Texture, id) {
                Ok(Record::Texture(t)) => Some((id as u8, t)),
                _ => None,
            })
            .collect();
        Ok(Self::new(&flos, &textures))
    }

    /// The colour of map floor id `id` (an underlay or overlay byte).
    /// `None` for 0 (no floor), an id past the table, or a floor with no
    /// colour at all.
    pub fn colour(&self, id: u8) -> Option<[f32; 4]> {
        // Map floor ids are 1-based; 0 means none.
        let i = usize::from(id).checked_sub(1)?;
        *self.colours.get(i)?
    }
}

/// The regions around one, as a 3 × 3 block: `[i][j]` is the region
/// `i − 1` east and `j − 1` north, `None` where there's none (and in the
/// centre, which is the region itself).
pub type Around = [[Option<Region>; 3]; 3];

/// Decodes the eight regions around `region`. One that isn't in the cache
/// or won't decode is left out.
pub fn neighbours(records: &Records, region: &Region) -> Around {
    let mut around: Around = Default::default();
    for (i, row) in around.iter_mut().enumerate() {
        for (j, slot) in row.iter_mut().enumerate() {
            let (Some(x), Some(y)) = (
                region.x.checked_add_signed(i as i8 - 1),
                region.y.checked_add_signed(j as i8 - 1),
            ) else {
                continue;
            };
            if (i, j) == (1, 1) {
                continue;
            }
            if let Ok(Record::Map(r)) =
                records.decode(RecordType::Map, u32::from(x) << 8 | u32::from(y))
            {
                *slot = Some(r);
            }
        }
    }
    around
}

/// How far into a neighbour a location can stand and still shade the
/// region's ground: its footprint, plus the one point the light reads
/// past each grid point.
const SHADOW_REACH: usize = 16;

/// Every location in the block that can shade the region's ground, with
/// its tile in block coordinates (the region's own tiles are 64–127):
/// regions in order of id, each in file order, so neighbouring regions
/// see the same walls win on their shared edge.
fn block_locations<'a>(
    region: &'a Region,
    around: &'a Around,
) -> impl Iterator<Item = ((usize, usize), &'a Location)> {
    let near = |v: usize| (REGION_SIZE - SHADOW_REACH..2 * REGION_SIZE + SHADOW_REACH).contains(&v);
    (0..3).flat_map(move |i| {
        (0..3).flat_map(move |j| {
            let r = if (i, j) == (1, 1) {
                Some(region)
            } else {
                around[i][j].as_ref()
            };
            r.into_iter().flat_map(move |r| {
                r.locations.iter().filter_map(move |l| {
                    let at = (
                        i * REGION_SIZE + usize::from(l.x),
                        j * REGION_SIZE + usize::from(l.y),
                    );
                    (near(at.0) && near(at.1)).then_some((at, l))
                })
            })
        })
    })
}

/// What a location type needs to cast its shadow: its footprint and, for
/// scenery, its model's horizontal radius.
#[derive(Debug, Clone, Copy)]
pub struct Caster {
    /// The definition's footprint, before turning.
    pub size: (u8, u8),
    /// The model's horizontal radius (0 for walls).
    pub radius: i32,
}

/// The locations in the block that cast a shadow on the region, by
/// `(id, type)`. The region's own are looked up in `defs` and `models`;
/// neighbours' are decoded here.
pub fn casters(
    records: &Records,
    region: &Region,
    around: &Around,
    defs: &BTreeMap<u16, LocDef>,
    models: &BTreeMap<(u16, u8), Model>,
) -> BTreeMap<(u16, u8), Caster> {
    let mut more: BTreeMap<u16, Option<LocDef>> = BTreeMap::new();
    let mut out = BTreeMap::new();
    for (_, l) in block_locations(region, around) {
        let key = (l.id, l.kind);
        if out.contains_key(&key) {
            continue;
        }
        let def = defs.get(&l.id).or_else(|| {
            more.entry(l.id)
                .or_insert_with(|| match records.decode(RecordType::Loc, l.id.into()) {
                    Ok(Record::Loc(d)) => Some(d),
                    _ => None,
                })
                .as_ref()
        });
        let Some(d) = def.filter(|d| d.casts_shadow) else {
            continue;
        };
        // Only scenery's shadow depends on its model.
        let radius = if matches!(l.kind, 10 | 11) {
            match models.get(&key) {
                Some(m) => horizontal_radius(&m.vertices),
                None => rs2convert::preview::location_model(records, d, l.kind)
                    .ok()
                    .flatten()
                    .map_or(0, |m| horizontal_radius(&m.vertices)),
            }
        } else {
            0
        };
        out.insert(
            key,
            Caster {
                size: (d.size_x, d.size_y),
                radius,
            },
        );
    }
    out
}

/// The shadow the block's locations cast, over the stitched block (see
/// [`stitched`]).
pub fn shadows(region: &Region, around: &Around, casters: &BTreeMap<(u16, u8), Caster>) -> Shadows {
    let mut s = Shadows::new(3 * REGION_SIZE);
    for (at, l) in block_locations(region, around) {
        if let Some(c) = casters.get(&(l.id, l.kind)) {
            s.add(l.plane, at, l.kind, l.rotation, c.size, c.radius);
        }
    }
    s
}

/// Each of the region's planes stitched together with its neighbours'.
pub fn stitched(region: &Region, around: &Around) -> Vec<Tiles> {
    region
        .planes
        .iter()
        .enumerate()
        .map(|(p, centre)| {
            let mut planes = [[None; 3]; 3];
            for (i, row) in around.iter().enumerate() {
                for (j, r) in row.iter().enumerate() {
                    planes[i][j] = r.as_ref().and_then(|r| r.planes.get(p));
                }
            }
            Tiles::stitched(centre, planes)
        })
        .collect()
}

/// Each plane's ground, blended, lit and shaped as the original client
/// draws it (see [`rs2classic::terrain`]), meeting its neighbours without
/// a seam; `None` for a plane with nothing on it.
pub fn grounds(tiles: &[Tiles], floors: &Floors, shadows: &Shadows) -> Vec<Option<Ground>> {
    tiles
        .iter()
        .zip(&shadows.planes)
        .map(|(t, s)| rs2classic::terrain::ground(t, &floors.terrain, Some(s)))
        .collect()
}

/// A plane's ground as meshes for the modern view, split by texture.
pub fn ground_parts(ground: &Ground) -> Vec<mesh::Part> {
    mesh::lit_model_parts(&ground.model, &ground.vertices)
}

/// Where a location sits: the centre of its footprint at the ground height
/// of its tile, and its turn `rotation × 90°` in radians. Without a
/// definition the footprint is one tile.
///
/// The caller applies the turn as a rotation of minus that angle about y,
/// which matches the classic scene's quarter turns once y and z are
/// negated.
pub fn placement(region: &Region, l: &Location, def: Option<&LocDef>) -> ([f32; 3], f32) {
    // Odd rotations turn the footprint sideways.
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
use rs2cache::types::model::Model;
use rs2cache::types::record::{Record, RecordType};

use crate::explorer::mesh;
use crate::explorer::selection::{Loaded, LoadedData, Sel, Selected};
use crate::explorer::sources::{Source, Sources};
use crate::explorer::ui::{Nav, ViewportRect};
use crate::explorer::viewport3d::{
    DrawLayer, DrawOrder, LightingMode, Limits, OrbitCamera, TextureMaterials,
};

/// Everything a region scene needs, computed on the task pool.
pub struct Built {
    /// The decoded region.
    pub region: Region,
    /// The colour each floor draws with.
    pub floors: Floors,
    /// Each plane's ground.
    pub ground: Vec<Option<Ground>>,
    /// The definition of each location id placed in the region that
    /// decodes.
    pub defs: BTreeMap<u16, LocDef>,
    /// The model each `(location id, type)` draws, where it draws one.
    pub models: BTreeMap<(u16, u8), Model>,
    /// Each of those lit with the object lighting, as both renderers draw
    /// them.
    pub lit: BTreeMap<(u16, u8), Arc<rs2classic::LitModel>>,
    /// The same scene prepared for the classic renderer.
    pub classic: Arc<ClassicScene>,
}

/// A region prepared for the classic renderer, in cache coordinates (y
/// down, z along the tile y axis).
pub struct ClassicScene {
    /// The ground of each plane that has any, with its plane.
    pub terrain: Vec<(u8, Ground)>,
    /// Each placed object, in file order.
    pub objects: Vec<SceneObject>,
    /// The region's middle at plane 0's ground height there, for the
    /// camera target.
    pub centre: [i32; 3],
}

/// One placed object in a [`ClassicScene`].
pub struct SceneObject {
    /// Its plane.
    pub plane: u8,
    /// The tile it stands on: its footprint's south-west corner.
    pub tile: (u8, u8),
    /// Its footprint in tiles, turned with it (east, north).
    pub size: (u8, u8),
    /// The lit model, shared by every placement of it.
    pub model: Arc<rs2classic::LitModel>,
    /// Its vertices, already turned and moved into place.
    pub vertices: Vec<[i32; 3]>,
}

/// Rotates a vertex by `steps` quarter turns the way the original client
/// turns a placed object: x' = z, z' = −x per step. Only the low two bits
/// of `steps` count.
fn quarter_turns(v: [i32; 3], steps: u8) -> [i32; 3] {
    let [mut x, y, mut z] = v;
    for _ in 0..steps & 3 {
        let t = x;
        x = z;
        z = -t;
    }
    [x, y, z]
}

/// Lights each distinct location model once with the object lighting, as
/// the original client lights scenery; placements share it.
pub fn light_models(
    models: &BTreeMap<(u16, u8), Model>,
) -> BTreeMap<(u16, u8), Arc<rs2classic::LitModel>> {
    models
        .iter()
        .map(|(k, m)| {
            (
                *k,
                Arc::new(rs2classic::light(m, &rs2classic::light::OBJECT)),
            )
        })
        .collect()
}

/// Prepares a built region for the classic renderer: each plane's ground
/// as it is, and the objects with their lit models ([`light_models`]).
pub fn classic_scene(
    region: &Region,
    ground: &[Option<Ground>],
    defs: &BTreeMap<u16, LocDef>,
    models: &BTreeMap<(u16, u8), Model>,
    lit: &BTreeMap<(u16, u8), Arc<rs2classic::LitModel>>,
) -> ClassicScene {
    let terrain = ground
        .iter()
        .enumerate()
        .filter_map(|(p, g)| Some((p as u8, g.clone()?)))
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
        // The footprint's centre: tile corner plus half its size (64 = half
        // a tile).
        let (ox, oz) = (
            i32::from(l.x) * 128 + i32::from(w) * 64,
            i32::from(l.y) * 128 + i32::from(h) * 64,
        );
        let verts = m.vertices.iter().map(|v| {
            let r = quarter_turns(*v, l.rotation);
            [r[0] + ox, r[1] + ground, r[2] + oz]
        });
        objects.push(SceneObject {
            plane: l.plane,
            tile: (l.x, l.y),
            size: (w, h),
            model: lm.clone(),
            vertices: verts.collect(),
        });
    }
    let half = (REGION_SIZE as i32 * 128) / 2;
    // Height at tile (32, 32).
    let mid = region.planes[0].heights[REGION_SIZE * REGION_SIZE / 2 + REGION_SIZE / 2];
    ClassicScene {
        terrain,
        objects,
        centre: [half, mid, half],
    }
}

/// Decodes what a region scene needs: the floors, the placed locations'
/// definitions and the model for each `(id, type)` placed. Blocking.
///
/// A floor that won't decode fails the build; a location that won't is
/// skipped.
pub fn build(records: &Records, region: Region) -> Result<Built, String> {
    let floors = Floors::load(records)?;
    let mut defs = BTreeMap::new();
    let mut models = BTreeMap::new();
    // Each distinct `(id, type)` once, however often it's placed.
    let wanted: BTreeSet<(u16, u8)> = region.locations.iter().map(|l| (l.id, l.kind)).collect();
    for (id, kind) in wanted {
        // Decode each definition once; skip ids that won't decode.
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
    // The neighbours only shape the ground: its edges, light and shadows.
    let around = neighbours(records, &region);
    let casters = casters(records, &region, &around, &defs, &models);
    let ground = grounds(
        &stitched(&region, &around),
        &floors,
        &shadows(&region, &around, &casters),
    );
    let lit = light_models(&models);
    let classic = Arc::new(classic_scene(&region, &ground, &defs, &models, &lit));
    Ok(Built {
        classic,
        region,
        floors,
        ground,
        defs,
        models,
        lit,
    })
}

/// Marks a spawned region entity with its plane.
#[derive(Component)]
pub struct RegionPart {
    /// The plane, 0–3, for the plane checkboxes.
    pub plane: u8,
}

/// The region viewer's state.
#[derive(Resource)]
pub struct RegionScene {
    /// The region on screen (`x << 8 | y`).
    pub region: Option<u16>,
    /// Which planes are shown; all four to start.
    pub planes: [bool; 4],
    // Everything spawned for the region, to despawn on the next one.
    entities: Vec<Entity>,
    // A region being built, with its id.
    pending: Option<(u16, Task<Result<Built, String>>)>,
    /// Why the last build failed, if it did.
    pub error: Option<String>,
    /// The region prepared for the classic renderer.
    pub classic: Option<Arc<ClassicScene>>,
}

impl RegionScene {
    /// Whether a region is still being built.
    pub fn is_loading(&self) -> bool {
        self.pending.is_some()
    }
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

/// Starts building the region when a map record is loaded, or clears the
/// scene when something else is.
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

/// One part of a location's model: its texture (if any), priority and
/// mesh.
type TexturedMesh = (Option<u16>, u8, Handle<Mesh>);

/// A region location part drawn with a texture; its material is the
/// texture's once the textures load.
#[derive(Component)]
pub struct RegionTexture(pub u16);

/// Spawns a finished region: terrain per plane, then every placed
/// location, and puts the camera above the middle, bounded to the region.
fn spawn(
    mut scene: ResMut<RegionScene>,
    textures: Res<TextureMaterials>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cams: Query<&mut OrbitCamera>,
) {
    // Peek at the task without flagging the scene as changed every frame.
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
    let unlit = materials.add(StandardMaterial {
        unlit: true,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    // The ground is lit already, so it's drawn unlit like the models.
    for (plane, g) in built.ground.iter().enumerate() {
        let Some(g) = g else { continue };
        for part in ground_parts(g) {
            let material = part
                .texture
                .and_then(|id| textures.get(id, LightingMode::Client))
                .unwrap_or_else(|| unlit.clone());
            let mut e = commands.spawn((
                Mesh3d(meshes.add(part.mesh)),
                MeshMaterial3d(material),
                DrawOrder::of(
                    plane as u8,
                    DrawLayer::Ground,
                    part.priority,
                    Transform::IDENTITY,
                ),
                Transform::IDENTITY,
                RegionPart { plane: plane as u8 },
            ));
            if let Some(id) = part.texture {
                e.insert(RegionTexture(id));
            }
            scene.entities.push(e.id());
        }
    }
    // Meshes per distinct model, shared by every placement of it. Regions
    // always use the 317 lighting: the object lighting the classic
    // renderer draws them with, smooth faces and all.
    let handles: BTreeMap<(u16, u8), Vec<TexturedMesh>> = built
        .lit
        .iter()
        .filter_map(|(k, lit)| Some((k, lit, built.models.get(k)?)))
        .map(|(k, lit, m)| {
            let parts = mesh::lit_model_parts(lit, &m.vertices);
            let parts = parts
                .into_iter()
                .map(|p| (p.texture, p.priority, meshes.add(p.mesh)))
                .collect();
            (*k, parts)
        })
        .collect();
    for l in &built.region.locations {
        let Some(parts) = handles.get(&(l.id, l.kind)) else {
            continue;
        };
        let (pos, rot) = placement(&built.region, l, built.defs.get(&l.id));
        let t =
            Transform::from_translation(Vec3::from(pos)).with_rotation(Quat::from_rotation_y(-rot));
        // Ground decorations lie on the ground, and go over it.
        let layer = if l.kind == 22 {
            DrawLayer::Decoration
        } else {
            DrawLayer::Object
        };
        for (texture, priority, h) in parts {
            let material = texture
                .and_then(|id| textures.get(id, LightingMode::Client))
                .unwrap_or_else(|| unlit.clone());
            let mut e = commands.spawn((
                Mesh3d(h.clone()),
                MeshMaterial3d(material),
                DrawOrder::of(l.plane, layer, *priority, t),
                t,
                RegionPart { plane: l.plane },
            ));
            if let Some(id) = texture {
                e.insert(RegionTexture(*id));
            }
            scene.entities.push(e.id());
        }
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
        // Middle of the region (world z is negative going north).
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

/// Gives textured parts spawned before the textures loaded their material.
fn refresh_textures(
    textures: Res<TextureMaterials>,
    mut q: Query<(&RegionTexture, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    if !textures.is_changed() {
        return;
    }
    for (t, mut mat) in &mut q {
        if let Some(h) = textures.get(t.0, LightingMode::Client) {
            mat.0 = h;
        }
    }
}

/// Shows and hides region parts to match the plane checkboxes.
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

/// The region `(dx, dy)` regions from `id` (north is +y), or `None` off
/// the 256 × 256 grid.
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
                // A 3 × 3 grid with N, W, E and S round an empty middle.
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
    // Stepping is a new selection, with Back history and a tree reveal.
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
    /// Width in pixels: 64 per region column.
    pub width: usize,
    /// Height in pixels: 64 per region row.
    pub height: usize,
    /// `width × height × 4` bytes, row by row from the top (north).
    pub rgba: Vec<u8>,
    /// Region coordinates of the image's top-left region: the lowest x
    /// and the highest y.
    pub origin: (u16, u16),
    /// Every region id that exists; the rest of the image is black.
    pub regions: BTreeSet<u16>,
}

/// Decodes every region and paints plane 0 into one image covering the
/// bounding box of all regions. Blocking and slow; the explorer runs it
/// once, as a task. Regions that won't decode stay black.
pub fn world_image(records: &Records) -> Result<WorldImage, String> {
    let e = |e: rs2cache::Error| e.to_string();
    let floors = Floors::load(records)?;
    let ids: Vec<u16> = records
        .ids(RecordType::Map)
        .map_err(e)?
        .into_iter()
        .map(|i| i as u16)
        .collect();
    // The bounding box of every region, in region coordinates.
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
        // The region's place in the box.
        let (rx, ry) = (usize::from(r.x - x0 as u8), usize::from(r.y - y0 as u8));
        for x in 0..REGION_SIZE {
            for y in 0..REGION_SIZE {
                let i = x * REGION_SIZE + y;
                let c = floors
                    .colour(p.overlays[i])
                    .or_else(|| floors.colour(p.underlays[i]))
                    .unwrap_or([0.0, 0.0, 0.0, 1.0]);
                // Flip vertically: image rows go down, tile y goes north.
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

/// Uploads the world image as tiles no larger than egui allows, each with
/// its offset in the image.
fn tiles(ctx: &egui::Context, img: &WorldImage) -> Vec<(egui::TextureHandle, egui::Vec2)> {
    let mut out = Vec::new();
    for ty in (0..img.height).step_by(MAX_TEXTURE) {
        for tx in (0..img.width).step_by(MAX_TEXTURE) {
            let (w, h) = (
                MAX_TEXTURE.min(img.width - tx),
                MAX_TEXTURE.min(img.height - ty),
            );
            // Copy the tile out row by row.
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

/// The world map window's state.
#[derive(Resource, Default)]
pub struct WorldMap {
    /// Whether the window is open (**View › World map**).
    pub open: bool,
    // The finished image, kept for the session.
    image: Option<Arc<WorldImage>>,
    // The build, while it runs.
    pending: Option<Task<Result<WorldImage, String>>>,
    /// The image cut into tiles within egui's 2048-pixel texture limit,
    /// with each tile's offset in the image.
    tiles: Vec<(egui::TextureHandle, egui::Vec2)>,
    // Why the build failed; it isn't retried.
    error: Option<String>,
}

/// The world map window: builds the image from the first open cache the
/// first time it's opened, draws it, outlines the region on screen, and
/// opens a region when you click it.
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
                    // Regions from the top-left, 64 points (one per tile)
                    // each.
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
                    // Click position back to region coordinates.
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

/// Adds the region viewer and the world map.
pub fn plugin(app: &mut App) {
    app.init_resource::<RegionScene>()
        .init_resource::<WorldMap>()
        .add_systems(
            Update,
            (start, spawn, refresh_textures, plane_visibility).chain(),
        )
        .add_systems(
            EguiPrimaryContextPass,
            (overlay, world_window).after(crate::explorer::ui::Layout),
        );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flo(id: u32, rgb: Option<u32>, texture: Option<u8>) -> FloDef {
        FloDef {
            id,
            rgb,
            texture,
            hide_underlay: false,
            occlude: true,
            name: None,
            secondary_rgb: None,
            unknown_opcodes: Vec::new(),
        }
    }

    #[test]
    fn textured_floors_take_their_texture_average() {
        let tex = SpriteSheet {
            max_width: 0,
            max_height: 0,
            palette: vec![0, 0x3050a0],
            sprites: Vec::new(),
        };
        let flos = [
            flo(0, Some(0x00ff00), None),
            flo(1, None, Some(7)),
            flo(2, None, Some(9)),
        ];
        let floors = Floors::new(&flos, &BTreeMap::from([(7, tex.clone())]));
        // Map ids are 1-based: 1 is flo 0.
        assert_eq!(floors.colour(1), Some([0.0, 1.0, 0.0, 1.0]));
        let want = split(rs2classic::texture::average_colour(
            &tex,
            DEFAULT_BRIGHTNESS,
        ))
        .map(|c| f32::from(c) / 255.0);
        assert_eq!(floors.colour(2), Some([want[0], want[1], want[2], 1.0]));
        // A texture that isn't loaded leaves the floor without a colour.
        assert_eq!(floors.colour(3), None);
        assert_eq!(floors.colour(0), None);
        assert_eq!(floors.colour(4), None);
    }

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
