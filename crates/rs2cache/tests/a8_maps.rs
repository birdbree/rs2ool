//! Every region in `map_index` decodes with its tile file consumed exactly,
//! every location type is 0–22, and a region with bridge tiles is
//! spot-checked.

mod common;

use rs2cache::types::map::{REGION_SIZE, Region};

#[test]
fn every_region_decodes() {
    let Some(cache) = common::cache() else { return };
    let index = cache.map_index().unwrap();
    let (mut ok, mut locs) = (0, 0);
    let mut failures = Vec::new();
    for e in &index {
        let tiles = cache.read(4, e.tile_file.into()).unwrap();
        let locations = cache.read(4, e.location_file.into()).unwrap();
        let (_, used) = rs2cache::map::decode_tiles(e, &tiles).unwrap();
        assert_eq!(used, tiles.len(), "region {}_{} tile file", e.x(), e.y());
        let (_, used) = rs2cache::map::decode_locations(&locations).unwrap();
        assert_eq!(
            used,
            locations.len(),
            "region {}_{} location file",
            e.x(),
            e.y()
        );
        match cache.region_for(e) {
            Ok(r) => {
                assert!(
                    r.locations.iter().all(|l| l.kind <= 22),
                    "region {}_{} has a type over 22",
                    r.x,
                    r.y
                );
                locs += r.locations.len();
                ok += 1;
            }
            Err(err) => failures.push(err.to_string()),
        }
    }
    eprintln!(
        "{} regions in map_index; {ok} decoded, {locs} locations; failures {:?}",
        index.len(),
        &failures[..failures.len().min(10)]
    );
    assert!(failures.is_empty(), "{} regions failed", failures.len());
}

#[test]
fn region_with_bridges() {
    // The first region with locations on plane 0 and at least one tile on
    // plane 1 flagged as a bridge (flag 2).
    let Some(cache) = common::cache() else { return };
    let index = cache.map_index().unwrap();
    let bridged = |r: &Region| r.planes[1].flags.iter().filter(|&&f| f & 2 != 0).count();
    let r = index
        .iter()
        .filter_map(|e| cache.region_for(e).ok())
        .find(|r| r.locations.iter().any(|l| l.plane == 0) && bridged(r) > 0)
        .expect("no region with plane 0 locations and a bridge tile on plane 1");
    let plane0 = r.locations.iter().filter(|l| l.plane == 0).count();
    let bridges = bridged(&r);
    let heights = r.planes[0].heights.iter();
    let (lo, hi) = (heights.clone().min().unwrap(), heights.max().unwrap());
    eprintln!(
        "{}_{}: {} locations ({plane0} on plane 0), {bridges} bridge tiles on plane 1, plane 0 heights {lo}..{hi}",
        r.x,
        r.y,
        r.locations.len()
    );
    // Fetching the region by coordinates gives the same region.
    assert_eq!(cache.region(r.x, r.y).unwrap(), r);
    // Every plane is a full grid, and every location sits on one.
    assert_eq!(r.planes.len(), 4);
    for p in &r.planes {
        assert_eq!(p.heights.len(), REGION_SIZE * REGION_SIZE);
        assert_eq!(p.flags.len(), REGION_SIZE * REGION_SIZE);
    }
    assert!(
        r.locations.iter().all(|l| l.plane < 4
            && usize::from(l.x) < REGION_SIZE
            && usize::from(l.y) < REGION_SIZE)
    );
}
