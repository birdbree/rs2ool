//! A region builds terrain for planes 0–2 and models for its locations,
//! including those on upper planes.

#![cfg(feature = "explorer")]

use std::path::PathBuf;

use rs2cache::types::record::{Record, RecordType};
use rs2cache::{Cache, Records};
use rs2ool::explorer::map_view::{build, plane_mesh, world_image};

#[test]
fn region_with_upper_floors_builds_every_plane() {
    let Some(dir) = std::env::var_os("RS2_CACHE").map(PathBuf::from) else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let cache = Cache::open(&dir).unwrap();
    let records = Records::new(&cache);
    // The first region with locations on planes 0, 1 and 2, and floor
    // tiles on planes 1 and 2 to stand them on.
    let floored = |r: &rs2cache::types::map::Region, p: usize| {
        let p = &r.planes[p];
        p.underlays.iter().chain(&p.overlays).any(|&f| f != 0)
    };
    let region = records
        .ids(RecordType::Map)
        .unwrap()
        .into_iter()
        .find_map(|id| match records.decode(RecordType::Map, id) {
            Ok(Record::Map(r))
                if (0..3).all(|p| r.locations.iter().any(|l| l.plane == p))
                    && floored(&r, 1)
                    && floored(&r, 2) =>
            {
                Some(r)
            }
            _ => None,
        })
        .expect("a region with locations and floors on upper planes");
    let (rx, ry) = (region.x, region.y);
    let built = build(&records, region).unwrap();
    for plane in 0..3 {
        let m = plane_mesh(&built.region, plane, &built.flos)
            .unwrap_or_else(|| panic!("region {rx}_{ry}: plane {plane} has no terrain"));
        assert!(m.count_vertices() > 0);
    }
    // Each placed location has a model exactly when its definition draws
    // one for its type.
    for l in &built.region.locations {
        let Some(def) = built.defs.get(&l.id) else {
            continue;
        };
        let want = rs2convert::preview::location_model(&records, def, l.kind)
            .unwrap()
            .is_some();
        assert_eq!(
            built.models.contains_key(&(l.id, l.kind)),
            want,
            "location {} type {}",
            l.id,
            l.kind
        );
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
        "region {rx}_{ry}: placed models per plane: {:?}; {} unique models",
        (0..4).map(placed).collect::<Vec<_>>(),
        built.models.len()
    );
    assert!(placed(0) > 0, "the ground has objects");
    assert!(placed(1) > 0 && placed(2) > 0, "upper floors have objects");

    // The world image covers every region in the map index, and on
    // average each paints at least a quarter of its 64 × 64 tiles.
    let world = world_image(&records).unwrap();
    assert_eq!(world.regions.len(), records.map_index().unwrap().len());
    assert!(
        world.rgba.chunks(4).filter(|p| p[..3] != [0, 0, 0]).count()
            > world.regions.len() * 64 * 64 / 4
    );
}
