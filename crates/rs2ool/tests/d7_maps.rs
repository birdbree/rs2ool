//! A region builds terrain for planes 0–2 and models for its locations,
//! including those on upper planes.

#![cfg(feature = "explorer")]

use std::path::PathBuf;

use rs2cache::{Cache, Records};
use rs2ool::explorer::map_view::{build, plane_mesh, world_image};

#[test]
fn lumbridge_has_upper_floors() {
    let Some(dir) = std::env::var_os("RS2_CACHE").map(PathBuf::from) else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let cache = Cache::open(&dir).unwrap();
    let records = Records::new(&cache);
    let region = cache.region(50, 50).unwrap();
    let built = build(&records, region).unwrap();
    for plane in 0..3 {
        let m = plane_mesh(&built.region, plane, &built.flos)
            .unwrap_or_else(|| panic!("plane {plane} has no terrain"));
        assert!(m.count_vertices() > 0);
    }
    let placed = |p: u8| {
        built
            .region
            .locations
            .iter()
            .filter(|l| l.plane == p && built.models.contains_key(&(l.id, l.kind)))
            .count()
    };
    eprintln!(
        "placed models per plane: {:?}; {} unique models",
        (0..4).map(placed).collect::<Vec<_>>(),
        built.models.len()
    );
    assert!(placed(0) > 1000);
    assert!(placed(1) > 0 && placed(2) > 0, "upper floors have objects");

    let world = world_image(&records).unwrap();
    assert_eq!(world.regions.len(), 660);
    assert!(world.rgba.chunks(4).filter(|p| p[..3] != [0, 0, 0]).count() > 660 * 1000);
}
