//! A region builds ground for planes 0–2 and models for its locations,
//! including those on upper planes. The ground is blended and lit: its
//! corners take many more colours than the region has floors.

#![cfg(feature = "explorer")]

use std::path::PathBuf;

use rs2cache::types::record::{Record, RecordType};
use rs2cache::{Cache, Records};
use rs2ool::explorer::map_view::{build, ground_parts, world_image};

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
    for plane in 0..4 {
        // Ground exactly where a floor shows: any underlay, or an overlay
        // that isn't the invisible floor.
        let p = &built.region.planes[plane];
        let shows = |&(&u, &o): &(&u8, &u8)| {
            u != 0
                || usize::from(o)
                    .checked_sub(1)
                    .and_then(|f| built.floors.terrain.get(f))
                    .is_some_and(|f| !f.hidden)
        };
        let visible = p.underlays.iter().zip(&p.overlays).any(|t| shows(&t));
        assert_eq!(
            built.ground[plane].is_some(),
            visible,
            "region {rx}_{ry}: plane {plane}"
        );
        let Some(g) = &built.ground[plane] else {
            continue;
        };
        let parts = ground_parts(g);
        assert!(parts.iter().map(|p| p.mesh.count_vertices()).sum::<usize>() > 0);
        // Every corner of a gouraud face is a 16-bit HSL colour.
        let m = &g.model;
        let plain = (0..m.faces.len()).filter(|&f| m.kinds[f] == 0);
        assert!(
            plain
                .clone()
                .all(|f| m.colours[f].iter().all(|c| (0..65536).contains(c)))
        );
        if plane == 0 {
            let floors: std::collections::BTreeSet<u8> = built.region.planes[0]
                .underlays
                .iter()
                .chain(&built.region.planes[0].overlays)
                .copied()
                .collect();
            let colours: std::collections::BTreeSet<i32> =
                plain.flat_map(|f| m.colours[f]).collect();
            assert!(
                colours.len() > 4 * floors.len(),
                "region {rx}_{ry}: {} corner colours from {} floors",
                colours.len(),
                floors.len()
            );
        }
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

/// Two neighbouring regions, built apart, meet without a seam: the light
/// and the heights along their shared edge agree, both east–west and
/// south–north.
#[test]
fn neighbouring_regions_meet_without_a_seam() {
    let Some(dir) = std::env::var_os("RS2_CACHE").map(PathBuf::from) else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let cache = Cache::open(&dir).unwrap();
    let records = Records::new(&cache);
    let region =
        |x: u8, y: u8| match records.decode(RecordType::Map, u32::from(x) << 8 | u32::from(y)) {
            Ok(Record::Map(r)) => Some(r),
            _ => None,
        };
    // The first region with neighbours to the east and the north, all
    // three with ground on plane 0.
    let has_ground =
        |r: &rs2cache::types::map::Region| r.planes[0].underlays.iter().any(|&u| u != 0);
    let (a, east, north) = records
        .ids(RecordType::Map)
        .unwrap()
        .into_iter()
        .find_map(|id| {
            let a = region((id >> 8) as u8, id as u8).filter(has_ground)?;
            let east = region(a.x + 1, a.y).filter(has_ground)?;
            let north = region(a.x, a.y + 1).filter(has_ground)?;
            Some((a, east, north))
        })
        .expect("a region with east and north neighbours");
    let (ax, ay) = (a.x, a.y);
    let build = |r| build(&records, r).unwrap();
    let (a, east, north) = (build(a), build(east), build(north));
    let ground = |b: &rs2ool::explorer::map_view::Built| b.ground[0].clone().unwrap();
    let (ga, ge, gn) = (ground(&a), ground(&east), ground(&north));
    const G: usize = rs2classic::terrain::GRID;
    for k in 0..G {
        assert_eq!(
            ga.light[64 * G + k],
            ge.light[k],
            "region {ax}_{ay} east edge, point {k}"
        );
        assert_eq!(
            ga.light[k * G + 64],
            gn.light[k * G],
            "region {ax}_{ay} north edge, point {k}"
        );
    }
    // Every vertex on a shared edge has a twin at the same height across
    // it (where the other side draws a tile there).
    let edge = |g: &rs2classic::terrain::Ground, axis: usize, at: i32| {
        let other = 2 - axis;
        g.vertices
            .iter()
            .filter(|p| p[axis] == at)
            .map(|p| (p[other], p[1]))
            .collect::<std::collections::BTreeMap<i32, i32>>()
    };
    for (mine, theirs) in [
        (edge(&ga, 0, 64 * 128), edge(&ge, 0, 0)),
        (edge(&ga, 2, 64 * 128), edge(&gn, 2, 0)),
    ] {
        for (along, h) in &mine {
            if let Some(t) = theirs.get(along) {
                assert_eq!(h, t, "region {ax}_{ay}: height at {along} along the edge");
            }
        }
    }
}
