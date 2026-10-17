//! A model's mesh has a vertex per face corner, finite normals and the
//! model's bounds (y up); arrow keys orbit; switching lighting swaps
//! prebuilt meshes without building new ones.

#![cfg(feature = "explorer")]

use std::path::PathBuf;
use std::time::Duration;

use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use rs2cache::{Cache, Records};
use rs2ool::explorer::mesh::{self, NPC_LIGHTING, Shading};
use rs2ool::explorer::viewport3d::{
    self, LightingMode, Limits, OrbitCamera, Shown, ShownPart, TextureMaterials,
};

mod common;

#[test]
fn npc_mesh_is_upright_with_a_vertex_per_corner() {
    let Some(dir) = std::env::var_os("RS2_CACHE").map(PathBuf::from) else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let cache = Cache::open(&dir).unwrap();
    let records = Records::new(&cache);
    let index = cache.frame_index().unwrap();
    let npc = common::walking_npc(&records);
    let creature = rs2convert::creature::npc(&records, &index, npc, &[]).unwrap();
    let m = &creature.model;
    for shading in [Shading::Raw, Shading::Client(NPC_LIGHTING)] {
        let mesh = mesh::mesh_from_model(m, &m.vertices, shading, false);
        assert_eq!(mesh.count_vertices(), m.faces.len() * 3);
        let Some(VertexAttributeValues::Float32x3(n)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!()
        };
        assert!(n.iter().flatten().all(|v| v.is_finite()));
        let (lo, hi) = mesh::bounds(&mesh).unwrap();
        let (mlo, mhi) = m.bounds().unwrap();
        assert_eq!((lo[0], hi[0]), (mlo[0] as f32, mhi[0] as f32));
        assert_eq!(
            (lo[1], hi[1]),
            (-mhi[1] as f32, -mlo[1] as f32),
            "y is negated"
        );
        // A standing character's feet are at the origin in the cache, its
        // body above them (negative y); after the flip it extends up.
        assert!(
            hi[1] > 0.0 && hi[1] > -lo[1],
            "the body is above its feet: y {} to {}",
            lo[1],
            hi[1]
        );
    }
    // A plain model too.
    let first = records
        .ids(rs2cache::types::record::RecordType::Model)
        .unwrap()[0];
    let m0 = cache.model(first).unwrap();
    assert_eq!(
        mesh::mesh_from_model(&m0, &m0.vertices, Shading::Raw, false).count_vertices(),
        m0.faces.len() * 3
    );
}

#[test]
fn arrow_keys_orbit() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            16,
        )))
        .add_systems(Update, viewport3d::orbit_keys);
    // The first update has no elapsed time.
    app.update();
    let cam = app.world_mut().spawn(OrbitCamera::default()).id();
    let before = *app.world().get::<OrbitCamera>(cam).unwrap();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ArrowLeft);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ArrowUp);
    app.update();
    let after = *app.world().get::<OrbitCamera>(cam).unwrap();
    assert!(after.yaw < before.yaw && after.pitch > before.pitch);
    let mut o = OrbitCamera::default();
    o.turn(0.0, 10.0);
    assert!(
        (o.pitch - viewport3d::PITCH_LIMIT).abs() < 1e-6,
        "pitch clamps at 85°"
    );
}

#[test]
fn wasd_flies_and_the_camera_stays_bounded() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            16,
        )))
        .add_systems(Update, viewport3d::orbit_keys);
    // The first update has no elapsed time.
    app.update();
    let mut start = OrbitCamera {
        yaw: 0.0,
        target: Vec3::splat(500.0),
        ..Default::default()
    };
    start.bound(Limits::around(Vec3::ZERO, Vec3::splat(1000.0), 0.0, 0.1));
    let cam = app.world_mut().spawn(start).id();
    let before = *app.world().get::<OrbitCamera>(cam).unwrap();
    {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.press(KeyCode::KeyW);
        keys.press(KeyCode::Space);
    }
    app.update();
    let after = *app.world().get::<OrbitCamera>(cam).unwrap();
    // At yaw 0 the eye is on +z looking towards −z.
    assert!(after.target.z < before.target.z, "W moves forward");
    assert!(after.target.y > before.target.y, "Space moves up");
    assert_eq!(after.yaw, before.yaw, "flying does not turn");
    {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release_all();
        keys.press(KeyCode::ShiftLeft);
    }
    app.update();
    let lower = *app.world().get::<OrbitCamera>(cam).unwrap();
    assert!(lower.target.y < after.target.y, "left Shift moves down");

    let mut o = after;
    o.fly(1e9, -1e9, -1e9);
    assert_eq!(
        o.target,
        Vec3::new(1000.0, 0.0, 1000.0),
        "target stays in the box"
    );
    o.zoom(1e-9);
    assert_eq!(o.distance, o.limits.near);
    o.zoom(1e9);
    assert_eq!(o.distance, o.limits.far);
    o.turn(0.0, -10.0);
    assert_eq!(o.pitch, 0.1, "pitch stays above the floor");
}

#[test]
fn lighting_switch_reuses_meshes() {
    let mut meshes = Assets::<Mesh>::default();
    let mut materials = Assets::<StandardMaterial>::default();
    let empty = || {
        Mesh::new(
            bevy::mesh::PrimitiveTopology::TriangleList,
            Default::default(),
        )
    };
    let mut part = |texture| ShownPart {
        entity: Entity::PLACEHOLDER,
        texture,
        client: meshes.add(empty()),
        real: meshes.add(empty()),
        blend: false,
    };
    let (plain, textured) = (part(None), part(Some(3)));
    let shown = Shown {
        unlit: materials.add(StandardMaterial::default()),
        lit: materials.add(StandardMaterial::default()),
        ..Default::default()
    };
    let count = meshes.len();
    let none = TextureMaterials::default();
    let (a, _) = viewport3d::handles(&shown, &plain, LightingMode::Client, &none);
    let (b, _) = viewport3d::handles(&shown, &plain, LightingMode::Real, &none);
    assert_ne!(a, b);
    assert_eq!((a, b), (plain.client.clone(), plain.real.clone()));
    assert_eq!(meshes.len(), count);

    // A textured part uses the plain material until its texture loads.
    let (_, before) = viewport3d::handles(&shown, &textured, LightingMode::Client, &none);
    assert_eq!(before, shown.unlit);
    let loaded = TextureMaterials {
        unlit: vec![
            None,
            None,
            None,
            Some(materials.add(StandardMaterial::default())),
        ],
        lit: Vec::new(),
    };
    let (_, after) = viewport3d::handles(&shown, &textured, LightingMode::Client, &loaded);
    assert_eq!(Some(after), loaded.unlit[3].clone());
}
