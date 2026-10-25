//! Structures found in the map: in region 50_50, a church, a smithy and a
//! castle come out with their rooms, doors and furnishings; the whole map
//! gives the same structures every time, and its yards.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use rs2cache::Records;
use rs2cache::types::config::LocDef;
use rs2cache::types::map::Region;
use rs2cache::types::record::{Record, RecordType};
use rs2convert::structure::{self, Structure};

/// The regions `keep` picks, and the definition of every location placed
/// in them.
fn load(
    cache: &rs2cache::Cache,
    keep: impl Fn(u8, u8) -> bool,
) -> (Vec<Region>, BTreeMap<u16, LocDef>) {
    let records = Records::new(cache);
    let regions: Vec<Region> = cache
        .map_index()
        .unwrap()
        .iter()
        .filter(|e| keep(e.x(), e.y()))
        .filter_map(|e| cache.region_for(e).ok())
        .collect();
    let ids: BTreeSet<u16> = regions
        .iter()
        .flat_map(|r| r.locations.iter().map(|l| l.id))
        .collect();
    let defs = ids
        .into_iter()
        .filter_map(|id| match records.decode(RecordType::Loc, id.into()) {
            Ok(Record::Loc(d)) => Some((id, d)),
            _ => None,
        })
        .collect();
    (regions, defs)
}

fn names(s: &Structure) -> BTreeSet<&str> {
    s.objects.iter().filter_map(|o| o.name.as_deref()).collect()
}

#[test]
fn a_region_gives_its_buildings_whole() {
    let Some(cache) = common::cache() else { return };
    let (regions, defs) = load(&cache, |x, y| {
        (49..=51).contains(&x) && (49..=51).contains(&y)
    });
    let found = structure::find(&regions, &defs);
    let inside: Vec<&Structure> = found
        .iter()
        .filter(|s| (s.origin.0 / 64, s.origin.1 / 64) == (50, 50))
        .collect();
    for s in &inside {
        eprintln!(
            "{:?} {:?} planes {:?}: {} rooms, {} doors, {} openings, {} objects",
            s.origin,
            s.size,
            s.planes,
            s.rooms.len(),
            s.doors.len(),
            s.openings.len(),
            s.objects.len()
        );
    }
    assert!(inside.len() >= 6, "{} structures", inside.len());
    // A church: an altar among pews, in one room with closed doors to the
    // outside and no open edges.
    let church = inside
        .iter()
        .find(|s| names(s).is_superset(&BTreeSet::from(["Altar", "Church pew"])))
        .expect("a church");
    assert_eq!(church.rooms.len(), 1);
    assert!(
        church
            .doors
            .iter()
            .any(|d| !d.open && d.rooms.contains(&None))
    );
    // A smithy: furnaces and blacksmith's tools under one roof.
    assert!(
        inside
            .iter()
            .any(|s| names(s).is_superset(&BTreeSet::from(["Furnace", "Blacksmith's tools"])))
    );
    // A castle block: two floors, many rooms, joined by doors and stairs.
    let castle = inside.iter().max_by_key(|s| s.rooms.len()).unwrap();
    assert!(castle.planes.len() >= 2 && castle.rooms.len() >= 8);
    assert!(castle.doors.len() >= 8);
    // Every object's room is one of its structure's rooms, on its plane.
    for s in &found {
        for o in &s.objects {
            if let Some(r) = o.room {
                assert_eq!(s.rooms[r].plane, o.plane);
            }
        }
        for d in &s.doors {
            assert!(d.rooms.iter().flatten().all(|&r| r < s.rooms.len()));
        }
    }
}

#[test]
fn the_whole_map_gives_the_same_structures_every_time() {
    let Some(cache) = common::cache() else { return };
    let (regions, defs) = load(&cache, |_, _| true);
    let a = structure::find(&regions, &defs);
    let mut reversed = regions.clone();
    reversed.reverse();
    let b = structure::find(&reversed, &defs);
    eprintln!("{} structures", a.len());
    assert!(a.len() > 500);
    assert_eq!(a, b, "the order regions come in changes nothing");
    // Every roofed tile is in exactly one room.
    let roofed: usize = regions
        .iter()
        .map(|r| {
            r.planes
                .iter()
                .map(|p| {
                    p.flags
                        .iter()
                        .filter(|&&f| f & structure::ROOFED != 0)
                        .count()
                })
                .sum::<usize>()
        })
        .sum();
    let in_rooms: usize = a
        .iter()
        .flat_map(|s| &s.rooms)
        .filter(|r| r.roofed)
        .map(|r| r.tiles.len())
        .sum();
    assert_eq!(in_rooms, roofed);
    // Yards: walled gardens, courtyards and terraces on buildings, and
    // pens and unflagged rooms on their own.
    let with_yards = a
        .iter()
        .filter(|s| s.rooms.iter().any(|r| r.roofed) && s.rooms.iter().any(|r| !r.roofed))
        .count();
    let yards_alone = a
        .iter()
        .filter(|s| s.rooms.iter().all(|r| !r.roofed))
        .count();
    eprintln!("{with_yards} buildings with yards, {yards_alone} yards alone");
    assert!(with_yards > 100 && yards_alone > 50);
    for s in &a {
        for r in &s.rooms {
            if !r.roofed {
                assert!(
                    r.tiles.len() >= structure::YARD_MIN && r.tiles.len() <= structure::YARD_MAX
                );
            }
        }
    }
}
