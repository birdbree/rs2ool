//! Every region in `map_index` decodes with its tile file consumed exactly,
//! every location type is 0–22, and one region's planes are spot-checked.

mod common;

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
fn lumbridge() {
    let Some(cache) = common::cache() else { return };
    let r = cache.region(50, 50).unwrap();
    let plane0 = r.locations.iter().filter(|l| l.plane == 0).count();
    let bridges = r.planes[1].flags.iter().filter(|&&f| f & 2 != 0).count();
    let heights = r.planes[0].heights.iter();
    let (lo, hi) = (heights.clone().min().unwrap(), heights.max().unwrap());
    eprintln!(
        "50_50: {} locations ({plane0} on plane 0), {bridges} bridge tiles on plane 1, plane 0 heights {lo}..{hi}",
        r.locations.len()
    );
    assert!(plane0 > 0);
    assert!(bridges > 0);
}
