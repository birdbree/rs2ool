//! Phase D3 checks: the Man mesh has a vertex per face corner, finite
//! normals and the model's bounds (y up); arrow keys orbit; switching
//! lighting swaps prebuilt meshes without building new ones.

use std::path::PathBuf;

use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use rs2cache::{Cache, Records};
use rs2explorer::mesh::{self, NPC_LIGHTING, Shading};
use rs2explorer::viewport3d::{self, LightingMode, OrbitCamera, Shown};

#[test]
fn man_mesh_is_upright_with_a_vertex_per_corner() {
    let Some(dir) = std::env::var_os("RS2_CACHE").map(PathBuf::from) else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let cache = Cache::open(&dir).unwrap();
    let records = Records::new(&cache);
    let index = cache.frame_index().unwrap();
    let man = rs2convert::creature::npc(&records, &index, 1, &[]).unwrap();
    let m = &man.model;
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
        assert!(hi[1] > 150.0, "the head is up");
    }
    // Model 0 too.
    let m0 = cache.model(0).unwrap();
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
        .add_systems(Update, viewport3d::orbit_keys);
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
fn lighting_switch_reuses_meshes() {
    let mut meshes = Assets::<Mesh>::default();
    let empty = || {
        Mesh::new(
            bevy::mesh::PrimitiveTopology::TriangleList,
            Default::default(),
        )
    };
    let shown = Shown {
        client: meshes.add(empty()),
        real: meshes.add(empty()),
        ..Default::default()
    };
    let count = meshes.len();
    let (a, _) = viewport3d::handles(&shown, LightingMode::Client);
    let (b, _) = viewport3d::handles(&shown, LightingMode::Real);
    assert_ne!(a, b);
    assert_eq!((a, b), (shown.client.clone(), shown.real.clone()));
    assert_eq!(meshes.len(), count);
}
