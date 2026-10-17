//! Phase D4 check: cache model 215 and its converted pack record resolve
//! to renderables from both sources, "Match" pairs them, they draw at the
//! same size, and a deliberately mis-scaled pack record does not; the
//! compare cameras share the one orbit state.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use rs2explorer::mesh;
use rs2explorer::renderable::{Renderable, find_match};
use rs2explorer::selection::{CompareSelected, Sel, Selected, load};
use rs2explorer::sources::{Source, Sources, open_cache, open_pack};
use rs2explorer::ui::ViewportRect;
use rs2explorer::viewport3d::{self, Compare, ComparePane, OrbitCamera, renderable_parts};
use rs2intermediate::record::RecordType;
use rs2pack::Kind;

/// The union of a renderable's raw-shaded part bounds.
fn bounds(r: &Renderable) -> (Vec3, Vec3) {
    let (_, real, _) = renderable_parts(r, false);
    real.iter()
        .filter_map(|p| mesh::bounds(&p.mesh))
        .map(|(lo, hi)| (Vec3::from(lo), Vec3::from(hi)))
        .reduce(|(a, b), (c, d)| (a.min(c), b.max(d)))
        .expect("bounds")
}

#[test]
fn cache_model_and_pack_record_compare() {
    let Some(dir) = std::env::var_os("RS2_CACHE").map(PathBuf::from) else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let cache = open_cache(&dir).unwrap();
    let Source::Cache { records, .. } = &cache else {
        unreachable!()
    };
    // Convert model 215 and the palette as `rs2conv` would.
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("d4_compare");
    let _ = std::fs::remove_dir_all(&out);
    let png = |_: &std::path::Path, _: u32, _: u32, _: &[u8]| Ok(());
    let models = rs2convert::run::models(records, 0, Some(215)).unwrap();
    let written = rs2convert::run::write(&models, &out, png).unwrap();
    rs2convert::run::write(&rs2convert::run::materials(records, 0).unwrap(), &out, png).unwrap();
    let (_, _, pack_path) = written.iter().find(|(k, ..)| *k == Kind::Models).unwrap();
    let sources = vec![cache, open_pack(pack_path).unwrap()];

    // Compare mode follows the selection to its converted version.
    {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Compare>()
            .init_resource::<Selected>()
            .init_resource::<CompareSelected>()
            .init_resource::<Sources>()
            .add_systems(Update, viewport3d::follow_match);
        app.world_mut().resource_mut::<Sources>().list =
            sources.iter().map(rs2explorer::sources::detach).collect();
        *app.world_mut().resource_mut::<Selected>() = Selected {
            source: Some(0),
            sel: Some(Sel::Record(RecordType::Model, 215)),
        };
        app.update();
        assert_eq!(app.world().resource::<CompareSelected>().0.sel, None, "off");
        app.world_mut().resource_mut::<Compare>().enabled = true;
        app.update();
        let right = &app.world().resource::<CompareSelected>().0;
        assert_eq!(
            (right.source, right.sel.clone()),
            (Some(1), Some(Sel::PackRecord(215)))
        );
    }

    // "Match" pairs them both ways.
    let cache_sel = Sel::Record(RecordType::Model, 215);
    assert_eq!(
        find_match(&sources, 0, &cache_sel),
        Some((1, Sel::PackRecord(215)))
    );
    assert_eq!(
        find_match(&sources, 1, &Sel::PackRecord(215)),
        Some((0, cache_sel.clone()))
    );

    let (_, left) = load(&sources[0], &cache_sel).unwrap();
    let (_, right) = load(&sources[1], &Sel::PackRecord(215)).unwrap();
    let (left, right) = (left.unwrap(), right.unwrap());
    let Renderable::PackModel { model, palette } = &right else {
        panic!("a pack model")
    };
    assert!(palette.is_some(), "the palette is found beside the pack");

    let (a, b) = (bounds(&left), bounds(&right));
    let err = (a.0 - b.0).abs().max((a.1 - b.1).abs()).max_element();
    assert!(err <= 1.0, "same size: cache {a:?} vs pack {b:?}");

    let mut doubled = model.clone();
    for v in &mut doubled.vertices {
        *v = v.map(|c| c.saturating_mul(2));
    }
    let wrong = Renderable::PackModel {
        model: doubled,
        palette: palette.clone(),
    };
    let c = bounds(&wrong);
    assert!(
        (c.1 - c.0).length() > 1.5 * (a.1 - a.0).length(),
        "a mis-scaled record is visibly larger: {c:?} vs {a:?}"
    );
}

#[test]
fn compare_cameras_share_the_orbit() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(16),
        ))
        .add_systems(
            Update,
            (
                viewport3d::orbit_keys,
                viewport3d::apply_orbit,
                viewport3d::sync_compare_camera,
            )
                .chain(),
        );
    let orbit = OrbitCamera::default();
    let main = app.world_mut().spawn((orbit, orbit.transform())).id();
    let right = app
        .world_mut()
        .spawn((ComparePane, Transform::IDENTITY))
        .id();
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ArrowLeft);
    app.update();
    let (m, r) = (
        *app.world().get::<Transform>(main).unwrap(),
        *app.world().get::<Transform>(right).unwrap(),
    );
    assert_ne!(m, orbit.transform(), "the input moved the camera");
    assert_eq!(m, r, "both panes look from the same place");
}

#[test]
fn compare_splits_the_viewport() {
    let rect = ViewportRect {
        position: UVec2::new(100, 20),
        size: UVec2::new(801, 400),
    };
    let full = viewport3d::pane_viewport(&rect, false, false);
    assert_eq!(
        (full.physical_position, full.physical_size),
        (rect.position, rect.size)
    );
    let l = viewport3d::pane_viewport(&rect, true, false);
    let r = viewport3d::pane_viewport(&rect, true, true);
    assert_eq!(l.physical_position, rect.position);
    assert_eq!(
        r.physical_position.x,
        l.physical_position.x + l.physical_size.x
    );
    assert_eq!(l.physical_size.x + r.physical_size.x, rect.size.x);
}
