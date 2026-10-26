//! Structures found in the map: in region 50_50, a church, a smithy and a
//! castle come out with their rooms, doors and furnishings, read as what
//! they are by the rules and the learned trees (which also put the castle's
//! blocks on one site), and are dressed in stone and wood; the whole map
//! gives the same structures every time, and its yards.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use contentpack::PackReader;
use contentpack::records::structure::{self as rec, StructureRecord};
use rs2cache::Records;
use rs2cache::types::config::LocDef;
use rs2cache::types::map::Region;
use rs2cache::types::record::{Record, RecordType};
use rs2convert::blueprint;
use rs2convert::learn::Trees;
use rs2convert::material::{self, Material};
use rs2convert::profile::{self, Via};
use rs2convert::purpose::{self, Class};
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

#[test]
fn a_regions_buildings_read_as_what_they_are() {
    let Some(cache) = common::cache() else { return };
    let (regions, defs) = load(&cache, |x, y| {
        (49..=51).contains(&x) && (49..=51).contains(&y)
    });
    let found = structure::find(&regions, &defs);
    let classes: Vec<(Class, Option<String>)> = found
        .iter()
        .filter(|s| (s.origin.0 / 64, s.origin.1 / 64) == (50, 50))
        .map(|s| {
            let p = purpose::of(s);
            eprintln!(
                "{:?} {:?}: {:?} {:?}",
                s.origin, p.class.what, p.class.detail, p.class.evidence
            );
            (p.class.what, p.class.detail)
        })
        .collect();
    let has = |c: Class, d: Option<&str>| {
        classes
            .iter()
            .any(|(cc, dd)| *cc == c && dd.as_deref() == d)
    };
    assert!(has(Class::Castle, None));
    assert!(has(Class::Church, None));
    assert!(has(Class::Smithy, None));
    assert!(has(Class::House, None));
    assert!(has(Class::Shop, Some("general")));
    assert!(has(Class::Shop, Some("axes")));
}

#[test]
fn a_regions_buildings_are_dressed_in_their_materials() {
    let Some(cache) = common::cache() else { return };
    let records = Records::new(&cache);
    let (regions, defs) = load(&cache, |x, y| {
        (49..=51).contains(&x) && (49..=51).contains(&y)
    });
    let floors = records
        .ids(RecordType::Flo)
        .unwrap()
        .into_iter()
        .filter_map(|id| match records.decode(RecordType::Flo, id) {
            Ok(Record::Flo(d)) => Some((u16::try_from(id).ok()?, d)),
            _ => None,
        })
        .collect();
    let mut found = structure::find(&regions, &defs);
    material::dress(&mut found, &records, &defs, &floors);
    let inside: Vec<&Structure> = found
        .iter()
        .filter(|s| (s.origin.0 / 64, s.origin.1 / 64) == (50, 50))
        .collect();
    // Nearly every wall has a material.
    let walls: Vec<_> = inside
        .iter()
        .flat_map(|s| &s.walls)
        .filter(|w| w.kind == 0)
        .collect();
    let dressed = walls.iter().filter(|w| w.material.is_some()).count();
    assert!(
        dressed * 10 >= walls.len() * 9,
        "{dressed} of {}",
        walls.len()
    );
    // The church: stone walls, a wooden floor.
    let church = inside
        .iter()
        .find(|s| purpose::of(s).class.what == Class::Church)
        .unwrap();
    let stone = church
        .walls
        .iter()
        .filter(|w| w.material.is_some_and(|m| m.material == Material::Stone))
        .count();
    assert!(stone * 2 > church.walls.len());
    assert_eq!(
        church.rooms[0].floor.map(|f| f.material),
        Some(Material::Wood)
    );
}

#[test]
fn the_learned_trees_read_a_region_and_group_its_castle() {
    let Some(cache) = common::cache() else { return };
    let records = Records::new(&cache);
    let (regions, defs) = load(&cache, |x, y| {
        (49..=51).contains(&x) && (49..=51).contains(&y)
    });
    let floors = records
        .ids(RecordType::Flo)
        .unwrap()
        .into_iter()
        .filter_map(|id| match records.decode(RecordType::Flo, id) {
            Ok(Record::Flo(d)) => Some((u16::try_from(id).ok()?, d)),
            _ => None,
        })
        .collect();
    let mut found = structure::find(&regions, &defs);
    material::dress(&mut found, &records, &defs, &floors);
    let trees = Trees::built_in();
    let sites = trees.sites(&found);
    let at = |x: u16, y: u16| found.iter().position(|s| s.origin == (x, y)).unwrap();
    let class = |i: usize| trees.classify(&found[i]).0;
    // The church, the smithy and the two castle blocks.
    let (church, smithy, keep, tower) = (
        at(3238, 3204),
        at(3222, 3252),
        at(3203, 3206),
        at(3227, 3212),
    );
    assert_eq!(class(church), Class::Church);
    assert_eq!(class(smithy), Class::Smithy);
    assert_eq!((class(keep), class(tower)), (Class::Castle, Class::Castle));
    // The blocks are one site; the church is on its own.
    assert_eq!(sites[keep], sites[tower]);
    assert!(sites.iter().filter(|&&s| s == sites[church]).count() == 1);
}

#[test]
fn a_regions_profiles_cover_its_structures() {
    let Some(cache) = common::cache() else { return };
    let records = Records::new(&cache);
    let (regions, defs) = load(&cache, |x, y| {
        (49..=51).contains(&x) && (49..=51).contains(&y)
    });
    let mut found = structure::find(&regions, &defs);
    material::dress(&mut found, &records, &defs, &BTreeMap::new());
    let trees = Trees::built_in();
    let guesses: Vec<_> = found.iter().map(|s| trees.classify(s)).collect();
    let sites = trees.sites(&found);
    let profiles = profile::profiles(&found, &guesses, &sites);
    assert_eq!(
        profiles.iter().map(|p| p.structures).sum::<usize>(),
        found.len()
    );
    // The castle's two blocks share a site, and its floors stack.
    let castle = profiles.iter().find(|p| p.class == Class::Castle).unwrap();
    assert!(castle.on_sites >= 2);
    assert_eq!(castle.site_classes[0].what, Class::Castle);
    assert!(castle.planes.max >= 3);
    assert!(castle.links.iter().any(|l| l.via == Via::Above));
    // The same every time.
    let again = profile::profiles(&found, &guesses, &sites);
    assert_eq!(
        serde_json::to_string(&profiles).unwrap(),
        serde_json::to_string(&again).unwrap()
    );
}

#[test]
fn a_regions_structures_pack_into_blueprints_sites_and_profiles() {
    let Some(cache) = common::cache() else { return };
    let records = Records::new(&cache);
    let region = |x: u8, y: u8| (49..=51).contains(&x) && (49..=51).contains(&y);
    let found = blueprint::read(&records, &region).unwrap().structures;
    let trees = Trees::built_in();
    let w = blueprint::pack(&found, &trees, 0).unwrap();
    // The same bytes every time.
    assert_eq!(w.bake(), blueprint::pack(&found, &trees, 0).unwrap().bake());
    let reader = PackReader::from_bytes(&w.bake().1).unwrap();
    let mut blueprints = BTreeMap::new();
    let mut sites = Vec::new();
    let mut profiles = 0;
    for id in reader.ids() {
        match StructureRecord::decode(&reader.get(id).unwrap().unwrap()).unwrap() {
            StructureRecord::Blueprint(b) => {
                blueprints.insert(id, b);
            }
            StructureRecord::Site(s) => sites.push((id, s)),
            StructureRecord::Profile(_) => profiles += 1,
        }
    }
    assert_eq!(blueprints.len(), found.len());
    assert!(profiles > 0);
    // Every tile is inside its box, counted south from the north edge.
    for b in blueprints.values() {
        let tiles = b.rooms.iter().flat_map(|r| &r.tiles);
        assert!(tiles.clone().all(|t| t.x < b.width && t.z < b.depth));
    }
    // The castle's keep: its north-west tile on the ground floor is at
    // z 0, and both blocks name the one site, which lists them.
    let keep = rec::blueprint_id(0, 3203, 3206).unwrap();
    let tower = rec::blueprint_id(0, 3227, 3212).unwrap();
    let site = blueprints[&keep].site;
    assert_ne!(site, rec::NONE);
    assert_eq!(blueprints[&tower].site, site);
    let (_, s) = sites.iter().find(|(id, _)| *id == site).unwrap();
    let members: Vec<u32> = s.members.iter().map(|m| m.blueprint).collect();
    assert_eq!(members, vec![keep, tower]);
    // The tower block's north edge is 6 tiles south of the keep's.
    let t = s.members.iter().find(|m| m.blueprint == tower).unwrap();
    let k = s.members.iter().find(|m| m.blueprint == keep).unwrap();
    assert_eq!((k.x, k.z), (0, 0));
    assert_eq!((t.x, t.z), (24, 6));
}
